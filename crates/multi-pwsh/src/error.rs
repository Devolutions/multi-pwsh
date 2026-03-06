use std::io;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MultiPwshError>;

#[derive(Debug, Error)]
pub enum MultiPwshError {
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("http error: {0}")]
    Http(Box<ureq::Error>),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("archive error: {0}")]
    Archive(String),

    #[error("version parse error: {0}")]
    Version(#[from] semver::Error),

    #[error("invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("unsupported platform: {0}")]
    UnsupportedPlatform(String),

    #[error("release not found: {0}")]
    ReleaseNotFound(String),

    #[error("asset not found: {0}")]
    AssetNotFound(String),

    #[error("home directory not found")]
    HomeDirectoryNotFound,

    #[error("alias creation failed: {0}")]
    AliasCreation(String),

    #[error("host error: {0}")]
    Host(String),
}

impl From<zip::result::ZipError> for MultiPwshError {
    fn from(value: zip::result::ZipError) -> Self {
        MultiPwshError::Archive(value.to_string())
    }
}

impl From<ureq::Error> for MultiPwshError {
    fn from(value: ureq::Error) -> Self {
        MultiPwshError::Http(Box::new(value))
    }
}
