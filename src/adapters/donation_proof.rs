use async_trait::async_trait;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Duration};
use tokio::process::Command;

use crate::{
    infrastructure::config::SuiConfig,
    port::sui::{
        DonationProofError, DonationProofPublisher, DonationProofReceipt, DonationProofRequest,
    },
};

const SUI_CLIENT_ENV_ALIAS: &str = "korede-backend";

#[derive(Debug, Clone)]
pub struct SuiDonationProofPublisher {
    config: SuiConfig,
    client_config_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DisabledDonationProofPublisher;

#[derive(Debug, Deserialize)]
struct SuiCallResponse {
    digest: Option<String>,
    effects: Option<SuiEffects>,
}

#[derive(Debug, Deserialize)]
struct SuiEffects {
    status: SuiExecutionStatus,
    transaction_digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SuiExecutionStatus {
    status: String,
    error: Option<String>,
}

impl SuiDonationProofPublisher {
    pub fn is_configured(config: &SuiConfig) -> bool {
        config.package_id.is_some()
            && config.admin_address.is_some()
            && config.keystore_path.is_some()
    }

    pub fn from_config(config: &SuiConfig) -> Result<Self, DonationProofError> {
        if config.package_id.is_none() {
            return Err(DonationProofError::MissingConfig("SUI_PACKAGE_ID"));
        }
        if config.admin_address.is_none() {
            return Err(DonationProofError::MissingConfig("SUI_ADMIN_ADDRESS"));
        }
        if config.keystore_path.is_none() {
            return Err(DonationProofError::MissingConfig("SUI_KEYSTORE_PATH"));
        }

        let keystore_path = config
            .keystore_path
            .as_ref()
            .ok_or(DonationProofError::MissingConfig("SUI_KEYSTORE_PATH"))?;
        let client_config_path = PathBuf::from(keystore_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("korede-sui-client.yaml");

        Ok(Self {
            config: config.clone(),
            client_config_path,
        })
    }

    fn hash_identifier(value: &str) -> Vec<u8> {
        Sha256::digest(value.as_bytes()).to_vec()
    }

    async fn ensure_client_environment(&self) -> Result<(), DonationProofError> {
        let keystore_path = self
            .config
            .keystore_path
            .as_ref()
            .ok_or(DonationProofError::MissingConfig("SUI_KEYSTORE_PATH"))?;
        let admin_address = self
            .config
            .admin_address
            .as_ref()
            .ok_or(DonationProofError::MissingConfig("SUI_ADMIN_ADDRESS"))?;

        let config_path = self.client_config_path.to_string_lossy().to_string();
        let keystore_literal = normalize_windows_path(keystore_path);

        run_sui_command(
            &[
                "client",
                "new-env",
                "--alias",
                SUI_CLIENT_ENV_ALIAS,
                "--rpc",
                &self.config.rpc_url,
                "--json",
            ],
            &config_path,
            Some(("SUI_KEYSTORE_PATH", keystore_literal.as_str())),
            self.config.request_timeout_seconds,
            true,
        )
        .await?;

        run_sui_command(
            &[
                "client",
                "switch",
                "--env",
                SUI_CLIENT_ENV_ALIAS,
                "--address",
                admin_address,
                "--json",
            ],
            &config_path,
            Some(("SUI_KEYSTORE_PATH", keystore_literal.as_str())),
            self.config.request_timeout_seconds,
            false,
        )
        .await?;

        Ok(())
    }
}

#[async_trait]
impl DonationProofPublisher for SuiDonationProofPublisher {
    async fn publish_donation_proof(
        &self,
        request: DonationProofRequest,
    ) -> Result<DonationProofReceipt, DonationProofError> {
        self.ensure_client_environment().await?;

        let package_id = self
            .config
            .package_id
            .as_deref()
            .ok_or(DonationProofError::MissingConfig("SUI_PACKAGE_ID"))?;
        let admin_address = self
            .config
            .admin_address
            .as_deref()
            .ok_or(DonationProofError::MissingConfig("SUI_ADMIN_ADDRESS"))?;
        let keystore_path = self
            .config
            .keystore_path
            .as_deref()
            .ok_or(DonationProofError::MissingConfig("SUI_KEYSTORE_PATH"))?;

        let case_hash = hex_string(&Self::hash_identifier(&request.case_id));
        let hospital_hash = hex_string(&Self::hash_identifier(&request.hospital_id));
        let reference_hash = hex_string(&Self::hash_identifier(&request.payment_reference));
        let gas_budget = self.config.gas_budget.to_string();
        let config_path = self.client_config_path.to_string_lossy().to_string();
        let keystore_literal = normalize_windows_path(keystore_path);

        let output = run_sui_command(
            &[
                "client",
                "call",
                "--package",
                package_id,
                "--module",
                "korede_donations",
                "--function",
                "record_donation",
                "--args",
                admin_address,
                &format!("0x{case_hash}"),
                &format!("0x{hospital_hash}"),
                &request.amount_kobo.to_string(),
                &format!("0x{reference_hash}"),
                &self.config.clock_object_id,
                "--gas-budget",
                &gas_budget,
                "--sender",
                admin_address,
                "--json",
            ],
            &config_path,
            Some(("SUI_KEYSTORE_PATH", keystore_literal.as_str())),
            self.config.request_timeout_seconds,
            false,
        )
        .await?;

        let response: SuiCallResponse = serde_json::from_str(&output)
            .map_err(|_| DonationProofError::Provider("invalid Sui CLI response".to_owned()))?;

        if let Some(effects) = response.effects {
            if effects.status.status != "success" {
                return Err(DonationProofError::Provider(
                    effects
                        .status
                        .error
                        .unwrap_or_else(|| "Sui transaction failed".to_owned()),
                ));
            }

            let tx_digest = effects
                .transaction_digest
                .or(response.digest)
                .ok_or_else(|| DonationProofError::Provider("missing Sui digest".to_owned()))?;

            return Ok(DonationProofReceipt {
                network: self.config.network.clone(),
                tx_digest,
            });
        }

        let tx_digest = response
            .digest
            .ok_or_else(|| DonationProofError::Provider("missing Sui digest".to_owned()))?;

        Ok(DonationProofReceipt {
            network: self.config.network.clone(),
            tx_digest,
        })
    }
}

#[async_trait]
impl DonationProofPublisher for DisabledDonationProofPublisher {
    async fn publish_donation_proof(
        &self,
        _request: DonationProofRequest,
    ) -> Result<DonationProofReceipt, DonationProofError> {
        Err(DonationProofError::MissingConfig(
            "SUI_PACKAGE_ID, SUI_ADMIN_ADDRESS, or SUI_KEYSTORE_PATH",
        ))
    }
}

async fn run_sui_command(
    args: &[&str],
    client_config_path: &str,
    env_override: Option<(&str, &str)>,
    timeout_seconds: u64,
    tolerate_existing_env_error: bool,
) -> Result<String, DonationProofError> {
    let mut command = Command::new("sui");
    command.args(args);
    command.arg("--client.config").arg(client_config_path);
    command.arg("--yes");

    if let Some((key, value)) = env_override {
        command.env(key, value);
    }

    let output = tokio::time::timeout(Duration::from_secs(timeout_seconds), command.output())
        .await
        .map_err(|_| DonationProofError::Provider("timed out waiting for Sui CLI".to_owned()))?
        .map_err(|_| DonationProofError::PublishFailed)?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if tolerate_existing_env_error
        && (stderr.contains("Alias already exists")
            || stderr.contains("environment config with this alias already exists"))
    {
        return Ok(String::new());
    }

    Err(DonationProofError::Provider(if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    } else {
        stderr
    }))
}

fn normalize_windows_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn hex_string(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{hex_string, SuiDonationProofPublisher};

    #[test]
    fn hash_identifier_is_sha256_bytes() {
        let hashed = SuiDonationProofPublisher::hash_identifier("case-123");
        assert_eq!(hashed.len(), 32);
    }

    #[test]
    fn sui_publisher_reports_configured_only_when_required_values_exist() {
        let mut config = crate::infrastructure::config::SuiConfig {
            network: "testnet".to_owned(),
            rpc_url: "https://fullnode.testnet.sui.io:443".to_owned(),
            package_id: None,
            admin_address: None,
            keystore_path: None,
            gas_budget: 10_000_000,
            clock_object_id: "0x6".to_owned(),
            request_timeout_seconds: 30,
        };

        assert!(!SuiDonationProofPublisher::is_configured(&config));
        config.package_id = Some("0xpackage".to_owned());
        config.admin_address = Some("0xadmin".to_owned());
        config.keystore_path = Some("C:/keys/sui.keystore".to_owned());
        assert!(SuiDonationProofPublisher::is_configured(&config));
    }

    #[test]
    fn normalize_windows_path_uses_forward_slashes() {
        assert_eq!(
            super::normalize_windows_path(r"C:\keys\sui.keystore"),
            "C:/keys/sui.keystore"
        );
    }

    #[test]
    fn hex_string_encodes_bytes() {
        assert_eq!(hex_string(&[0x0a, 0xff, 0x10]), "0aff10");
    }
}
