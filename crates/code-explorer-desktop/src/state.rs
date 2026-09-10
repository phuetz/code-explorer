//! Application state managed by Tauri.
//!
//! Loads the knowledge graph directly into memory for O(1) lookups,
//! following the same pattern as the TUI dashboard.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::storage::repo_manager::{self, RegistryEntry};
use code_explorer_db::inmemory::cypher::GraphIndexes;
use code_explorer_db::inmemory::fts::FtsIndex;
use code_explorer_db::snapshot;
use code_explorer_mcp::backend::local::LocalBackend;
use code_explorer_search::embeddings::{EmbeddingConfig, EmbeddingStore};
use code_explorer_search::fusion;
use tokio::sync::{Mutex, RwLock};

use crate::types::ChatConfig;

/// Strip Windows UNC prefix `\\?\` from paths so git and other tools work.
fn normalize_path(path: &str) -> String {
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

/// A loaded repository with its graph, indexes, and FTS.
///
/// `embeddings` and `embeddings_config` are loaded best-effort at `open_repo`
/// time: present means the repo has been embedded (`code-explorer embed --model …`)
/// and the chat can use hybrid BM25+semantic search; absent means we silently
/// fall back to BM25-only — the user gets a usable chat either way.
pub struct LoadedRepo {
    pub entry: RegistryEntry,
    pub graph: Arc<KnowledgeGraph>,
    pub indexes: Arc<GraphIndexes>,
    pub fts_index: Arc<FtsIndex>,
    pub embeddings: Option<Arc<EmbeddingStore>>,
    pub embeddings_config: Option<Arc<EmbeddingConfig>>,
}

/// The main application state, shared across all Tauri commands.
pub struct AppState {
    /// Loaded repositories keyed by name.
    repos: RwLock<HashMap<String, LoadedRepo>>,
    /// Global registry entries.
    registry: RwLock<Vec<RegistryEntry>>,
    /// Currently active repository name.
    active_repo: RwLock<Option<String>>,
    /// Serializes the slow snapshot-load path so two concurrent calls to
    /// `open_repo("foo")` can't both hit disk + rebuild indexes for the
    /// same repo. The fast path (already-loaded) doesn't take this lock.
    load_lock: Mutex<()>,
    /// Full chat config for the current desktop session. Secrets stay here,
    /// not on disk.
    chat_config: RwLock<Option<ChatConfig>>,
    /// Cancellation flag — set to true to abort the current streaming chat request.
    pub cancel_flag: Arc<AtomicBool>,
    /// Shared MCP backend used by the chat to fall back to the full 27-tool
    /// surface (`hotspots`, `coupling`, `coverage`, `find_cycles`, …) when the
    /// LLM invokes a tool the chat hasn't custom-implemented. Wrapped in a
    /// Tokio Mutex because `LocalBackend::call_tool` takes `&mut self` for its
    /// internal snapshot/indexes/fts caches.
    ///
    /// TODO: share snapshot/indexes/fts caches with `LoadedRepo` to avoid
    /// holding two copies of every loaded graph in RAM (~50-100MB extra per
    /// repo on Sample-scale codebases). Not blocking — desktop has the budget —
    /// but worth deduplicating before piling more state on `LocalBackend`.
    pub mcp_backend: Arc<Mutex<LocalBackend>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// Create a new empty state.
    pub fn new() -> Self {
        // Best-effort init of the MCP backend's registry. A registry-load
        // failure here just means the backend starts empty — the chat can
        // still call tools that don't require a `repo` arg, and the registry
        // will be re-loaded by the desktop's own `load_registry()` flow.
        let mut backend = LocalBackend::new();
        if let Err(e) = backend.init() {
            tracing::warn!(error = %e, "LocalBackend init failed — chat MCP fallback may be empty until a repo is added");
        }
        Self {
            repos: RwLock::new(HashMap::new()),
            registry: RwLock::new(Vec::new()),
            active_repo: RwLock::new(None),
            load_lock: Mutex::new(()),
            chat_config: RwLock::new(None),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            mcp_backend: Arc::new(Mutex::new(backend)),
        }
    }

    /// Load the global registry from disk.
    pub async fn load_registry(&self) -> Result<Vec<RegistryEntry>, String> {
        let entries = repo_manager::read_registry().map_err(|e| e.to_string())?;
        *self.registry.write().await = entries.clone();
        // Keep the MCP backend's view in sync so chat tool fallbacks can
        // resolve newly-added repos by name.
        if let Err(e) = self.mcp_backend.lock().await.init() {
            tracing::warn!(error = %e, "failed to refresh LocalBackend registry");
        }
        Ok(entries)
    }

    /// Get the current registry entries.
    pub async fn registry(&self) -> Vec<RegistryEntry> {
        self.registry.read().await.clone()
    }

    /// Open a repository by name: load its graph and build indexes.
    pub async fn open_repo(&self, name: &str) -> Result<(), String> {
        // Fast path: already loaded.
        if self.repos.read().await.contains_key(name) {
            *self.active_repo.write().await = Some(name.to_string());
            return Ok(());
        }

        // Slow path: serialize load attempts so two concurrent callers don't
        // both load the same snapshot from disk and rebuild its indexes.
        let _guard = self.load_lock.lock().await;

        // Re-check inside the load lock — a previous waiter may have just
        // finished loading this same repo while we were waiting on the lock.
        if self.repos.read().await.contains_key(name) {
            *self.active_repo.write().await = Some(name.to_string());
            return Ok(());
        }

        // Find entry in registry
        let registry = self.registry.read().await;
        let entry = registry
            .iter()
            .find(|e| e.name == name)
            .ok_or_else(|| format!("Repository '{}' not found in registry", name))?
            .clone();
        drop(registry);

        // Load snapshot (normalize UNC path for Windows compatibility)
        let normalized_storage = normalize_path(&entry.storage_path);
        let storage_path = Path::new(&normalized_storage);
        let snap_path = snapshot::snapshot_path(storage_path);

        if !snapshot::snapshot_exists(&snap_path) {
            return Err(format!(
                "No snapshot found for '{}'. Run `code-explorer analyze` first.",
                name
            ));
        }

        let graph = snapshot::load_snapshot(&snap_path).map_err(|e| e.to_string())?;
        let indexes = GraphIndexes::build(&graph);
        let fts_index = FtsIndex::build(&graph);

        // Best-effort embeddings load. A malformed sidecar logs a warning but
        // shouldn't take the whole repo offline — the chat will just fall back
        // to BM25-only and the user can re-run `code-explorer embed`.
        let (embeddings, embeddings_config) =
            match fusion::try_load_embeddings_from_storage(storage_path) {
                Ok(Some((store, cfg))) => {
                    tracing::info!(
                        repo = %name,
                        count = store.header.count,
                        model = %cfg.model_name,
                        "loaded embeddings for hybrid chat search"
                    );
                    (Some(Arc::new(store)), Some(Arc::new(cfg)))
                }
                Ok(None) => (None, None),
                Err(e) => {
                    tracing::warn!(
                        repo = %name,
                        error = %e,
                        "failed to load embeddings — chat will use BM25-only search"
                    );
                    (None, None)
                }
            };

        let loaded = LoadedRepo {
            entry,
            graph: Arc::new(graph),
            indexes: Arc::new(indexes),
            fts_index: Arc::new(fts_index),
            embeddings,
            embeddings_config,
        };

        self.repos.write().await.insert(name.to_string(), loaded);
        *self.active_repo.write().await = Some(name.to_string());

        Ok(())
    }

    /// Get the active repository name.
    pub async fn active_repo_name(&self) -> Option<String> {
        self.active_repo.read().await.clone()
    }

    /// Reload a repo's graph from disk (e.g. after re-indexing).
    /// Removes the old loaded data and re-loads from snapshot.
    pub async fn reload_repo(&self, name: &str) -> Result<(), String> {
        // Remove cached data so next open_repo reloads from disk
        self.repos.write().await.remove(name);
        // Re-open with fresh data
        self.open_repo(name).await
    }

    /// Get a reference to a loaded repo's components.
    /// Returns (graph, indexes, fts_index) or an error.
    pub async fn get_repo(
        &self,
        name: Option<&str>,
    ) -> Result<
        (
            Arc<KnowledgeGraph>,
            Arc<GraphIndexes>,
            Arc<FtsIndex>,
            String,
        ),
        String,
    > {
        let repo_name = match name {
            Some(n) => n.to_string(),
            None => self
                .active_repo
                .read()
                .await
                .clone()
                .ok_or_else(|| "No active repository. Open one first.".to_string())?,
        };

        let repos = self.repos.read().await;
        let loaded = repos
            .get(&repo_name)
            .ok_or_else(|| format!("Repository '{}' is not loaded", repo_name))?;

        Ok((
            Arc::clone(&loaded.graph),
            Arc::clone(&loaded.indexes),
            Arc::clone(&loaded.fts_index),
            normalize_path(&loaded.entry.path),
        ))
    }

    /// Same as `get_repo` but also returns the (optional) cached embedding
    /// store and config — used by the chat's hybrid search path. Callers that
    /// don't need embeddings should keep using `get_repo` to avoid pulling in
    /// the search-types churn.
    #[allow(clippy::type_complexity)]
    pub async fn get_repo_with_embeddings(
        &self,
        name: Option<&str>,
    ) -> Result<
        (
            Arc<KnowledgeGraph>,
            Arc<GraphIndexes>,
            Arc<FtsIndex>,
            Option<Arc<EmbeddingStore>>,
            Option<Arc<EmbeddingConfig>>,
            String,
        ),
        String,
    > {
        let repo_name = match name {
            Some(n) => n.to_string(),
            None => self
                .active_repo
                .read()
                .await
                .clone()
                .ok_or_else(|| "No active repository. Open one first.".to_string())?,
        };

        let repos = self.repos.read().await;
        let loaded = repos
            .get(&repo_name)
            .ok_or_else(|| format!("Repository '{}' is not loaded", repo_name))?;

        Ok((
            Arc::clone(&loaded.graph),
            Arc::clone(&loaded.indexes),
            Arc::clone(&loaded.fts_index),
            loaded.embeddings.as_ref().map(Arc::clone),
            loaded.embeddings_config.as_ref().map(Arc::clone),
            normalize_path(&loaded.entry.path),
        ))
    }

    /// Resolve the storage path (`.codeexplorer/`) of the active repo.
    /// Used by per-repo persistent stores (bookmarks, saved queries, …).
    pub async fn active_storage_path(&self) -> Result<String, String> {
        let active = self
            .active_repo
            .read()
            .await
            .clone()
            .ok_or_else(|| "No active repository.".to_string())?;
        let repos = self.repos.read().await;
        repos
            .get(&active)
            .map(|l| l.entry.storage_path.clone())
            .ok_or_else(|| format!("Repository '{active}' not loaded"))
    }

    /// Resolve the on-disk source path of the active repo (the directory the
    /// user actually opened). Used by Theme C's commit-aware snapshot logic
    /// that needs to invoke `git worktree add` against the real working tree.
    pub async fn active_repo_path(&self) -> Result<String, String> {
        let active = self
            .active_repo
            .read()
            .await
            .clone()
            .ok_or_else(|| "No active repository.".to_string())?;
        let repos = self.repos.read().await;
        repos
            .get(&active)
            .map(|l| normalize_path(&l.entry.path))
            .ok_or_else(|| format!("Repository '{active}' not loaded"))
    }

    /// Get the current session's full chat config, if one has been set.
    pub async fn chat_config(&self) -> Option<ChatConfig> {
        self.chat_config.read().await.clone()
    }

    /// Update the in-memory chat config for the current session.
    pub async fn set_chat_config(&self, config: ChatConfig) {
        *self.chat_config.write().await = Some(config);
    }
}
