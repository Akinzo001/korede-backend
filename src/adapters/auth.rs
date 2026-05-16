use argon2::{
    Argon2,
    password_hash::{
        PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString, rand_core::OsRng,
    },
};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::port::auth::{
    AuthenticatedHospital, PasswordHashError, PasswordHasher, TokenError, TokenService,
};

#[derive(Debug, Clone)]
pub struct Argon2PasswordHasher;

impl PasswordHasher for Argon2PasswordHasher {
    fn hash_password(&self, password: &str) -> Result<String, PasswordHashError> {
        let salt = SaltString::generate(&mut OsRng);

        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| PasswordHashError::HashFailed)
    }

    fn verify_password(
        &self,
        password: &str,
        password_hash: &str,
    ) -> Result<bool, PasswordHashError> {
        let parsed_hash =
            PasswordHash::new(password_hash).map_err(|_| PasswordHashError::VerifyFailed)?;

        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }
}

#[derive(Debug, Clone)]
pub struct JwtTokenService {
    secret: String,
    expires_in_seconds: i64,
}

impl JwtTokenService {
    pub fn new(secret: String, expires_in_seconds: i64) -> Self {
        Self {
            secret,
            expires_in_seconds,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    role: String,
    iat: usize,
    exp: usize,
}

impl TokenService for JwtTokenService {
    fn create_access_token(&self, hospital_id: Uuid, email: &str) -> Result<String, TokenError> {
        let now = Utc::now().timestamp();
        let exp = now + self.expires_in_seconds;

        let claims = Claims {
            sub: hospital_id.to_string(),
            email: email.to_owned(),
            role: "hospital".to_owned(),
            iat: now as usize,
            exp: exp as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|_| TokenError::CreateFailed)
    }

    fn verify_access_token(&self, token: &str) -> Result<AuthenticatedHospital, TokenError> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| TokenError::Invalid)?;

        if token_data.claims.role != "hospital" {
            return Err(TokenError::Invalid);
        }

        let hospital_id =
            Uuid::parse_str(&token_data.claims.sub).map_err(|_| TokenError::Invalid)?;

        Ok(AuthenticatedHospital {
            hospital_id,
            email: token_data.claims.email,
            role: token_data.claims.role,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_hashes_and_verifies_passwords() {
        let hasher = Argon2PasswordHasher;
        let password_hash = hasher.hash_password("strong-password").unwrap();

        assert!(
            hasher
                .verify_password("strong-password", &password_hash)
                .unwrap()
        );
        assert!(
            !hasher
                .verify_password("wrong-password", &password_hash)
                .unwrap()
        );
    }

    #[test]
    fn jwt_service_creates_and_verifies_hospital_token() {
        let service = JwtTokenService::new("test-secret".to_owned(), 3600);
        let hospital_id = Uuid::new_v4();
        let token = service
            .create_access_token(hospital_id, "hospital@example.com")
            .unwrap();

        let authenticated = service.verify_access_token(&token).unwrap();

        assert_eq!(authenticated.hospital_id, hospital_id);
        assert_eq!(authenticated.email, "hospital@example.com");
        assert_eq!(authenticated.role, "hospital");
    }
}
