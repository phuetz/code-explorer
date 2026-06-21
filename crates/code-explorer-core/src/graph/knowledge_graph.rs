use std::collections::HashMap;

use super::types::{GraphNode, GraphRelationship};

/// In-memory knowledge graph.
///
/// Stores nodes and relationships in HashMaps for O(1) lookup.
/// Maintains a secondary index from file_path to node IDs.
///
/// Not thread-safe: callers must wrap in `Arc<RwLock<>>` when sharing.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct KnowledgeGraph {
    nodes: HashMap<String, GraphNode>,
    relationships: HashMap<String, GraphRelationship>,
    /// Secondary index: file_path -> vec of node IDs in that file
    file_index: HashMap<String, Vec<String>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(nodes: usize, relationships: usize) -> Self {
        Self {
            nodes: HashMap::with_capacity(nodes),
            relationships: HashMap::with_capacity(relationships),
            file_index: HashMap::new(),
        }
    }

    // ─── Node operations ─────────────────────────────────────────────

    pub fn add_node(&mut self, node: GraphNode) {
        let file_path = node.properties.file_path.clone();
        let id = node.id.clone();
        // If a node with this ID already exists, remove its previous file_index entry
        // first so we don't accumulate duplicates on re-insertion (which would
        // double-count nodes in nodes_by_file and cause incorrect remove_nodes_by_file
        // counts).
        if let Some(prev) = self.nodes.insert(id.clone(), node) {
            if prev.properties.file_path != file_path {
                if let Some(ids) = self.file_index.get_mut(&prev.properties.file_path) {
                    ids.retain(|existing| existing != &id);
                    if ids.is_empty() {
                        self.file_index.remove(&prev.properties.file_path);
                    }
                }
            } else {
                // Same file: avoid pushing a duplicate id below
                if let Some(ids) = self.file_index.get(&file_path) {
                    if ids.iter().any(|existing| existing == &id) {
                        return;
                    }
                }
            }
        }
        self.file_index.entry(file_path).or_default().push(id);
    }

    pub fn get_node(&self, id: &str) -> Option<&GraphNode> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut GraphNode> {
        self.nodes.get_mut(id)
    }

    pub fn remove_node(&mut self, id: &str) -> bool {
        if let Some(node) = self.nodes.remove(id) {
            // Remove from file index
            if let Some(ids) = self.file_index.get_mut(&node.properties.file_path) {
                ids.retain(|nid| nid != id);
                if ids.is_empty() {
                    self.file_index.remove(&node.properties.file_path);
                }
            }
            // Remove dangling relationships that reference this node
            self.relationships
                .retain(|_, rel| rel.source_id != id && rel.target_id != id);
            true
        } else {
            false
        }
    }

    /// Remove all nodes belonging to a file. Returns the number of nodes that
    /// were actually removed from `self.nodes`. Stale IDs in the file_index
    /// (e.g. from a path-normalization mismatch) are skipped, so the count
    /// reflects real removals — callers like the incremental update flow rely
    /// on this for accurate diff stats.
    pub fn remove_nodes_by_file(&mut self, file_path: &str) -> usize {
        if let Some(ids) = self.file_index.remove(file_path) {
            let mut count = 0usize;
            for id in &ids {
                if self.nodes.remove(id).is_some() {
                    count += 1;
                }
            }
            // Also remove relationships referencing these nodes
            let id_set: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
            self.relationships.retain(|_, rel| {
                !id_set.contains(rel.source_id.as_str()) && !id_set.contains(rel.target_id.as_str())
            });
            count
        } else {
            0
        }
    }

    /// Remove all nodes with a given label and their associated relationships.
    /// Returns count of removed nodes.
    ///
    /// Uses a single relationship scan (O(M)) instead of per-node scans.
    pub fn remove_nodes_by_label(&mut self, label: super::types::NodeLabel) -> usize {
        let ids_to_remove: Vec<String> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.label == label)
            .map(|(id, _)| id.clone())
            .collect();
        let count = ids_to_remove.len();
        // Remove from node map and file index
        for id in &ids_to_remove {
            if let Some(node) = self.nodes.remove(id) {
                if let Some(v) = self.file_index.get_mut(&node.properties.file_path) {
                    v.retain(|nid| nid != id);
                    if v.is_empty() {
                        self.file_index.remove(&node.properties.file_path);
                    }
                }
            }
        }
        // Single relationship scan
        let id_set: std::collections::HashSet<&str> =
            ids_to_remove.iter().map(|s| s.as_str()).collect();
        self.relationships.retain(|_, rel| {
            !id_set.contains(rel.source_id.as_str()) && !id_set.contains(rel.target_id.as_str())
        });
        count
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn iter_nodes(&self) -> impl Iterator<Item = &GraphNode> {
        self.nodes.values()
    }

    /// Get all node IDs for a given file path.
    pub fn nodes_by_file(&self, file_path: &str) -> Option<&[String]> {
        self.file_index.get(file_path).map(|v| v.as_slice())
    }

    // ─── Relationship operations ─────────────────────────────────────

    pub fn add_relationship(&mut self, rel: GraphRelationship) {
        self.relationships.insert(rel.id.clone(), rel);
    }

    /// Remove a relationship by id, returning it if present.
    pub fn remove_relationship(&mut self, id: &str) -> Option<GraphRelationship> {
        self.relationships.remove(id)
    }

    pub fn get_relationship(&self, id: &str) -> Option<&GraphRelationship> {
        self.relationships.get(id)
    }

    pub fn relationship_count(&self) -> usize {
        self.relationships.len()
    }

    pub fn iter_relationships(&self) -> impl Iterator<Item = &GraphRelationship> {
        self.relationships.values()
    }

    // ─── Bulk accessors ──────────────────────────────────────────────

    pub fn nodes(&self) -> Vec<&GraphNode> {
        self.nodes.values().collect()
    }

    pub fn relationships(&self) -> Vec<&GraphRelationship> {
        self.relationships.values().collect()
    }

    /// Iterate over all nodes, calling `f` for each.
    pub fn for_each_node<F>(&self, mut f: F)
    where
        F: FnMut(&GraphNode),
    {
        for node in self.nodes.values() {
            f(node);
        }
    }

    /// Iterate over all relationships, calling `f` for each.
    pub fn for_each_relationship<F>(&self, mut f: F)
    where
        F: FnMut(&GraphRelationship),
    {
        for rel in self.relationships.values() {
            f(rel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::*;

    fn make_node(id: &str, label: NodeLabel, file_path: &str) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label,
            properties: NodeProperties {
                name: id.to_string(),
                file_path: file_path.to_string(),
                ..Default::default()
            },
        }
    }

    fn make_rel(id: &str, src: &str, tgt: &str, rel_type: RelationshipType) -> GraphRelationship {
        GraphRelationship {
            id: id.to_string(),
            source_id: src.to_string(),
            target_id: tgt.to_string(),
            rel_type,
            confidence: 1.0,
            reason: "test".to_string(),
            step: None,
        }
    }

    #[test]
    fn test_add_and_get_node() {
        let mut graph = KnowledgeGraph::new();
        let node = make_node("Function:main", NodeLabel::Function, "src/main.ts");
        graph.add_node(node);

        assert_eq!(graph.node_count(), 1);
        let retrieved = graph.get_node("Function:main").unwrap();
        assert_eq!(retrieved.label, NodeLabel::Function);
    }

    #[test]
    fn test_add_and_get_relationship() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_node("f1", NodeLabel::Function, "a.ts"));
        graph.add_node(make_node("f2", NodeLabel::Function, "b.ts"));
        graph.add_relationship(make_rel("r1", "f1", "f2", RelationshipType::Calls));

        assert_eq!(graph.relationship_count(), 1);
        let rel = graph.get_relationship("r1").unwrap();
        assert_eq!(rel.rel_type, RelationshipType::Calls);
    }

    #[test]
    fn test_remove_node() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_node("f1", NodeLabel::Function, "a.ts"));
        assert!(graph.remove_node("f1"));
        assert_eq!(graph.node_count(), 0);
        assert!(!graph.remove_node("f1")); // already removed
    }

    #[test]
    fn test_remove_nodes_by_file() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_node("f1", NodeLabel::Function, "a.ts"));
        graph.add_node(make_node("f2", NodeLabel::Function, "a.ts"));
        graph.add_node(make_node("f3", NodeLabel::Function, "b.ts"));
        graph.add_relationship(make_rel("r1", "f1", "f3", RelationshipType::Calls));

        let removed = graph.remove_nodes_by_file("a.ts");
        assert_eq!(removed, 2);
        assert_eq!(graph.node_count(), 1);
        // Relationship involving removed node should also be gone
        assert_eq!(graph.relationship_count(), 0);
    }

    #[test]
    fn test_remove_nodes_by_label() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_node("f1", NodeLabel::Function, "a.ts"));
        graph.add_node(make_node("f2", NodeLabel::Function, "b.ts"));
        graph.add_node(make_node("m1", NodeLabel::Method, "a.ts"));
        graph.add_relationship(make_rel("r1", "f1", "m1", RelationshipType::Calls));
        graph.add_relationship(make_rel("r2", "f2", "m1", RelationshipType::Calls));

        let removed = graph.remove_nodes_by_label(NodeLabel::Function);
        assert_eq!(removed, 2);
        assert_eq!(graph.node_count(), 1); // only m1 remains
        assert_eq!(graph.relationship_count(), 0); // all rels removed
        assert!(graph.get_node("m1").is_some());
    }

    #[test]
    fn test_file_index() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_node("f1", NodeLabel::Function, "a.ts"));
        graph.add_node(make_node("f2", NodeLabel::Method, "a.ts"));
        graph.add_node(make_node("f3", NodeLabel::Function, "b.ts"));

        let ids = graph.nodes_by_file("a.ts").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"f1".to_string()));
        assert!(ids.contains(&"f2".to_string()));

        assert!(graph.nodes_by_file("c.ts").is_none());
    }

    #[test]
    fn test_iterators() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(make_node("f1", NodeLabel::Function, "a.ts"));
        graph.add_node(make_node("f2", NodeLabel::Function, "b.ts"));

        let count = graph.iter_nodes().count();
        assert_eq!(count, 2);

        let mut for_each_count = 0;
        graph.for_each_node(|_| for_each_count += 1);
        assert_eq!(for_each_count, 2);
    }
}
