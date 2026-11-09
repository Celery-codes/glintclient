use serde::ser::SerializeStruct;

use crate::auth::AuthError;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("unexpected data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("checksum mismatch for {file} (expected {expected}, got {actual})")]
    Checksum {
        file: String,
        expected: String,
        actual: String,
    },
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("{0}")]
    Config(String),
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Java(String),
    #[error("{0}")]
    Launch(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Stable machine-readable code the UI can branch on.
    pub fn kind(&self) -> &'static str {
        match self {
            Error::Io(_) => "io",
            Error::Http(_) => "network",
            Error::Json(_) => "data",
            Error::Checksum { .. } => "checksum",
            Error::Auth(e) => e.kind(),
            Error::Config(_) => "config",
            Error::NotFound(_) => "not_found",
            Error::Java(_) => "java",
            Error::Launch(_) => "launch",
            Error::Other(_) => "other",
        }
    }
}

/// Errors cross the IPC boundary as `{ kind, message }`.
impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Error", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}
