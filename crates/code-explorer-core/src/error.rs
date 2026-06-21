use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Invalid node label: {0}")]
    InvalidNodeLabel(String),

    #[error("Invalid relationship type: {0}")]
    InvalidRelationshipType(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
