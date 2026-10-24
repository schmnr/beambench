//! Read-only desktop library protocol. Authentication and transport are Phase 2.
use serde::{Deserialize, Serialize};

pub const LIBRARY_SCHEMA_VERSION: u32 = 1;
pub const LIBRARY_MAX_PAGE_SIZE: usize = 50;
pub const CRAFTGINEER_MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryProvider {
    Craftgineer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LibraryMimeType {
    #[serde(rename = "image/svg+xml")]
    Svg,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/webp")]
    Webp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryAccess {
    Active,
    SubscriptionRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAccount {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCapabilities {
    pub mime_types: Vec<LibraryMimeType>,
    pub max_file_bytes: u64,
    pub max_page_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAccountResponse {
    pub schema_version: u32,
    pub provider: LibraryProvider,
    pub account: LibraryAccount,
    pub access: LibraryAccess,
    pub capabilities: LibraryCapabilities,
}

impl LibraryAccountResponse {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != LIBRARY_SCHEMA_VERSION {
            return Err("unsupported_library_version");
        }
        if !valid_id(&self.account.id) {
            return Err("invalid_library_account_id");
        }
        if self.capabilities.max_file_bytes == 0
            || self.capabilities.max_file_bytes > CRAFTGINEER_MAX_FILE_BYTES
            || self.capabilities.max_page_size == 0
            || self.capabilities.max_page_size > LIBRARY_MAX_PAGE_SIZE
            || self.capabilities.mime_types.is_empty()
        {
            return Err("invalid_library_capabilities");
        }
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryCollection {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFile {
    pub id: String,
    pub name: String,
    pub mime_type: LibraryMimeType,
    pub size_bytes: u64,
    pub revision: String,
    pub source_tool: Option<String>,
    pub created_at: String,
    pub collection: Option<LibraryCollection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryListingResponse {
    pub schema_version: u32,
    pub provider: LibraryProvider,
    pub account_id: String,
    pub files: Vec<LibraryFile>,
    pub next_cursor: Option<String>,
}

impl LibraryListingResponse {
    /// Bind a response to the authorized account before showing or importing it.
    pub fn validate_for(&self, account_id: &str) -> Result<(), &'static str> {
        if self.schema_version != LIBRARY_SCHEMA_VERSION {
            return Err("unsupported_library_version");
        }
        if self.account_id != account_id {
            return Err("library_account_mismatch");
        }
        if self.files.len() > LIBRARY_MAX_PAGE_SIZE {
            return Err("library_page_too_large");
        }
        for file in &self.files {
            if !valid_id(&file.id) {
                return Err("invalid_library_file_id");
            }
            if file.size_bytes == 0 || file.size_bytes > CRAFTGINEER_MAX_FILE_BYTES {
                return Err("library_file_too_large");
            }
            let Some(hash) = file.revision.strip_prefix("sha256:") else {
                return Err("invalid_library_revision");
            };
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("invalid_library_revision");
            }
            if chrono::DateTime::parse_from_rfc3339(&file.created_at).is_err() {
                return Err("invalid_library_timestamp");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryErrorCode {
    InvalidRequest,
    InvalidToken,
    InsufficientScope,
    SubscriptionRequired,
    NotFound,
    RevisionConflict,
    FileTooLarge,
    UnsupportedMediaType,
    RateLimited,
    ServiceUnavailable,
}

impl LibraryErrorCode {
    pub fn http_status(&self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::InvalidToken => 401,
            Self::InsufficientScope | Self::SubscriptionRequired => 403,
            Self::NotFound => 404,
            Self::RevisionConflict => 412,
            Self::FileTooLarge => 413,
            Self::UnsupportedMediaType => 415,
            Self::RateLimited => 429,
            Self::ServiceUnavailable => 503,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryError {
    pub code: LibraryErrorCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryErrorResponse {
    pub schema_version: u32,
    pub error: LibraryError,
}
