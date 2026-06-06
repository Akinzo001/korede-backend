use std::path::PathBuf;

use async_trait::async_trait;
use aws_credential_types::Credentials;
use aws_sdk_s3::{config::BehaviorVersion, primitives::ByteStream, Client as S3Client};
use aws_smithy_runtime_api::client::result::SdkError;
use aws_types::region::Region;
use sanitize_filename::sanitize;
use tokio::fs;
use uuid::Uuid;

use crate::{
    domain::hospital_document::HospitalDocumentType,
    infrastructure::config::BackblazeConfig,
    port::storage::{DocumentStorage, DocumentStorageError, StoredDocument},
};

#[derive(Debug, Clone)]
pub struct LocalDocumentStorage {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BackblazeDocumentStorage {
    client: S3Client,
    bucket: String,
}

impl LocalDocumentStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl BackblazeDocumentStorage {
    pub fn from_config(config: &BackblazeConfig) -> Result<Self, DocumentStorageError> {
        let bucket = config
            .bucket
            .clone()
            .ok_or(DocumentStorageError::MissingConfig("BACKBLAZE_BUCKET"))?;
        let endpoint = config
            .endpoint
            .clone()
            .ok_or(DocumentStorageError::MissingConfig("BACKBLAZE_ENDPOINT"))?;
        let access_key_id =
            config
                .access_key_id
                .clone()
                .ok_or(DocumentStorageError::MissingConfig(
                    "BACKBLAZE_ACCESS_KEY_ID",
                ))?;
        let secret_access_key =
            config
                .secret_access_key
                .clone()
                .ok_or(DocumentStorageError::MissingConfig(
                    "BACKBLAZE_SECRET_ACCESS_KEY",
                ))?;

        let credentials =
            Credentials::new(access_key_id, secret_access_key, None, None, "backblaze-b2");

        let sdk_config = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.region.clone()))
            .endpoint_url(endpoint)
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();

        Ok(Self {
            client: S3Client::from_conf(sdk_config),
            bucket,
        })
    }
}

#[async_trait]
impl DocumentStorage for LocalDocumentStorage {
    async fn save_document(
        &self,
        hospital_id: Uuid,
        document_type: HospitalDocumentType,
        original_filename: &str,
        mime_type: &str,
        contents: &[u8],
    ) -> Result<StoredDocument, DocumentStorageError> {
        let safe_original_filename = sanitize(original_filename);
        let extension = extension_for(mime_type, &safe_original_filename);
        let generated_filename = format!("{}.{}", Uuid::new_v4(), extension);
        let document_type_path = document_type.as_str();

        let relative_path = PathBuf::from("kyc")
            .join("hospitals")
            .join(hospital_id.to_string())
            .join(document_type_path)
            .join(generated_filename);

        let full_path = self.root.join(&relative_path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|_| DocumentStorageError::StoreFailed)?;
        }

        fs::write(&full_path, contents)
            .await
            .map_err(|_| DocumentStorageError::StoreFailed)?;

        Ok(StoredDocument {
            storage_provider: "local".to_owned(),
            storage_key: relative_path.to_string_lossy().replace('\\', "/"),
            original_filename: safe_original_filename,
            mime_type: mime_type.to_owned(),
            file_size_bytes: contents.len() as i64,
        })
    }

    async fn save_case_document(
        &self,
        hospital_id: Uuid,
        case_id: Uuid,
        document_type: &str,
        original_filename: &str,
        mime_type: &str,
        contents: &[u8],
    ) -> Result<StoredDocument, DocumentStorageError> {
        let safe_original_filename = sanitize(original_filename);
        let safe_document_type = sanitize(document_type);
        let extension = extension_for(mime_type, &safe_original_filename);
        let generated_filename = format!("{}.{}", Uuid::new_v4(), extension);

        let relative_path = PathBuf::from("cases")
            .join("hospitals")
            .join(hospital_id.to_string())
            .join(case_id.to_string())
            .join(safe_document_type)
            .join(generated_filename);

        let full_path = self.root.join(&relative_path);

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|_| DocumentStorageError::StoreFailed)?;
        }

        fs::write(&full_path, contents)
            .await
            .map_err(|_| DocumentStorageError::StoreFailed)?;

        Ok(StoredDocument {
            storage_provider: "local".to_owned(),
            storage_key: relative_path.to_string_lossy().replace('\\', "/"),
            original_filename: safe_original_filename,
            mime_type: mime_type.to_owned(),
            file_size_bytes: contents.len() as i64,
        })
    }
}

#[async_trait]
impl DocumentStorage for BackblazeDocumentStorage {
    async fn save_document(
        &self,
        hospital_id: Uuid,
        document_type: HospitalDocumentType,
        original_filename: &str,
        mime_type: &str,
        contents: &[u8],
    ) -> Result<StoredDocument, DocumentStorageError> {
        let safe_original_filename = sanitize(original_filename);
        let extension = extension_for(mime_type, &safe_original_filename);
        let generated_filename = format!("{}.{}", Uuid::new_v4(), extension);
        let storage_key = format!(
            "kyc/hospitals/{}/{}/{}",
            hospital_id,
            document_type.as_str(),
            generated_filename
        );

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&storage_key)
            .content_type(mime_type)
            .body(ByteStream::from(contents.to_vec()))
            .send()
            .await
            .map_err(|error| {
                log_backblaze_error(error);
                DocumentStorageError::StoreFailed
            })?;

        Ok(StoredDocument {
            storage_provider: "backblaze".to_owned(),
            storage_key,
            original_filename: safe_original_filename,
            mime_type: mime_type.to_owned(),
            file_size_bytes: contents.len() as i64,
        })
    }

    async fn save_case_document(
        &self,
        hospital_id: Uuid,
        case_id: Uuid,
        document_type: &str,
        original_filename: &str,
        mime_type: &str,
        contents: &[u8],
    ) -> Result<StoredDocument, DocumentStorageError> {
        let safe_original_filename = sanitize(original_filename);
        let safe_document_type = sanitize(document_type);
        let extension = extension_for(mime_type, &safe_original_filename);
        let generated_filename = format!("{}.{}", Uuid::new_v4(), extension);
        let storage_key = format!(
            "cases/hospitals/{}/{}/{}/{}",
            hospital_id, case_id, safe_document_type, generated_filename
        );

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&storage_key)
            .content_type(mime_type)
            .body(ByteStream::from(contents.to_vec()))
            .send()
            .await
            .map_err(|error| {
                log_backblaze_error(error);
                DocumentStorageError::StoreFailed
            })?;

        Ok(StoredDocument {
            storage_provider: "backblaze".to_owned(),
            storage_key,
            original_filename: safe_original_filename,
            mime_type: mime_type.to_owned(),
            file_size_bytes: contents.len() as i64,
        })
    }
}

fn log_backblaze_error<E, R>(error: SdkError<E, R>)
where
    E: std::fmt::Debug,
    R: std::fmt::Debug,
{
    tracing::error!(?error, "failed to upload document to Backblaze B2");
}

fn extension_for(mime_type: &str, original_filename: &str) -> String {
    match mime_type {
        "application/pdf" => "pdf".to_owned(),
        "image/jpeg" => "jpg".to_owned(),
        "image/png" => "png".to_owned(),
        "image/webp" => "webp".to_owned(),
        _ => original_filename
            .rsplit('.')
            .next()
            .filter(|extension| !extension.is_empty())
            .unwrap_or("bin")
            .to_owned(),
    }
}
