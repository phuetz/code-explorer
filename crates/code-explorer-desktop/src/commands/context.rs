use tauri::State;

use code_explorer_core::graph::types::RelationshipType;

use crate::commands::shared::node_to_cyto;
use crate::state::AppState;
use crate::types::*;

/// Get 360-degree context for a symbol.
#[tauri::command]
pub async fn get_symbol_context(
    state: State<'_, AppState>,
    node_id: String,
) -> Result<SymbolContext, String> {
    let (graph, indexes, _fts, _repo_path) = state.get_repo(None).await?;

    let node = graph
        .get_node(&node_id)
        .ok_or_else(|| format!("Node '{}' not found", node_id))?;

    let cyto_node = node_to_cyto(node);

    // Collect related nodes by relationship type
    let mut callers = Vec::new();
    let mut callees = Vec::new();
    let mut imports = Vec::new();
    let mut imported_by = Vec::new();
    let mut inherits = Vec::new();
    let mut inherited_by = Vec::new();

    // Outgoing relationships
    if let Some(outs) = indexes.outgoing.get(&node_id) {
        for (target_id, rel_type) in outs {
            if let Some(target) = graph.get_node(target_id) {
                let related = RelatedNode {
                    id: target.id.clone(),
                    name: target.properties.name.clone(),
                    label: target.label.as_str().to_string(),
                    file_path: target.properties.file_path.clone(),
                };
                match rel_type {
                    RelationshipType::Calls => callees.push(related),
                    RelationshipType::Imports => imports.push(related),
                    RelationshipType::Inherits
                    | RelationshipType::Extends
                    | RelationshipType::Implements => inherits.push(related),
                    _ => {}
                }
            }
        }
    }

    // Incoming relationships
    if let Some(ins) = indexes.incoming.get(&node_id) {
        for (source_id, rel_type) in ins {
            if let Some(source) = graph.get_node(source_id) {
                let related = RelatedNode {
                    id: source.id.clone(),
                    name: source.properties.name.clone(),
                    label: source.label.as_str().to_string(),
                    file_path: source.properties.file_path.clone(),
                };
                match rel_type {
                    RelationshipType::Calls => callers.push(related),
                    RelationshipType::Imports => imported_by.push(related),
                    RelationshipType::Inherits
                    | RelationshipType::Extends
                    | RelationshipType::Implements => inherited_by.push(related),
                    _ => {}
                }
            }
        }
    }

    // Find community membership
    let community = find_community(&graph, &indexes, &node_id);

    Ok(SymbolContext {
        node: cyto_node,
        callers,
        callees,
        imports,
        imported_by,
        inherits,
        inherited_by,
        community,
    })
}

fn find_community(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    indexes: &code_explorer_db::inmemory::cypher::GraphIndexes,
    node_id: &str,
) -> Option<CommunityInfo> {
    // MEMBER_OF edges go: member --MEMBER_OF--> community
    // So we check outgoing edges from this node to find its community.
    if let Some(outs) = indexes.outgoing.get(node_id) {
        for (target_id, rel_type) in outs {
            if *rel_type == RelationshipType::MemberOf {
                if let Some(community_node) = graph.get_node(target_id) {
                    return Some(CommunityInfo {
                        id: community_node.id.clone(),
                        name: community_node.properties.name.clone(),
                        description: community_node.properties.description.clone(),
                        member_count: community_node.properties.symbol_count,
                        cohesion: community_node.properties.cohesion,
                    });
                }
            }
        }
    }

    None
}
