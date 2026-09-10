//! Database adapter with connection management.
//!
//! Defines a trait-based abstraction for the graph database backend.
//! The default `StubDbAdapter` provides a no-op implementation that
//! stores query history for testing. When the `kuzu-backend` feature
//! is enabled, `KuzuDbBackend` provides a real KuzuDB-backed
//! implementation using the official `kuzu` crate.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tracing::{info, warn};

use crate::error::{DbError, Result};
use crate::schema;

// ─── Trait ──────────────────────────────────────────────────────────────

/// Abstraction over the graph database engine.
///
/// Implementations must handle connection lifecycle, schema creation,
/// CSV bulk loading, and Cypher query execution.
pub trait DatabaseBackend: Send + Sync {
    /// Open or create the database at the given path.
    fn open(&mut self, db_path: &Path) -> Result<()>;

    /// Execute all schema DDL statements.
    fn create_schema(&self) -> Result<()>;

    /// Bulk-load CSV files into node/relationship tables.
    ///
    /// `csv_dir` should contain files named `{TableName}.csv` and `CodeRelation.csv`.
    fn bulk_load_csv(&self, csv_dir: &Path) -> Result<()>;

    /// Execute a read-only Cypher query and return results as JSON rows.
    fn execute_query(&self, query: &str) -> Result<Vec<Value>>;

    /// Close the connection and release resources.
    fn close(&mut self) -> Result<()>;

    /// Check if the database is currently open.
    fn is_open(&self) -> bool;

    /// Get the database path.
    fn db_path(&self) -> Option<&Path>;
}

// ─── DbAdapter (concrete wrapper) ───────────────────────────────────────

/// High-level database adapter that wraps a `DatabaseBackend`.
pub struct DbAdapter {
    inner: Box<dyn DatabaseBackend>,
}

impl DbAdapter {
    /// Create a new adapter with the given backend.
    pub fn new(backend: Box<dyn DatabaseBackend>) -> Self {
        Self { inner: backend }
    }

    /// Create a new adapter using the default stub backend.
    pub fn new_stub() -> Self {
        Self {
            inner: Box::new(StubDbBackend::new()),
        }
    }

    /// Create a new adapter using the real KuzuDB backend.
    ///
    /// Requires the `kuzu-backend` feature to be enabled.
    /// Panics at compile time if the feature is not available.
    #[cfg(feature = "kuzu-backend")]
    pub fn new_kuzu() -> Self {
        Self {
            inner: Box::new(KuzuDbBackend::new()),
        }
    }

    /// Open the database at the specified path.
    pub fn open(&mut self, db_path: &Path) -> Result<()> {
        self.inner.open(db_path)
    }

    /// Create the full schema (node tables, rel table, FTS indexes).
    pub fn create_schema(&self) -> Result<()> {
        self.inner.create_schema()
    }

    /// Bulk-load CSVs from the given directory.
    pub fn bulk_load_csv(&self, csv_dir: &Path) -> Result<()> {
        self.inner.bulk_load_csv(csv_dir)
    }

    /// Execute a Cypher query and return JSON results.
    pub fn execute_query(&self, query: &str) -> Result<Vec<Value>> {
        self.inner.execute_query(query)
    }

    /// Close the connection.
    pub fn close(&mut self) -> Result<()> {
        self.inner.close()
    }

    /// Check if the database is open.
    pub fn is_open(&self) -> bool {
        self.inner.is_open()
    }

    /// Get the database path.
    pub fn db_path(&self) -> Option<&Path> {
        self.inner.db_path()
    }
}

// ─── Stub Backend ───────────────────────────────────────────────────────

/// Stub database backend for testing and development.
///
/// Records all operations in memory. Does not persist data.
/// This will be replaced with a real KuzuDB backend when Rust bindings
/// become available.
pub struct StubDbBackend {
    db_path: Option<PathBuf>,
    is_open: bool,
    #[allow(dead_code)]
    schema_created: bool,
    /// Queries executed (for testing/inspection).
    pub query_log: Vec<String>,
}

impl StubDbBackend {
    pub fn new() -> Self {
        Self {
            db_path: None,
            is_open: false,
            schema_created: false,
            query_log: Vec::new(),
        }
    }
}

impl Default for StubDbBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseBackend for StubDbBackend {
    fn open(&mut self, db_path: &Path) -> Result<()> {
        info!("StubDbBackend: opening database at {}", db_path.display());
        self.db_path = Some(db_path.to_path_buf());
        self.is_open = true;
        Ok(())
    }

    fn create_schema(&self) -> Result<()> {
        if !self.is_open {
            return Err(DbError::SchemaError("Database not open".into()));
        }
        let schema_qs = schema::schema_queries();
        let fts_qs = schema::fts_queries();
        info!(
            "StubDbBackend: would create {} node/rel tables and {} FTS indexes",
            schema_qs.len(),
            fts_qs.len()
        );
        Ok(())
    }

    fn bulk_load_csv(&self, csv_dir: &Path) -> Result<()> {
        if !self.is_open {
            return Err(DbError::SchemaError("Database not open".into()));
        }
        info!(
            "StubDbBackend: would bulk-load CSVs from {}",
            csv_dir.display()
        );

        // Verify CSV directory exists and list files
        match std::fs::read_dir(csv_dir) {
            Ok(entries) => {
                let csv_files: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map(|ext| ext == "csv")
                            .unwrap_or(false)
                    })
                    .collect();
                info!("StubDbBackend: found {} CSV files to load", csv_files.len());
            }
            Err(e) => {
                warn!("StubDbBackend: CSV directory not found: {e}");
            }
        }

        Ok(())
    }

    fn execute_query(&self, query: &str) -> Result<Vec<Value>> {
        if !self.is_open {
            return Err(DbError::QueryError {
                query: query.to_string(),
                cause: "Database not open".into(),
            });
        }
        // Char-based truncation for UTF-8 safety: a multi-byte code point at
        // byte 99 would otherwise panic the logger when sliced at byte 100.
        let preview: String = query.chars().take(100).collect();
        info!("StubDbBackend: executing query: {}", preview);
        // Return empty results for the stub
        Ok(Vec::new())
    }

    fn close(&mut self) -> Result<()> {
        info!("StubDbBackend: closing database");
        self.is_open = false;
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.is_open
    }

    fn db_path(&self) -> Option<&Path> {
        self.db_path.as_deref()
    }
}

// ─── KuzuDB Backend ─────────────────────────────────────────────────────

#[cfg(feature = "kuzu-backend")]
mod kuzu_backend {
    use super::*;

    /// Real KuzuDB backend using the `kuzu` crate.
    ///
    /// Stores the [`kuzu::Database`] handle and creates fresh
    /// [`kuzu::Connection`]s on demand for each operation. This avoids
    /// self-referential borrow issues since `Connection<'a>` borrows
    /// `Database` with lifetime `'a`.
    pub struct KuzuDbBackend {
        db: Option<kuzu::Database>,
        db_path: Option<PathBuf>,
    }

    impl KuzuDbBackend {
        pub fn new() -> Self {
            Self {
                db: None,
                db_path: None,
            }
        }

        /// Create a new connection to the open database.
        fn connect(&self) -> Result<kuzu::Connection<'_>> {
            let db = self
                .db
                .as_ref()
                .ok_or_else(|| DbError::SchemaError("Database not open".into()))?;
            kuzu::Connection::new(db)
                .map_err(|e| DbError::SchemaError(format!("Failed to create connection: {e}")))
        }

        /// Convert a [`kuzu::Value`] to a [`serde_json::Value`].
        fn kuzu_value_to_json(val: &kuzu::Value) -> serde_json::Value {
            match val {
                kuzu::Value::Null(_) => serde_json::Value::Null,
                kuzu::Value::Bool(b) => serde_json::Value::Bool(*b),
                kuzu::Value::Int8(n) => serde_json::json!(*n),
                kuzu::Value::Int16(n) => serde_json::json!(*n),
                kuzu::Value::Int32(n) => serde_json::json!(*n),
                kuzu::Value::Int64(n) => serde_json::json!(*n),
                kuzu::Value::UInt8(n) => serde_json::json!(*n),
                kuzu::Value::UInt16(n) => serde_json::json!(*n),
                kuzu::Value::UInt32(n) => serde_json::json!(*n),
                kuzu::Value::UInt64(n) => serde_json::json!(*n),
                kuzu::Value::Int128(n) => serde_json::json!(n.to_string()),
                kuzu::Value::Float(f) => serde_json::json!(*f),
                kuzu::Value::Double(f) => serde_json::json!(*f),
                kuzu::Value::String(s) => serde_json::Value::String(s.clone()),
                kuzu::Value::Blob(b) => {
                    serde_json::Value::String(format!("<blob:{} bytes>", b.len()))
                }
                kuzu::Value::List(_, items) | kuzu::Value::Array(_, items) => {
                    serde_json::Value::Array(items.iter().map(Self::kuzu_value_to_json).collect())
                }
                kuzu::Value::Struct(fields) => {
                    let map: serde_json::Map<String, serde_json::Value> = fields
                        .iter()
                        .map(|(k, v)| (k.clone(), Self::kuzu_value_to_json(v)))
                        .collect();
                    serde_json::Value::Object(map)
                }
                kuzu::Value::Node(node) => {
                    let mut map = serde_json::Map::new();
                    map.insert(
                        "_label".to_string(),
                        serde_json::Value::String(node.get_label_name().clone()),
                    );
                    let id = node.get_node_id();
                    map.insert(
                        "_id".to_string(),
                        serde_json::json!(format!("{}:{}", id.table_id, id.offset)),
                    );
                    for (k, v) in node.get_properties() {
                        map.insert(k.clone(), Self::kuzu_value_to_json(v));
                    }
                    serde_json::Value::Object(map)
                }
                kuzu::Value::Rel(rel) => {
                    let mut map = serde_json::Map::new();
                    map.insert(
                        "_label".to_string(),
                        serde_json::Value::String(rel.get_label_name().clone()),
                    );
                    let src = rel.get_src_node();
                    let dst = rel.get_dst_node();
                    map.insert(
                        "_src".to_string(),
                        serde_json::json!(format!("{}:{}", src.table_id, src.offset)),
                    );
                    map.insert(
                        "_dst".to_string(),
                        serde_json::json!(format!("{}:{}", dst.table_id, dst.offset)),
                    );
                    for (k, v) in rel.get_properties() {
                        map.insert(k.clone(), Self::kuzu_value_to_json(v));
                    }
                    serde_json::Value::Object(map)
                }
                kuzu::Value::Map(_, entries) => {
                    let map: serde_json::Map<String, serde_json::Value> = entries
                        .iter()
                        .map(|(k, v)| (k.to_string(), Self::kuzu_value_to_json(v)))
                        .collect();
                    serde_json::Value::Object(map)
                }
                // Fallback: use Display for date/time/interval/UUID/decimal types
                other => serde_json::Value::String(other.to_string()),
            }
        }
    }

    impl Default for KuzuDbBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    impl DatabaseBackend for KuzuDbBackend {
        fn open(&mut self, db_path: &Path) -> Result<()> {
            info!("KuzuDbBackend: opening database at {}", db_path.display());

            // Ensure the parent directory exists
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    DbError::SchemaError(format!(
                        "Failed to create directory {}: {e}",
                        parent.display()
                    ))
                })?;
            }

            let db = kuzu::Database::new(db_path, kuzu::SystemConfig::default()).map_err(|e| {
                DbError::SchemaError(format!(
                    "Failed to open KuzuDB at {}: {e}",
                    db_path.display()
                ))
            })?;

            // Verify connectivity
            {
                let _conn = kuzu::Connection::new(&db).map_err(|e| {
                    DbError::SchemaError(format!("Failed to create initial connection: {e}"))
                })?;
            }

            self.db_path = Some(db_path.to_path_buf());
            self.db = Some(db);

            info!("KuzuDbBackend: database opened successfully");
            Ok(())
        }

        fn create_schema(&self) -> Result<()> {
            let conn = self.connect()?;
            let schema_qs = schema::schema_queries();
            let fts_qs = schema::fts_queries();

            info!(
                "KuzuDbBackend: creating {} node/rel tables and {} FTS indexes",
                schema_qs.len(),
                fts_qs.len()
            );

            for query in &schema_qs {
                conn.query(query).map_err(|e| {
                    DbError::SchemaError(format!("Failed to execute: {query}\n  Error: {e}"))
                })?;
            }

            for query in &fts_qs {
                conn.query(query).map_err(|e| {
                    DbError::SchemaError(format!(
                        "Failed to create FTS index: {query}\n  Error: {e}"
                    ))
                })?;
            }

            info!("KuzuDbBackend: schema created successfully");
            Ok(())
        }

        fn bulk_load_csv(&self, csv_dir: &Path) -> Result<()> {
            let conn = self.connect()?;

            info!(
                "KuzuDbBackend: bulk-loading CSVs from {}",
                csv_dir.display()
            );

            // Collect CSV files, partitioned into node tables vs. the relation table.
            // The relation table (CodeRelation) must be loaded after all node tables
            // so that the referenced node IDs exist.
            let entries = std::fs::read_dir(csv_dir).map_err(|e| DbError::CsvError {
                table: "csv_dir".into(),
                cause: format!("Cannot read directory {}: {e}", csv_dir.display()),
            })?;

            let mut node_csv_files = Vec::new();
            let mut rel_csv_file = None;

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "csv") {
                    let table_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    if table_name == "CodeRelation" {
                        rel_csv_file = Some((table_name, path));
                    } else {
                        node_csv_files.push((table_name, path));
                    }
                }
            }

            // Load node tables first
            for (table_name, path) in &node_csv_files {
                let csv_path_str = path.to_str().unwrap_or("").replace('\\', "/");
                let query = format!(
                    "COPY {table_name} FROM '{csv_path_str}' (HEADER=true, ESCAPE='\\\"', DELIM=',')"
                );
                info!("KuzuDbBackend: loading {table_name} from CSV");
                conn.query(&query).map_err(|e| DbError::CsvError {
                    table: table_name.clone(),
                    cause: e.to_string(),
                })?;
            }

            // Load relationship table
            if let Some((table_name, path)) = &rel_csv_file {
                let csv_path_str = path.to_str().unwrap_or("").replace('\\', "/");
                let query = format!(
                    "COPY {table_name} FROM '{csv_path_str}' (HEADER=true, ESCAPE='\\\"', DELIM=',')"
                );
                info!("KuzuDbBackend: loading {table_name} from CSV");
                conn.query(&query).map_err(|e| DbError::CsvError {
                    table: table_name.clone(),
                    cause: e.to_string(),
                })?;
            }

            info!(
                "KuzuDbBackend: loaded {} node CSVs and {} relation CSVs",
                node_csv_files.len(),
                if rel_csv_file.is_some() { 1 } else { 0 }
            );

            Ok(())
        }

        fn execute_query(&self, query: &str) -> Result<Vec<Value>> {
            // Propagate the real reason from `connect()` rather than masking
            // it with a fixed "Database not open" string. `connect()` can
            // also fail when `kuzu::Connection::new` returns an internal
            // error (e.g. transient lock contention or an upstream kuzu
            // panic-to-error conversion); flattening every variant to
            // "Database not open" hid those failures and made KuzuDB-mode
            // bug reports very hard to triage.
            let conn = self.connect().map_err(|e| DbError::QueryError {
                query: query.to_string(),
                cause: e.to_string(),
            })?;

            // Char-based truncation for UTF-8 safety: a multi-byte code
            // point at byte 119 would otherwise panic the logger when
            // sliced at byte 120.
            let preview: String = query.chars().take(120).collect();
            info!("KuzuDbBackend: executing query: {}", preview);

            let result = conn.query(query).map_err(|e| DbError::QueryError {
                query: query.to_string(),
                cause: e.to_string(),
            })?;

            let column_names = result.get_column_names();
            let num_cols = result.get_num_columns();

            let mut rows = Vec::new();
            for tuple in result {
                let mut row = serde_json::Map::with_capacity(num_cols);
                for (i, val) in tuple.iter().enumerate() {
                    let col_name = column_names
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("col_{i}"));
                    row.insert(col_name, Self::kuzu_value_to_json(val));
                }
                rows.push(serde_json::Value::Object(row));
            }

            Ok(rows)
        }

        fn close(&mut self) -> Result<()> {
            info!("KuzuDbBackend: closing database");
            // Drop the database handle, which releases all resources.
            self.db = None;
            self.db_path = None;
            Ok(())
        }

        fn is_open(&self) -> bool {
            self.db.is_some()
        }

        fn db_path(&self) -> Option<&Path> {
            self.db_path.as_deref()
        }
    }
}

#[cfg(feature = "kuzu-backend")]
pub use kuzu_backend::KuzuDbBackend;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_stub_lifecycle() {
        let mut adapter = DbAdapter::new_stub();
        assert!(!adapter.is_open());

        adapter.open(&PathBuf::from("/tmp/test.db")).unwrap();
        assert!(adapter.is_open());

        adapter.create_schema().unwrap();

        let results = adapter.execute_query("MATCH (n) RETURN n LIMIT 1").unwrap();
        assert!(results.is_empty());

        adapter.close().unwrap();
        assert!(!adapter.is_open());
    }

    #[test]
    fn test_stub_errors_when_closed() {
        let adapter = DbAdapter::new_stub();
        assert!(adapter.execute_query("MATCH (n) RETURN n").is_err());
        assert!(adapter.create_schema().is_err());
    }
}
