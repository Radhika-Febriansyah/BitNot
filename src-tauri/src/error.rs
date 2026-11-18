use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Data format error: {0}")]
    Json(#[from] serde_json::Error),
    /// The request was understood but is not allowed or not valid.
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error(
        "This data was created by a newer version of BitNot (schema {found}, this version supports up to {supported}). Update BitNot to open it."
    )]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("The database file {0} failed its integrity check. It has been left untouched.")]
    Corrupt(String),
    /// Startup failed; the message explains why.
    #[error("{0}")]
    Startup(String),
}

impl AppError {
    pub fn invalid(message: impl Into<String>) -> Self {
        AppError::Invalid(message.into())
    }

    fn kind(&self) -> &'static str {
        match self {
            AppError::Db(_) => "database",
            AppError::Io(_) => "io",
            AppError::Json(_) => "format",
            AppError::Invalid(_) => "invalid",
            AppError::NotFound(_) => "notFound",
            AppError::SchemaTooNew { .. } => "schemaTooNew",
            AppError::Corrupt(_) => "corrupt",
            AppError::Startup(_) => "startup",
        }
    }
}

/// Errors cross the IPC boundary as `{ kind, message }`.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}
