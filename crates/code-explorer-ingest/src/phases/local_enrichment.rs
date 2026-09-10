//! File-owned enrichment artifacts. Global joins are deliberately kept outside
//! this cache: a graph edge is not a complete dependency index for unresolved names.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use code_explorer_core::graph::{types::*, KnowledgeGraph};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::structure::FileEntry;
use crate::manifest::compute_hash_from_content;

#[derive(Default, Serialize, Deserialize)]
struct Cache {
    executable: String,
    hashes: BTreeMap<String, String>,
    neighbors: BTreeMap<String, BTreeSet<String>>,
    phases: BTreeMap<String, BTreeMap<String, Artifact>>,
}

#[derive(Serialize, Deserialize)]
struct Artifact {
    input: String,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphRelationship>,
    stats: serde_json::Value,
}

#[derive(Default, Serialize, Clone)]
pub struct LocalPhaseStats {
    pub scanned_files: usize,
    pub reused_files: usize,
}

pub(crate) struct LocalEnrichment {
    path: PathBuf,
    cache: Cache,
    changed: BTreeSet<String>,
    affected: BTreeSet<String>,
    pub counts: BTreeMap<String, LocalPhaseStats>,
}

impl LocalEnrichment {
    pub fn new(storage: &Path, files: &[FileEntry], incremental: bool) -> Self {
        let path = storage.join("local-enrichment.bin");
        let executable = std::env::current_exe().ok()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| format!("{}:{:?}", m.len(), m.modified().ok()))
            .unwrap_or_default();
        let mut cache = if incremental && !executable.is_empty() {
            std::fs::read_to_string(&path).ok().and_then(|text| {
                let (digest, payload) = text.split_once('\n')?;
                (compute_hash_from_content(payload) == digest)
                    .then(|| serde_json::from_str::<Cache>(payload).ok()).flatten()
            }).filter(|c| c.executable == executable).unwrap_or_default()
        } else { Cache::default() };
        let hashes: BTreeMap<_, _> = files.iter().map(|f|
            (f.path.clone(), compute_hash_from_content(&f.content))).collect();
        let changed: BTreeSet<_> = hashes.keys().chain(cache.hashes.keys())
            .filter(|p| hashes.get(*p) != cache.hashes.get(*p)).cloned().collect();
        let mut affected = changed.clone();
        for path in &changed {
            if let Some(neighbors) = cache.neighbors.get(path) {
                affected.extend(neighbors.iter().cloned());
            }
        }
        cache.executable = executable;
        cache.hashes = hashes;
        Self { path, cache, changed, affected, counts: BTreeMap::new() }
    }

    /// Only scanners whose outputs are owned by one file may use this method.
    /// Scanners receive all current nodes in that file (including enrichments
    /// from earlier phases), but no cross-file graph. Their exact input is hashed.
    pub fn run<T: Serialize + DeserializeOwned>(
        &mut self, phase: &str, graph: &mut KnowledgeGraph, files: &[FileEntry],
        scan: impl Fn(&mut KnowledgeGraph, &[FileEntry]) -> T,
    ) -> (Vec<T>, usize) {
        // Include new direct neighbors too; only expand from changed files.
        for (path, neighbors) in neighbors(graph) {
            if self.changed.contains(&path) { self.affected.extend(neighbors); }
        }
        let mut old = self.cache.phases.remove(phase).unwrap_or_default();
        let mut next = BTreeMap::new();
        let mut counts = LocalPhaseStats::default();
        let mut stats = Vec::new();
        let mut seen = BTreeSet::new();
        let mut emitted = 0;
        for file in files {
            let mut local = KnowledgeGraph::new();
            for id in graph.nodes_by_file(&file.path).unwrap_or_default() {
                if let Some(node) = graph.get_node(id) { local.add_node(node.clone()); }
            }
            let seed: BTreeSet<_> = local.iter_nodes().map(|n| n.id.clone()).collect();
            let input = compute_hash_from_content(&format!("{}:{}",
                self.cache.hashes[&file.path], serde_json::to_string(&local).unwrap()));
            let cached = old.remove(&file.path).filter(|a|
                !self.affected.contains(&file.path) && a.input == input);
            let artifact = match cached {
                Some(artifact) => { counts.reused_files += 1; artifact }
                None => {
                    counts.scanned_files += 1;
                    let stats = scan(&mut local, std::slice::from_ref(file));
                    Artifact {
                        input,
                        nodes: local.iter_nodes().filter(|n| !seed.contains(&n.id)).cloned().collect(),
                        edges: local.iter_relationships().cloned().collect(),
                        stats: serde_json::to_value(stats).unwrap(),
                    }
                }
            };
            // First declaration wins, matching the API phase's repository-wide
            // route deduplication. Do not retain edges from losing declarations.
            let mut skipped = BTreeSet::new();
            for node in &artifact.nodes {
                if seen.insert(node.id.clone()) {
                    graph.add_node(node.clone()); emitted += 1;
                } else { skipped.insert(&node.id); }
            }
            for edge in &artifact.edges {
                if !skipped.contains(&edge.source_id) && !skipped.contains(&edge.target_id) {
                    graph.add_relationship(edge.clone());
                }
            }
            stats.push(serde_json::from_value(artifact.stats.clone()).unwrap());
            next.insert(file.path.clone(), artifact);
        }
        self.cache.phases.insert(phase.to_string(), next);
        self.counts.insert(phase.to_string(), counts);
        (stats, emitted)
    }

    pub fn save(&mut self, graph: &KnowledgeGraph) {
        self.cache.neighbors = neighbors(graph);
        let save = || -> std::io::Result<()> {
            use std::io::Write;
            std::fs::create_dir_all(self.path.parent().unwrap())?;
            let payload = serde_json::to_string(&self.cache)?;
            let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default().as_nanos();
            let tmp = self.path.with_extension(format!("tmp.{}.{nonce}", std::process::id()));
            let mut file = std::fs::File::create(&tmp)?;
            writeln!(file, "{}", compute_hash_from_content(&payload))?;
            file.write_all(payload.as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(tmp, &self.path)
        };
        if let Err(error) = save() {
            tracing::warn!(%error, "Local enrichment cache not saved; next run may rescan");
        }
    }
}

fn neighbors(graph: &KnowledgeGraph) -> BTreeMap<String, BTreeSet<String>> {
    let mut result: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in graph.iter_relationships() {
        let (Some(a), Some(b)) = (graph.get_node(&edge.source_id), graph.get_node(&edge.target_id)) else { continue };
        let (a, b) = (&a.properties.file_path, &b.properties.file_path);
        if a != b && !a.is_empty() && !b.is_empty() {
            result.entry(a.clone()).or_default().insert(b.clone());
            result.entry(b.clone()).or_default().insert(a.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{api_surface, structure, todos};

    #[test]
    fn cached_file_outputs_match_global_scanners_with_duplicate_routes() {
        let files: Vec<_> = ["a.ts", "z.ts"].into_iter().map(|path| FileEntry {
            path: path.into(), content: "// TODO review\napp.get('/shared', handler);".into(),
            language: Some(code_explorer_core::config::languages::SupportedLanguage::TypeScript),
            size: 0,
        }).collect();
        let seed = || {
            let mut graph = KnowledgeGraph::new();
            structure::create_structure_nodes(&mut graph, &files);
            for file in &files {
                graph.add_node(GraphNode {
                    id: format!("Function:{}:handler", file.path), label: NodeLabel::Function,
                    properties: NodeProperties { name: "handler".into(), file_path: file.path.clone(), ..Default::default() },
                });
            }
            graph
        };
        let mut expected = seed();
        todos::scan_todos(&mut expected, &files);
        api_surface::extract_api_surface(&mut expected, &files);
        let mut actual = seed();
        // No publication: this test does not create a cache directory.
        let mut cache = LocalEnrichment::new(Path::new("unused-local-cache"), &files, false);
        cache.run("todos", &mut actual, &files, todos::scan_todos);
        cache.run("api_surface", &mut actual, &files, api_surface::extract_api_surface);
        assert_eq!(serde_json::to_value(actual).unwrap(), serde_json::to_value(expected).unwrap());
    }
}
