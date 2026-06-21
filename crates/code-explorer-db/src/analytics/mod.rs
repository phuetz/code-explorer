//! Code-quality analytics on top of the knowledge graph.
//!
//! Each submodule is a pure function over `KnowledgeGraph` / `GraphIndexes`.
//! No I/O, no mutation, no external state — easy to test, cheap to call from
//! both MCP tools and Tauri commands.

pub mod clones;
pub mod complexity;
pub mod cycles;
pub mod graph_diff;

pub use graph_diff::{diff_graphs, EdgeKey, GraphDiff, ModifiedNode};
