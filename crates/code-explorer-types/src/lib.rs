//! Core types for Code Explorer — zero dependencies beyond serde.
//!
//! This crate is the leaf of the dependency graph. All other Code Explorer crates
//! depend on it for shared type definitions. Designed to be WASM-compatible.

pub mod node;
pub mod edge;
pub mod id;
pub mod config;

// Re-export key types at crate root for convenience
pub use node::{NodeLabel, NodeProperties, GraphNode, ProcessType, EnrichedBy};
pub use edge::{RelationshipType, GraphRelationship};
pub use id::generate_id;
pub use config::SupportedLanguage;
