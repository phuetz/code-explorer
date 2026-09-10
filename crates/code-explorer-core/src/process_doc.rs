//! Process documentation data structures and graph traversal.
//!
//! Provides reusable functions for collecting process steps, evidence,
//! entities, components, and generating Mermaid diagrams from the knowledge graph.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::config::languages::SupportedLanguage;
use crate::graph::types::*;
use crate::graph::KnowledgeGraph;
use crate::trace;

// ─── Data structures ────────────────────────────────────────────────────

/// A single step in a process flow, resolved from the graph.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessStep {
    pub step_number: u32,
    pub node_id: String,
    pub name: String,
    pub class_name: Option<String>,
    pub file_path: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub return_type: Option<String>,
    pub parameter_count: Option<u32>,
    pub is_traced: bool,
    pub trace_call_count: u32,
    pub label: NodeLabel,
    pub language: Option<SupportedLanguage>,
}

/// Source of evidence with priority ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EvidenceSource {
    Code,
    Trace,
    Rag,
}

/// A piece of evidence from a single source.
#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub id: String,
    pub source: EvidenceSource,
    pub content: String,
    pub file_path: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub confidence: f64,
    pub staleness_warning: bool,
}

/// Runtime parameter extracted from a raw trace file.
#[derive(Debug, Clone, Serialize)]
pub struct TraceParam {
    pub name: String,
    pub value: String,
    pub param_type: Option<String>,
}

/// Access type for a database entity.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum EntityAccessType {
    Read,
    Write,
    ReadWrite,
}

/// A database entity accessed by the process.
#[derive(Debug, Clone, Serialize)]
pub struct EntityInfo {
    pub node_id: String,
    pub name: String,
    pub db_table_name: Option<String>,
    pub access_type: EntityAccessType,
    pub accessed_by_step: Option<u32>,
}

/// A component (Class/Service/Controller) involved in the process.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentInfo {
    pub node_id: String,
    pub name: String,
    pub label: NodeLabel,
    pub file_path: String,
    pub step_count: usize,
    pub dependencies: Vec<String>,
}

/// Full evidence collection for a process.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessEvidence {
    pub process_id: String,
    pub process_name: String,
    pub process_type: Option<ProcessType>,
    pub entry_point_name: Option<String>,
    pub entry_point_file: Option<String>,
    pub terminal_name: Option<String>,
    pub communities: Vec<String>,
    pub steps: Vec<ProcessStep>,
    pub step_evidence: Vec<Vec<Evidence>>,
    pub global_rag_evidence: Vec<Evidence>,
    pub entities: Vec<EntityInfo>,
    pub components: Vec<ComponentInfo>,
    pub trace_coverage_pct: f64,
    pub dead_code_candidates: Vec<String>,
}

// ─── Process step collection ────────────────────────────────────────────

/// Collect ordered steps for a Process node by reading StepInProcess edges.
pub fn collect_process_steps(graph: &KnowledgeGraph, process_id: &str) -> Vec<ProcessStep> {
    let mut steps: Vec<ProcessStep> = Vec::new();

    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::StepInProcess && rel.target_id == process_id {
            let step_number = rel.step.unwrap_or(0);
            if let Some(node) = graph.get_node(&rel.source_id) {
                // Find parent class via reverse HasMethod edge
                let class_name = find_parent_class(graph, &rel.source_id);

                steps.push(ProcessStep {
                    step_number,
                    node_id: rel.source_id.clone(),
                    name: node.properties.name.clone(),
                    class_name,
                    file_path: node.properties.file_path.clone(),
                    start_line: node.properties.start_line,
                    end_line: node.properties.end_line,
                    return_type: node.properties.return_type.clone(),
                    parameter_count: node.properties.parameter_count,
                    is_traced: node.properties.is_traced.unwrap_or(false),
                    trace_call_count: node.properties.trace_call_count.unwrap_or(0),
                    label: node.label,
                    language: node.properties.language,
                });
            }
        }
    }

    steps.sort_by_key(|s| s.step_number);
    // Renumber if gaps exist
    for (i, step) in steps.iter_mut().enumerate() {
        step.step_number = (i + 1) as u32;
    }
    steps
}

/// Find the parent class/service name for a method node via reverse HasMethod edge.
fn find_parent_class(graph: &KnowledgeGraph, method_id: &str) -> Option<String> {
    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::HasMethod && rel.target_id == method_id {
            if let Some(parent) = graph.get_node(&rel.source_id) {
                return Some(parent.properties.name.clone());
            }
        }
    }
    None
}

// ─── Entity collection ──────────────────────────────────────────────────

/// Collect DB entities accessed by any step in the process.
pub fn collect_process_entities(graph: &KnowledgeGraph, steps: &[ProcessStep]) -> Vec<EntityInfo> {
    let step_ids: HashSet<&str> = steps.iter().map(|s| s.node_id.as_str()).collect();
    let step_map: HashMap<&str, u32> = steps
        .iter()
        .map(|s| (s.node_id.as_str(), s.step_number))
        .collect();
    let mut entities = Vec::new();
    let mut seen = HashSet::new();

    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::MapsToEntity
            || rel.rel_type == RelationshipType::Accesses
        {
            // Check if source is one of our steps, or if source's parent class is
            let is_step = step_ids.contains(rel.source_id.as_str());
            if !is_step {
                continue;
            }

            if let Some(entity_node) = graph.get_node(&rel.target_id) {
                if matches!(
                    entity_node.label,
                    NodeLabel::DbEntity | NodeLabel::DbContext
                ) && seen.insert(rel.target_id.clone())
                {
                    // Order matters: the ReadWrite branch must run BEFORE the
                    // Write branch, otherwise a reason like "read+write" would
                    // hit the Write arm (because it contains "write") and the
                    // ReadWrite variant would be unreachable dead code.
                    let access_type = if rel.reason.contains("read") && rel.reason.contains("write")
                    {
                        EntityAccessType::ReadWrite
                    } else if rel.reason.contains("write")
                        || rel.reason.contains("insert")
                        || rel.reason.contains("update")
                        || rel.reason.contains("delete")
                    {
                        EntityAccessType::Write
                    } else {
                        EntityAccessType::Read
                    };

                    entities.push(EntityInfo {
                        node_id: rel.target_id.clone(),
                        name: entity_node.properties.name.clone(),
                        db_table_name: entity_node.properties.db_table_name.clone(),
                        access_type,
                        accessed_by_step: step_map.get(rel.source_id.as_str()).copied(),
                    });
                }
            }
        }
    }

    entities
}

// ─── Component collection ───────────────────────────────────────────────

/// Collect components (classes/services) that own the process steps.
pub fn collect_process_components(
    graph: &KnowledgeGraph,
    steps: &[ProcessStep],
) -> Vec<ComponentInfo> {
    let mut comp_map: HashMap<String, (String, NodeLabel, String, usize)> = HashMap::new();

    for step in steps {
        if let Some(class_name) = &step.class_name {
            let entry = comp_map.entry(class_name.clone()).or_insert_with(|| {
                // Find the class node
                let (label, file_path) = graph
                    .iter_nodes()
                    .find(|n| {
                        n.properties.name == *class_name
                            && matches!(
                                n.label,
                                NodeLabel::Class
                                    | NodeLabel::Service
                                    | NodeLabel::Controller
                                    | NodeLabel::Repository
                                    | NodeLabel::Interface
                                    | NodeLabel::Struct
                            )
                    })
                    .map(|n| (n.label, n.properties.file_path.clone()))
                    .unwrap_or((NodeLabel::Class, step.file_path.clone()));
                (class_name.clone(), label, file_path, 0)
            });
            entry.3 += 1;
        }
    }

    let mut components: Vec<ComponentInfo> = comp_map
        .into_iter()
        .map(|(name, (node_name, label, file_path, count))| {
            // Find DependsOn edges for this component
            let deps: Vec<String> = graph
                .iter_relationships()
                .filter(|r| r.rel_type == RelationshipType::DependsOn)
                .filter(|r| {
                    graph
                        .get_node(&r.source_id)
                        .map(|n| n.properties.name == name)
                        .unwrap_or(false)
                })
                .filter_map(|r| {
                    graph
                        .get_node(&r.target_id)
                        .map(|n| n.properties.name.clone())
                })
                .collect();

            ComponentInfo {
                node_id: format!("Component:{}", node_name),
                name,
                label,
                file_path,
                step_count: count,
                dependencies: deps,
            }
        })
        .collect();

    components.sort_by(|a, b| b.step_count.cmp(&a.step_count));
    components
}

// ─── Evidence collection ────────────────────────────────────────────��───

/// Collect code evidence for a process step (source code snippet).
pub fn collect_code_evidence(step: &ProcessStep, repo_path: &Path) -> Vec<Evidence> {
    let mut evidence = Vec::new();

    if let (Some(start), Some(end)) = (step.start_line, step.end_line) {
        // Path traversal guard: `step.file_path` originates from a graph
        // snapshot that could contain `..` segments (corrupted or hand-
        // crafted), and we must not let them escape the repo root and
        // exfiltrate arbitrary files into the generated documentation.
        let full_path = repo_path.join(&step.file_path);
        let source_safe = match (full_path.canonicalize().ok(), repo_path.canonicalize().ok()) {
            (Some(canon), Some(root)) => canon.starts_with(&root),
            _ => false,
        };
        if source_safe {
            if let Some(source) = trace::extract_source_lines(&full_path, start, end) {
                evidence.push(Evidence {
                    id: format!("CODE:{}", step.node_id),
                    source: EvidenceSource::Code,
                    content: source,
                    file_path: step.file_path.clone(),
                    start_line: Some(start),
                    end_line: Some(end),
                    confidence: 1.0,
                    staleness_warning: false,
                });
            }
        }
    }

    evidence
}

/// Collect trace evidence for a step from node properties.
pub fn collect_trace_evidence(step: &ProcessStep) -> Vec<Evidence> {
    if !step.is_traced {
        return Vec::new();
    }

    vec![Evidence {
        id: format!("TRACE:step_{}", step.step_number),
        source: EvidenceSource::Trace,
        content: format!(
            "Method `{}` is instrumented with tracing. Call count: {} invocations.",
            step.name, step.trace_call_count
        ),
        file_path: step.file_path.clone(),
        start_line: step.start_line,
        end_line: step.end_line,
        confidence: 0.9,
        staleness_warning: false,
    }]
}

/// Collect RAG evidence for symbols by following Mentions relationships from DocChunk nodes.
pub fn collect_rag_evidence(graph: &KnowledgeGraph, symbol_node_ids: &[&str]) -> Vec<Evidence> {
    let target_set: HashSet<&str> = symbol_node_ids.iter().copied().collect();
    let mut evidence = Vec::new();
    let mut seen_chunks = HashSet::new();

    for rel in graph.iter_relationships() {
        if rel.rel_type == RelationshipType::Mentions
            && target_set.contains(rel.target_id.as_str())
            && seen_chunks.insert(rel.source_id.clone())
        {
            if let Some(chunk_node) = graph.get_node(&rel.source_id) {
                if let Some(content) = &chunk_node.properties.content {
                    evidence.push(Evidence {
                        id: format!("RAG:{}", chunk_node.id),
                        source: EvidenceSource::Rag,
                        content: content.clone(),
                        file_path: chunk_node.properties.file_path.clone(),
                        start_line: None,
                        end_line: None,
                        confidence: rel.confidence.min(0.8),
                        staleness_warning: true,
                    });
                }
            }
        }
    }

    evidence
}

/// Full evidence collection for a process, combining all three sources.
pub fn collect_full_evidence(
    graph: &KnowledgeGraph,
    process_id: &str,
    repo_path: &Path,
) -> ProcessEvidence {
    let process_node = graph.get_node(process_id);
    let process_name = process_node
        .map(|n| n.properties.name.clone())
        .unwrap_or_default();
    let process_type = process_node.and_then(|n| n.properties.process_type);
    let communities = process_node
        .and_then(|n| n.properties.communities.clone())
        .unwrap_or_default();

    let steps = collect_process_steps(graph, process_id);

    // Entry point and terminal
    let entry_point_id = process_node.and_then(|n| n.properties.entry_point_id.clone());
    let terminal_id = process_node.and_then(|n| n.properties.terminal_id.clone());

    let entry_point_name = entry_point_id
        .as_deref()
        .and_then(|id| graph.get_node(id))
        .map(|n| n.properties.name.clone());
    let entry_point_file = entry_point_id
        .as_deref()
        .and_then(|id| graph.get_node(id))
        .map(|n| n.properties.file_path.clone());
    let terminal_name = terminal_id
        .as_deref()
        .and_then(|id| graph.get_node(id))
        .map(|n| n.properties.name.clone());

    // Collect per-step evidence
    let mut step_evidence = Vec::new();
    let symbol_ids: Vec<&str> = steps.iter().map(|s| s.node_id.as_str()).collect();

    for step in &steps {
        let mut ev = Vec::new();
        ev.extend(collect_code_evidence(step, repo_path));
        ev.extend(collect_trace_evidence(step));
        step_evidence.push(ev);
    }

    // RAG evidence for all symbols in the process
    let global_rag_evidence = collect_rag_evidence(graph, &symbol_ids);

    // Entities and components
    let entities = collect_process_entities(graph, &steps);
    let components = collect_process_components(graph, &steps);

    // Trace coverage
    let traced_count = steps.iter().filter(|s| s.is_traced).count();
    let trace_coverage_pct = if steps.is_empty() {
        0.0
    } else {
        traced_count as f64 / steps.len() as f64 * 100.0
    };

    // Dead code candidates
    let dead_code_candidates: Vec<String> = steps
        .iter()
        .filter(|s| {
            graph
                .get_node(&s.node_id)
                .and_then(|n| n.properties.is_dead_candidate)
                .unwrap_or(false)
        })
        .map(|s| s.node_id.clone())
        .collect();

    ProcessEvidence {
        process_id: process_id.to_string(),
        process_name,
        process_type,
        entry_point_name,
        entry_point_file,
        terminal_name,
        communities,
        steps,
        step_evidence,
        global_rag_evidence,
        entities,
        components,
        trace_coverage_pct,
        dead_code_candidates,
    }
}

// ─── Diagram generation ─────────────────────────────────────────────────

/// Generate a Mermaid sequence diagram from process steps.
pub fn generate_sequence_diagram(steps: &[ProcessStep]) -> String {
    if steps.is_empty() {
        return String::from("sequenceDiagram\n    Note over System: No steps detected");
    }

    let mut lines = vec![String::from("sequenceDiagram")];

    // Declare participants in order of first appearance
    let mut seen_participants = Vec::new();
    for step in steps {
        let participant = step.class_name.as_deref().unwrap_or("System");
        if !seen_participants.contains(&participant) {
            seen_participants.push(participant);
            lines.push(format!(
                "    participant {} as {}",
                sanitize_mermaid_id(participant),
                escape_mermaid_label(participant)
            ));
        }
    }

    // Generate arrows between consecutive steps
    for window in steps.windows(2) {
        let from = window[0].class_name.as_deref().unwrap_or("System");
        let to = window[1].class_name.as_deref().unwrap_or("System");
        let label = escape_mermaid_label(&window[1].name);

        if from == to {
            lines.push(format!(
                "    {}->>{}:  {}()",
                sanitize_mermaid_id(from),
                sanitize_mermaid_id(to),
                label
            ));
        } else {
            lines.push(format!(
                "    {}->>{}: {}()",
                sanitize_mermaid_id(from),
                sanitize_mermaid_id(to),
                label
            ));
        }

        if window[1].is_traced {
            lines.push(format!(
                "    Note right of {}: Traced ({} calls)",
                sanitize_mermaid_id(to),
                window[1].trace_call_count
            ));
        }
    }

    // Add first step if only one
    if steps.len() == 1 {
        let s = &steps[0];
        let participant = s.class_name.as_deref().unwrap_or("System");
        lines.push(format!(
            "    {}->>{}:  {}()",
            sanitize_mermaid_id(participant),
            sanitize_mermaid_id(participant),
            escape_mermaid_label(&s.name)
        ));
    }

    lines.join("\n")
}

/// Generate a Mermaid component diagram from components.
pub fn generate_component_diagram(components: &[ComponentInfo]) -> String {
    if components.is_empty() {
        return String::from("graph LR\n    NoComponents[No components detected]");
    }

    let mut lines = vec![String::from("graph LR")];

    for comp in components {
        let id = sanitize_mermaid_id(&comp.name);
        let label = format!(
            "{}\\n({})",
            escape_mermaid_label(&comp.name),
            comp.label.as_str()
        );
        lines.push(format!("    {}[\"{}\" ]", id, label));
    }

    // Add dependency edges
    let comp_names: HashSet<&str> = components.iter().map(|c| c.name.as_str()).collect();
    for comp in components {
        for dep in &comp.dependencies {
            if comp_names.contains(dep.as_str()) {
                lines.push(format!(
                    "    {} --> {}",
                    sanitize_mermaid_id(&comp.name),
                    sanitize_mermaid_id(dep)
                ));
            }
        }
    }

    lines.join("\n")
}

fn sanitize_mermaid_id(id: &str) -> String {
    id.replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
}

/// Escape a label for safe use inside Mermaid `["..."]` quoted strings and
/// `participant X as <label>` declarations. Uses HTML entity encoding — the
/// backslash escapes that some toolchains recommend are not honored by
/// Mermaid 11.x, but `&quot;`, `&lt;`, `&gt;`, `&amp;`, `&#91;`, `&#93;` are.
/// Without this, C# generics like `List<string>` or indexers like `Foo[int]`
/// corrupt the diagram source and the whole graph fails to render.
fn escape_mermaid_label(label: &str) -> String {
    label
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('[', "&#91;")
        .replace(']', "&#93;")
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::knowledge_graph::KnowledgeGraph;

    fn build_test_graph() -> KnowledgeGraph {
        let mut g = KnowledgeGraph::new();

        // Process node
        g.add_node(GraphNode {
            id: "Process:HandleRequest".to_string(),
            label: NodeLabel::Process,
            properties: NodeProperties {
                name: "HandleRequest".to_string(),
                process_type: Some(ProcessType::CrossCommunity),
                step_count: Some(3),
                entry_point_id: Some("Method:Controller.Handle".to_string()),
                terminal_id: Some("Method:Repo.Save".to_string()),
                communities: Some(vec!["Web".to_string(), "Data".to_string()]),
                ..Default::default()
            },
        });

        // Step nodes
        g.add_node(GraphNode {
            id: "Method:Controller.Handle".to_string(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "Handle".to_string(),
                file_path: "src/Controller.cs".to_string(),
                start_line: Some(10),
                end_line: Some(20),
                is_traced: Some(true),
                trace_call_count: Some(42),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Method:Service.Process".to_string(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "Process".to_string(),
                file_path: "src/Service.cs".to_string(),
                start_line: Some(30),
                end_line: Some(50),
                is_traced: Some(false),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Method:Repo.Save".to_string(),
            label: NodeLabel::Method,
            properties: NodeProperties {
                name: "Save".to_string(),
                file_path: "src/Repo.cs".to_string(),
                start_line: Some(5),
                end_line: Some(15),
                is_traced: Some(true),
                trace_call_count: Some(100),
                ..Default::default()
            },
        });

        // Parent classes
        g.add_node(GraphNode {
            id: "Class:Controller".to_string(),
            label: NodeLabel::Controller,
            properties: NodeProperties {
                name: "Controller".to_string(),
                file_path: "src/Controller.cs".to_string(),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Class:Service".to_string(),
            label: NodeLabel::Service,
            properties: NodeProperties {
                name: "Service".to_string(),
                file_path: "src/Service.cs".to_string(),
                ..Default::default()
            },
        });
        g.add_node(GraphNode {
            id: "Class:Repo".to_string(),
            label: NodeLabel::Repository,
            properties: NodeProperties {
                name: "Repo".to_string(),
                file_path: "src/Repo.cs".to_string(),
                ..Default::default()
            },
        });

        // HasMethod edges
        g.add_relationship(GraphRelationship {
            id: "hm1".into(),
            source_id: "Class:Controller".into(),
            target_id: "Method:Controller.Handle".into(),
            rel_type: RelationshipType::HasMethod,
            confidence: 1.0,
            reason: "".into(),
            step: None,
        });
        g.add_relationship(GraphRelationship {
            id: "hm2".into(),
            source_id: "Class:Service".into(),
            target_id: "Method:Service.Process".into(),
            rel_type: RelationshipType::HasMethod,
            confidence: 1.0,
            reason: "".into(),
            step: None,
        });
        g.add_relationship(GraphRelationship {
            id: "hm3".into(),
            source_id: "Class:Repo".into(),
            target_id: "Method:Repo.Save".into(),
            rel_type: RelationshipType::HasMethod,
            confidence: 1.0,
            reason: "".into(),
            step: None,
        });

        // StepInProcess edges
        g.add_relationship(GraphRelationship {
            id: "sp1".into(),
            source_id: "Method:Controller.Handle".into(),
            target_id: "Process:HandleRequest".into(),
            rel_type: RelationshipType::StepInProcess,
            confidence: 1.0,
            reason: "".into(),
            step: Some(1),
        });
        g.add_relationship(GraphRelationship {
            id: "sp2".into(),
            source_id: "Method:Service.Process".into(),
            target_id: "Process:HandleRequest".into(),
            rel_type: RelationshipType::StepInProcess,
            confidence: 1.0,
            reason: "".into(),
            step: Some(2),
        });
        g.add_relationship(GraphRelationship {
            id: "sp3".into(),
            source_id: "Method:Repo.Save".into(),
            target_id: "Process:HandleRequest".into(),
            rel_type: RelationshipType::StepInProcess,
            confidence: 1.0,
            reason: "".into(),
            step: Some(3),
        });

        // DependsOn
        g.add_relationship(GraphRelationship {
            id: "dep1".into(),
            source_id: "Class:Controller".into(),
            target_id: "Class:Service".into(),
            rel_type: RelationshipType::DependsOn,
            confidence: 1.0,
            reason: "".into(),
            step: None,
        });
        g.add_relationship(GraphRelationship {
            id: "dep2".into(),
            source_id: "Class:Service".into(),
            target_id: "Class:Repo".into(),
            rel_type: RelationshipType::DependsOn,
            confidence: 1.0,
            reason: "".into(),
            step: None,
        });

        g
    }

    #[test]
    fn test_collect_process_steps() {
        let graph = build_test_graph();
        let steps = collect_process_steps(&graph, "Process:HandleRequest");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].name, "Handle");
        assert_eq!(steps[0].step_number, 1);
        assert_eq!(steps[0].class_name, Some("Controller".to_string()));
        assert!(steps[0].is_traced);
        assert_eq!(steps[1].name, "Process");
        assert_eq!(steps[2].name, "Save");
    }

    #[test]
    fn test_collect_process_components() {
        let graph = build_test_graph();
        let steps = collect_process_steps(&graph, "Process:HandleRequest");
        let components = collect_process_components(&graph, &steps);
        assert_eq!(components.len(), 3);
        // Controller depends on Service
        let ctrl = components.iter().find(|c| c.name == "Controller").unwrap();
        assert!(ctrl.dependencies.contains(&"Service".to_string()));
    }

    #[test]
    fn test_trace_coverage() {
        let graph = build_test_graph();
        let steps = collect_process_steps(&graph, "Process:HandleRequest");
        let traced = steps.iter().filter(|s| s.is_traced).count();
        let pct = traced as f64 / steps.len() as f64 * 100.0;
        assert!((pct - 66.6).abs() < 1.0); // 2/3 traced
    }

    #[test]
    fn test_generate_sequence_diagram() {
        let graph = build_test_graph();
        let steps = collect_process_steps(&graph, "Process:HandleRequest");
        let diagram = generate_sequence_diagram(&steps);
        assert!(diagram.contains("sequenceDiagram"));
        assert!(diagram.contains("Controller"));
        assert!(diagram.contains("Service"));
        assert!(diagram.contains("Repo"));
        assert!(diagram.contains("Process()"));
        assert!(diagram.contains("Save()"));
    }

    #[test]
    fn test_generate_component_diagram() {
        let graph = build_test_graph();
        let steps = collect_process_steps(&graph, "Process:HandleRequest");
        let components = collect_process_components(&graph, &steps);
        let diagram = generate_component_diagram(&components);
        assert!(diagram.contains("graph LR"));
        assert!(diagram.contains("Controller"));
        assert!(diagram.contains("-->"));
    }

    #[test]
    fn test_empty_process() {
        let graph = KnowledgeGraph::new();
        let steps = collect_process_steps(&graph, "NonExistent");
        assert!(steps.is_empty());
        let diagram = generate_sequence_diagram(&steps);
        assert!(diagram.contains("No steps detected"));
    }

    #[test]
    fn test_collect_full_evidence() {
        let graph = build_test_graph();
        let evidence = collect_full_evidence(&graph, "Process:HandleRequest", Path::new("."));
        assert_eq!(evidence.steps.len(), 3);
        assert_eq!(evidence.process_name, "HandleRequest");
        assert_eq!(evidence.process_type, Some(ProcessType::CrossCommunity));
        assert!((evidence.trace_coverage_pct - 66.6).abs() < 1.0);
        assert_eq!(evidence.communities.len(), 2);
        assert_eq!(evidence.entry_point_name, Some("Handle".to_string()));
        assert_eq!(evidence.terminal_name, Some("Save".to_string()));
    }
}
