pub mod ast_cache;
pub mod grammar;
pub mod incremental;
pub mod manifest;
pub mod phases;
pub mod pipeline;
pub mod type_env;
pub mod utils;
pub mod workers;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum IngestError {
    #[error("Pipeline phase {phase} failed: {message}")]
    PhaseError { phase: String, message: String },
    #[error("Parse timeout for {path} after {timeout_secs}s")]
    ParseTimeout { path: String, timeout_secs: u64 },
    #[error("Tree-sitter error for {path}: {message}")]
    TreeSitterError { path: String, message: String },
    #[error(transparent)]
    Core(#[from] code_explorer_core::error::CoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
