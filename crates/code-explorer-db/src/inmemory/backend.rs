//! In-memory database backend that replaces KuzuDB.
//!
//! Loads a `KnowledgeGraph` from a JSON snapshot and executes Cypher-like
//! queries against it using the hand-written parser in [`super::cypher`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use code_explorer_core::graph::KnowledgeGraph;
use serde_json::Value;
use tracing::info;

use crate::adapter::DatabaseBackend;
use crate::error::{DbError, Result};

use super::cypher::GraphIndexes;
use super::fts::FtsIndex;

/// In-memory backend that holds a `KnowledgeGraph` and answers Cypher-like queries.
pub struct InMemoryBackend {
    graph: Option<Arc<KnowledgeGraph>>,
    db_path: Option<PathBuf>,
    indexes: Option<GraphIndexes>,
    fts_index: FtsIndex,
}

impl InMemoryBackend {
    /// Create a new, unopened in-memory backend.
    pub fn new() -> Self {
        Self {
            graph: None,
            db_path: None,
            indexes: None,
            fts_index: FtsIndex::new(),
        }
    }

    /// Create an in-memory backend from an already-loaded `KnowledgeGraph`.
    ///
    /// This is useful for testing or when the graph is already in memory.
    pub fn from_graph(graph: KnowledgeGraph) -> Self {
        let indexes = GraphIndexes::build(&graph);
        let fts_index = FtsIndex::build(&graph);
        let graph = Arc::new(graph);
        Self {
            graph: Some(graph),
            db_path: None,
            indexes: Some(indexes),
            fts_index,
        }
    }

    /// Get a reference to the underlying `KnowledgeGraph`, if loaded.
    pub fn graph(&self) -> Option<&KnowledgeGraph> {
        self.graph.as_deref()
    }

    /// Get a clone of the `Arc<KnowledgeGraph>`, if loaded.
    pub fn graph_arc(&self) -> Option<Arc<KnowledgeGraph>> {
        self.graph.clone()
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseBackend for InMemoryBackend {
    fn open(&mut self, db_path: &Path) -> Result<()> {
        info!("InMemoryBackend: opening database at {}", db_path.display());

        // The db_path may be storage_path/db (the KuzuDB convention) while
        // the JSON snapshot lives at storage_path/graph.bin.  Try db_path
        // first, then its parent, so both layouts work.
        let snapshot_path = crate::snapshot::snapshot_path(db_path);
        let snapshot_path = if crate::snapshot::snapshot_exists(&snapshot_path) {
            snapshot_path
        } else if let Some(parent) = db_path.parent() {
            crate::snapshot::snapshot_path(parent)
        } else {
            snapshot_path
        };

        if !crate::snapshot::snapshot_exists(&snapshot_path) {
            // No snapshot yet — open with empty graph. Build the indexes
            // from the *same* empty graph we store in `self.graph` rather
            // than constructing a second one. Both are empty today so the
            // visible behaviour is identical, but having the indexes
            // derived from a different graph instance than the one served
            // by `execute_query` is a latent correctness hazard if
            // `KnowledgeGraph::new()` ever grows per-instance state.
            info!(
                "InMemoryBackend: no snapshot at {}, starting with empty graph",
                snapshot_path.display()
            );
            let empty = KnowledgeGraph::new();
            let indexes = GraphIndexes::build(&empty);
            self.graph = Some(Arc::new(empty));
            self.indexes = Some(indexes);
            self.fts_index = FtsIndex::new();
            self.db_path = Some(db_path.to_path_buf());
            return Ok(());
        }

        let graph = crate::snapshot::load_snapshot(&snapshot_path)?;

        info!(
            "InMemoryBackend: loaded {} nodes, {} relationships",
            graph.node_count(),
            graph.relationship_count()
        );

        let indexes = GraphIndexes::build(&graph);
        let fts_index = FtsIndex::build(&graph);

        self.graph = Some(Arc::new(graph));
        self.indexes = Some(indexes);
        self.fts_index = fts_index;
        self.db_path = Some(db_path.to_path_buf());

        info!("InMemoryBackend: database opened successfully");
        Ok(())
    }

    fn create_schema(&self) -> Result<()> {
        // No-op: schema is implicit in the KnowledgeGraph structure.
        info!("InMemoryBackend: create_schema is a no-op (graph is already structured)");
        Ok(())
    }

    fn bulk_load_csv(&self, csv_dir: &Path) -> Result<()> {
        // No-op: data is loaded from the snapshot.
        info!(
            "InMemoryBackend: bulk_load_csv is a no-op (data loaded from snapshot), csv_dir={}",
            csv_dir.display()
        );
        Ok(())
    }

    fn execute_query(&self, query: &str) -> Result<Vec<Value>> {
        if crate::query::is_write_query(query) {
            return Err(DbError::QueryError {
                query: query.to_string(),
                cause: "Write queries are not allowed".into(),
            });
        }

        let graph = self.graph.as_ref().ok_or_else(|| DbError::QueryError {
            query: query.to_string(),
            cause: "Database not open".into(),
        })?;

        let indexes = self.indexes.as_ref().ok_or_else(|| DbError::QueryError {
            query: query.to_string(),
            cause: "Indexes not built".into(),
        })?;

        let stmt = super::cypher::parse(query).map_err(|e| DbError::QueryError {
            query: query.to_string(),
            cause: format!("{e}"),
        })?;

        let query_start = std::time::Instant::now();
        let mut results = super::cypher::execute(&stmt, graph, indexes, &self.fts_index)?;
        let duration = query_start.elapsed();

        if duration.as_secs() > 5 {
            tracing::warn!(
                query = %query,
                duration_ms = duration.as_millis() as u64,
                "Slow Cypher query detected in InMemoryBackend"
            );
        }

        if results.len() > 1000 {
            tracing::warn!(
                total_rows = results.len(),
                "Query returned {} results, truncating to 1000",
                results.len()
            );
            results.truncate(1000);
        }

        Ok(results)
    }

    fn close(&mut self) -> Result<()> {
        info!("InMemoryBackend: closing database");
        self.graph = None;
        self.indexes = None;
        self.fts_index = FtsIndex::new();
        self.db_path = None;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.graph.is_some()
    }

    fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::*;

    fn make_test_graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();
        g.add_node(GraphNode {
            id: "Function:src/auth.ts:handleLogin".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "handleLogin".to_string(),
                file_path: "src/auth.ts".to_string(),
                start_line: Some(10),
                end_line: Some(30),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Function:src/auth.ts:validateToken".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "validateToken".to_string(),
                file_path: "src/auth.ts".to_string(),
                start_line: Some(35),
                end_line: Some(50),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Class:src/user.ts:UserService".to_string(),
            label: NodeLabel::Class,
            properties: NodeProperties {
                name: "UserService".to_string(),
                file_path: "src/user.ts".to_string(),
                ..Default::default()
            },
        });
        g.add_relationship(GraphRelationship {
            id: "r1".to_string(),
            source_id: "Function:src/auth.ts:handleLogin".to_string(),
            target_id: "Function:src/auth.ts:validateToken".to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 1.0,
            reason: "exact".to_string(),
            step: None,
        });
        g
    }

    #[test]
    fn test_from_graph() {
        let graph = make_test_graph();
        let backend = InMemoryBackend::from_graph(graph);
        assert!(backend.is_open());
        assert!(backend.graph().is_some());
    }

    #[test]
    fn test_execute_query() {
        let graph = make_test_graph();
        let backend = InMemoryBackend::from_graph(graph);

        let results = backend
            .execute_query("MATCH (n:Function) RETURN n")
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_execute_query_with_where() {
        let graph = make_test_graph();
        let backend = InMemoryBackend::from_graph(graph);

        let results = backend
            .execute_query("MATCH (n:Function) WHERE n.name = 'handleLogin' RETURN n")
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["n"]["name"], "handleLogin");
    }

    #[test]
    fn test_execute_relationship_query() {
        let graph = make_test_graph();
        let backend = InMemoryBackend::from_graph(graph);

        let results = backend
            .execute_query("MATCH (n)-[:CALLS]->(m) WHERE n.name = 'handleLogin' RETURN m.name")
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["m.name"], "validateToken");
    }

    #[test]
    fn test_close_and_requery_fails() {
        let graph = make_test_graph();
        let mut backend = InMemoryBackend::from_graph(graph);
        backend.close().unwrap();

        assert!(!backend.is_open());
        assert!(backend.execute_query("MATCH (n) RETURN n").is_err());
    }

    #[test]
    fn test_create_schema_noop() {
        let graph = make_test_graph();
        let backend = InMemoryBackend::from_graph(graph);
        assert!(backend.create_schema().is_ok());
    }

    #[test]
    fn test_bulk_load_csv_noop() {
        let graph = make_test_graph();
        let backend = InMemoryBackend::from_graph(graph);
        assert!(backend.bulk_load_csv(Path::new("/tmp")).is_ok());
    }

    #[test]
    fn test_fts_query() {
        let graph = make_test_graph();
        let backend = InMemoryBackend::from_graph(graph);

        let results = backend
            .execute_query("CALL QUERY_FTS_INDEX('fts_Function', 'handleLogin')")
            .unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0]["name"], "handleLogin");
    }

    #[test]
    fn test_open_empty() {
        let mut backend = InMemoryBackend::new();
        let dir = std::env::temp_dir().join("code_explorer_inmem_test_empty");
        std::fs::create_dir_all(&dir).unwrap();

        backend.open(&dir).unwrap();
        assert!(backend.is_open());

        let results = backend
            .execute_query("MATCH (n:Function) RETURN n")
            .unwrap();
        assert!(results.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_open_from_snapshot() {
        let graph = make_test_graph();

        let dir = std::env::temp_dir().join("code_explorer_inmem_test_snapshot");
        std::fs::create_dir_all(&dir).unwrap();

        let snap_path = crate::snapshot::snapshot_path(&dir);
        crate::snapshot::save_snapshot(&graph, &snap_path).unwrap();

        let mut backend = InMemoryBackend::new();
        backend.open(&dir).unwrap();
        assert!(backend.is_open());

        let results = backend
            .execute_query("MATCH (n:Function) RETURN n")
            .unwrap();
        assert_eq!(results.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
