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

    /// Stable, machine-readable identifier for this failure.
    ///
    /// JSON-RPC codes are coarse (`-32603` covers every internal failure), so a
    /// caller that wants to react — retry, re-index, ask the user — has to
    /// pattern-match on English prose. This slug never changes with the
    /// wording.
    pub fn slug(&self) -> &'static str {
        match self {
            McpError::Transport(_) => "TRANSPORT_ERROR",
            McpError::UnknownTool(_) => "UNKNOWN_TOOL",
            McpError::InvalidArguments { .. } => "INVALID_ARGUMENTS",
            McpError::WriteQueryRejected => "WRITE_QUERY_REJECTED",
            McpError::RepoNotFound(_) => "REPO_NOT_FOUND",
            McpError::MethodNotFound(_) => "METHOD_NOT_FOUND",
            McpError::Internal(_) => "INTERNAL_ERROR",
            McpError::Db(_) => "DATABASE_ERROR",
            McpError::Core(_) => "CORE_ERROR",
            McpError::Json(_) => "JSON_ERROR",
            McpError::Io(_) => "IO_ERROR",
        }
    }

    /// What the caller should do about it, in one sentence.
    pub fn hint(&self) -> &'static str {
        match self {
            McpError::Transport(_) => "The connection to the MCP server failed; restart the client.",
            McpError::UnknownTool(_) => "Call `tools/list` to see the tools this server exposes.",
            McpError::InvalidArguments { .. } => {
                "Check the tool's input schema in `tools/list` and retry with corrected arguments."
            }
            McpError::WriteQueryRejected => {
                "The `cypher` tool is read-only; use MATCH/RETURN, and change the repository with an editor."
            }
            McpError::RepoNotFound(_) => {
                "Run `code-explorer analyze <path>`; the server picks the new index up on the next call, no restart needed."
            }
            McpError::MethodNotFound(_) => "Use a JSON-RPC method this server implements.",
            McpError::Db(_) | McpError::Io(_) => {
                "The index may be missing or damaged; run `code-explorer doctor <path>`."
            }
            McpError::Core(_) => "Run `code-explorer doctor <path>` to check the index and registry.",
            McpError::Json(_) => "The payload was not valid JSON; check the request body.",
            McpError::Internal(_) => "Retry; if it persists, run `code-explorer doctor <path>`.",
        }
    }

    /// Structured payload for the JSON-RPC `error.data` field.
    pub fn error_data(&self) -> serde_json::Value {
        let mut data = serde_json::json!({
            "code": self.slug(),
            "message": self.to_string(),
            "hint": self.hint(),
        });
        if let McpError::InvalidArguments { tool, .. } = self {
            data["tool"] = serde_json::Value::String(tool.clone());
        }
        if let McpError::UnknownTool(tool) = self {
            data["tool"] = serde_json::Value::String(tool.clone());
        }
        data
    }
}

pub type Result<T> = std::result::Result<T, McpError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_carries_a_slug_a_message_and_a_hint() {
        let errors = vec![
            McpError::Transport("boom".into()),
            McpError::UnknownTool("nope".into()),
            McpError::InvalidArguments {
                tool: "context".into(),
                reason: "missing name".into(),
            },
            McpError::WriteQueryRejected,
            McpError::RepoNotFound("somewhere".into()),
            McpError::MethodNotFound("weird/method".into()),
            McpError::Internal("oops".into()),
            McpError::Json(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
            McpError::Io(std::io::Error::other("disk")),
        ];

        for error in errors {
            let data = error.error_data();
            let code = data["code"].as_str().expect("code");
            assert!(
                code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "slug must be a stable machine identifier, got {code}"
            );
            assert!(!data["message"].as_str().expect("message").is_empty());
            assert!(!data["hint"].as_str().expect("hint").is_empty());
        }
    }

    #[test]
    fn argument_errors_name_the_tool_in_their_data() {
        let error = McpError::InvalidArguments {
            tool: "impact".into(),
            reason: "missing target".into(),
        };
        assert_eq!(error.error_data()["tool"], "impact");
        assert_eq!(error.error_data()["code"], "INVALID_ARGUMENTS");
    }

    #[test]
    fn a_missing_repository_tells_the_caller_to_analyze_it() {
        let error = McpError::RepoNotFound("/repos/thing".into());
        let data = error.error_data();
        assert_eq!(data["code"], "REPO_NOT_FOUND");
        assert!(data["hint"].as_str().unwrap().contains("code-explorer analyze"));
    }
}
