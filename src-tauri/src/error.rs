use serde::Serialize;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum CleanerError {
    #[error("opencode is running: {0:?}")]
    ProcessRunning(Vec<String>),

    #[error("data directory not found: {0}")]
    DataDirNotFound(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("backup failed: {0}")]
    BackupFailed(String),

    #[error("restore failed: {0}")]
    RestoreFailed(String),

    #[error("insufficient disk space: need {needed} bytes, have {available} bytes")]
    InsufficientSpace { needed: u64, available: u64 },

    #[error("incompatible opencode schema: {0}")]
    IncompatibleSchema(String),

    #[error("invalid selection: {0}")]
    InvalidSelection(String),
}

impl Serialize for CleanerError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl From<io::Error> for CleanerError {
    fn from(e: io::Error) -> Self {
        CleanerError::Io(e.to_string())
    }
}

impl From<rusqlite::Error> for CleanerError {
    fn from(e: rusqlite::Error) -> Self {
        CleanerError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for CleanerError {
    fn from(e: serde_json::Error) -> Self {
        CleanerError::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CleanerError>;
