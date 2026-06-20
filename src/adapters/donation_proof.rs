use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    infrastructure::config::SuiConfig,
    port::sui::{
        DonationProofError, DonationProofPublisher, DonationProofReceipt, DonationProofRequest,
    },
};

#[derive(Debug, Clone)]
pub struct SuiDonationProofPublisher {
    client: Client,
    config: SuiConfig,
}

impl SuiDonationProofPublisher {
    pub fn from_config(config: &SuiConfig) -> Result<Self, DonationProofError> {
        if config.package_id.is_none() {
            return Err(DonationProofError::MissingConfig("SUI_PACKAGE_ID"));
        }

        if config.admin_address.is_none() {
            return Err(DonationProofError::MissingConfig("SUI_ADMIN_ADDRESS"));
        }

        Ok(Self {
            client: Client::new(),
            config: config.clone(),
        })
    }

    fn hash_identifier(value: &str) -> String {
        let digest = Sha256::digest(value.as_bytes());
        hex_string(&digest)
    }
}

#[async_trait]
impl DonationProofPublisher for SuiDonationProofPublisher {
    async fn publish_donation_proof(
        &self,
        request: DonationProofRequest,
    ) -> Result<DonationProofReceipt, DonationProofError> {
        let package_id = self
            .config
            .package_id
            .as_ref()
            .ok_or(DonationProofError::MissingConfig("SUI_PACKAGE_ID"))?;
        let admin_address = self
            .config
            .admin_address
            .as_ref()
            .ok_or(DonationProofError::MissingConfig("SUI_ADMIN_ADDRESS"))?;

        let case_hash = Self::hash_identifier(&request.case_id);
        let hospital_hash = Self::hash_identifier(&request.hospital_id);
        let reference_hash = Self::hash_identifier(&request.payment_reference);

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "suix_executeTransactionBlock",
            "params": [
                package_id,
                admin_address,
                case_hash,
                hospital_hash,
                request.amount_kobo,
                reference_hash,
                self.config.gas_budget
            ]
        });

        let response = self
            .client
            .post(&self.config.rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(|_| DonationProofError::PublishFailed)?;

        if !response.status().is_success() {
            return Err(DonationProofError::Provider(
                response.text().await.unwrap_or_default(),
            ));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|_| DonationProofError::PublishFailed)?;

        let tx_digest = body
            .get("result")
            .and_then(|result| result.get("digest"))
            .and_then(|digest| digest.as_str())
            .ok_or_else(|| DonationProofError::Provider("missing Sui digest".to_owned()))?;

        Ok(DonationProofReceipt {
            network: self.config.network.clone(),
            tx_digest: tx_digest.to_owned(),
        })
    }
}

fn hex_string(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}
