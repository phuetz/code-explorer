//! Error types for the MCP server.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum McpError {
    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Unknown tool: {0}")]
    UnknownTool(String),

    #[error("Invalid arguments for tool {tool}: {reason}")]
    InvalidArguments { tool: String, reason: String },

    #[error("Write query rejected: mutation queries are not allowed via MCP")]
    WriteQueryRejected,

    #[error("Repository not found: {0}")]
    RepoNotFound(String),

    #[error("Method not found: {0}")]
    MethodNotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Db(#[from] code_explorer_db::error::DbError),

    #[error(transparent)]
    Core(#[from] code_explorer_core::error::CoreError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl McpError {
    /// Convert to a JSON-RPC error code.
    pub fn error_code(&self) -> i64 {
        match self {
            McpError::MethodNotFound(_) => -32601,
            McpError::InvalidArguments { .. } => -32602,
            McpError::UnknownTool(_) => -32602,
            McpError::Transport(_) => -32700,
            McpError::WriteQueryRejected => -32001,
            McpError::RepoNotFound(_) => -32002,
            _ => -32603, // Internal error
        }
    }
}

pub type Result<T> = std::result::Result<T, McpError>;
