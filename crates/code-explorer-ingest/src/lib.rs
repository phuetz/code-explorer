#![allow(clippy::if_same_then_else)]
#![allow(clippy::too_many_arguments, clippy::unnecessary_sort_by, clippy::collapsible_if, clippy::manual_contains, clippy::question_mark, clippy::manual_pattern_char_comparison, clippy::needless_lifetimes, clippy::derivable_impls, clippy::type_complexity)]
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
