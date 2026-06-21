//! **code-explorer-query** — Code Query Language (CQL) parser and executor.
//!
//! Provides a Cypher-like query language for exploring a [`KnowledgeGraph`].
//!
//! ```text
//! MATCH (n:Function) WHERE n.name CONTAINS 'handle' RETURN n.name, n.file_path LIMIT 10
//! MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a.name, b.name
//! CALL QUERY_FTS_INDEX('symbols', 'login')
//! ```

pub mod ast;
pub mod executor;
pub mod parser;

use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct CqlParser;

// Re-export main public API
pub use executor::{execute, ExecutionError, QueryResult, Value};
pub use parser::{parse_cql, ParseError};
