//! Execute CQL queries against a [`KnowledgeGraph`].
//!
//! The executor builds lightweight in-memory indexes (adjacency lists, label
//! index, name index) on the first query, then pattern-matches and filters
//! according to the AST produced by [`crate::parser`].

use std::collections::HashMap;

use code_explorer_core::graph::types::{GraphNode, GraphRelationship, NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;
use thiserror::Error;
use tracing::debug;

use crate::ast::*;

// ─── Error ───────────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("Unbound variable: {0}")]
    UnboundVariable(String),
    #[error("Type error: {0}")]
    TypeError(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

// ─── Result value ────────────────────────────────────────────────────────

/// A single cell in a query result row.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    /// A node reference (carries the node ID).
    Node(String),
    List(Vec<Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            Value::Node(id) => Some(id),
            _ => None,
        }
    }

    /// Type tag for ORDER BY (lower tag sorts first across heterogeneous types).
    fn sort_type_tag(&self) -> u8 {
        match self {
            Value::Null => 0,
            Value::Bool(_) => 1,
            Value::Int(_) | Value::Float(_) => 2,
            Value::String(_) => 4,
            Value::Node(_) => 5,
            Value::List(_) => 6,
        }
    }

    /// Total ordering for ORDER BY: numerically correct for ints/floats,
    /// lexicographic for strings, and falls back to type tag across types.
    fn order_cmp(&self, other: &Value) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Value::Int(a), Value::Float(b)) => {
                (*a as f64).partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (Value::Float(a), Value::Int(b)) => {
                a.partial_cmp(&(*b as f64)).unwrap_or(Ordering::Equal)
            }
            (Value::String(a), Value::String(b)) => a.cmp(b),
            (Value::Node(a), Value::Node(b)) => a.cmp(b),
            _ => self.sort_type_tag().cmp(&other.sort_type_tag()),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::String(s) => write!(f, "{s}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Null => write!(f, "null"),
            Value::Node(id) => write!(f, "({id})"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
        }
    }
}

/// A result row: column name -> value.
pub type Row = Vec<(String, Value)>;

/// Full query result.
#[derive(Debug)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

// ─── Indexes (built lazily per query) ────────────────────────────────────

#[allow(dead_code)]
struct GraphIndex<'g> {
    /// label -> vec of node references
    label_index: HashMap<NodeLabel, Vec<&'g GraphNode>>,
    /// name -> vec of node references
    name_index: HashMap<String, Vec<&'g GraphNode>>,
    /// source_id -> vec of (rel, target_id)
    outgoing: HashMap<String, Vec<(&'g GraphRelationship, &'g str)>>,
    /// target_id -> vec of (rel, source_id)
    incoming: HashMap<String, Vec<(&'g GraphRelationship, &'g str)>>,
}

impl<'g> GraphIndex<'g> {
    fn build(graph: &'g KnowledgeGraph) -> Self {
        let mut label_index: HashMap<NodeLabel, Vec<&GraphNode>> = HashMap::new();
        let mut name_index: HashMap<String, Vec<&GraphNode>> = HashMap::new();
        let mut outgoing: HashMap<String, Vec<(&GraphRelationship, &str)>> = HashMap::new();
        let mut incoming: HashMap<String, Vec<(&GraphRelationship, &str)>> = HashMap::new();

        for node in graph.iter_nodes() {
            label_index.entry(node.label).or_default().push(node);
            name_index
                .entry(node.properties.name.clone())
                .or_default()
                .push(node);
        }

        for rel in graph.iter_relationships() {
            outgoing
                .entry(rel.source_id.clone())
                .or_default()
                .push((rel, &rel.target_id));
            incoming
                .entry(rel.target_id.clone())
                .or_default()
                .push((rel, &rel.source_id));
        }

        debug!(
            labels = label_index.len(),
            names = name_index.len(),
            "Built query indexes"
        );

        Self {
            label_index,
            name_index,
            outgoing,
            incoming,
        }
    }
}

// ─── Binding environment ─────────────────────────────────────────────────

type Bindings<'g> = HashMap<String, &'g GraphNode>;

// ─── Public API ──────────────────────────────────────────────────────────

/// Execute a parsed CQL statement against a knowledge graph.
pub fn execute(
    statement: &Statement,
    graph: &KnowledgeGraph,
) -> Result<QueryResult, ExecutionError> {
    match statement {
        Statement::Match(m) => execute_match(m, graph),
        Statement::Call(c) => execute_call(c, graph),
    }
}

// ─── CALL execution ──────────────────────────────────────────────────────

fn execute_call(
    call: &CallStatement,
    graph: &KnowledgeGraph,
) -> Result<QueryResult, ExecutionError> {
    match call.function_name.to_uppercase().as_str() {
        "QUERY_FTS_INDEX" => {
            // CALL QUERY_FTS_INDEX('table_name', 'search_query')
            // Simple substring search over node names.
            let query_str = match call.args.get(1) {
                Some(Expr::StringLit(s)) => s.to_lowercase(),
                _ => {
                    return Err(ExecutionError::TypeError(
                        "QUERY_FTS_INDEX expects (table, query) string args".into(),
                    ))
                }
            };

            let mut rows = Vec::new();
            graph.for_each_node(|node| {
                if node.properties.name.to_lowercase().contains(&query_str) {
                    rows.push(vec![
                        ("id".to_string(), Value::String(node.id.clone())),
                        (
                            "name".to_string(),
                            Value::String(node.properties.name.clone()),
                        ),
                        (
                            "label".to_string(),
                            Value::String(node.label.as_str().to_string()),
                        ),
                        (
                            "file_path".to_string(),
                            Value::String(node.properties.file_path.clone()),
                        ),
                    ]);
                }
            });

            Ok(QueryResult {
                columns: vec![
                    "id".into(),
                    "name".into(),
                    "label".into(),
                    "file_path".into(),
                ],
                rows,
            })
        }
        _ => Err(ExecutionError::Unsupported(format!(
            "Unknown function: {}",
            call.function_name
        ))),
    }
}

// ─── MATCH execution ─────────────────────────────────────────────────────

fn execute_match(
    m: &MatchStatement,
    graph: &KnowledgeGraph,
) -> Result<QueryResult, ExecutionError> {
    let index = GraphIndex::build(graph);

    // Step 1: Pattern matching -> produce binding sets
    let mut binding_sets: Vec<Bindings> = vec![HashMap::new()];

    for pattern in &m.patterns {
        binding_sets = match_pattern(pattern, &binding_sets, graph, &index)?;
    }

    // Step 2: Filter by WHERE clause
    if let Some(ref where_expr) = m.where_clause {
        binding_sets
            .retain(|bindings| eval_expr_bool(where_expr, bindings, graph).unwrap_or(false));
    }

    // Step 3: Project RETURN items
    let columns = compute_column_names(&m.return_items);
    let has_aggregation = m.return_items.iter().any(|ri| is_aggregation(&ri.expr));

    let mut rows: Vec<Row> = if has_aggregation {
        // Simple aggregation: aggregate over all binding sets into one row
        let mut row = Vec::new();
        for (i, ri) in m.return_items.iter().enumerate() {
            let val = eval_aggregate(&ri.expr, &binding_sets, graph)?;
            row.push((columns[i].clone(), val));
        }
        vec![row]
    } else {
        let mut result_rows = Vec::new();
        for bindings in &binding_sets {
            let mut row = Vec::new();
            for (i, ri) in m.return_items.iter().enumerate() {
                let val = eval_expr(&ri.expr, bindings, graph)?;
                row.push((columns[i].clone(), val));
            }
            result_rows.push(row);
        }
        result_rows
    };

    // Step 4: ORDER BY
    if !m.order_by.is_empty() {
        rows.sort_by(|a, b| {
            for oi in &m.order_by {
                // Find the column corresponding to this order expression
                let col_name = expr_column_name(&oi.expr);
                let val_a = a.iter().find(|(c, _)| *c == col_name).map(|(_, v)| v);
                let val_b = b.iter().find(|(c, _)| *c == col_name).map(|(_, v)| v);
                let ord = match (val_a, val_b) {
                    (Some(va), Some(vb)) => va.order_cmp(vb),
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                };
                let ord = if oi.descending { ord.reverse() } else { ord };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    // Step 5: LIMIT
    if let Some(limit) = m.limit {
        rows.truncate(limit as usize);
    }

    Ok(QueryResult { columns, rows })
}

// ─── Pattern matching ────────────────────────────────────────────────────

fn match_pattern<'g>(
    pattern: &Pattern,
    current: &[Bindings<'g>],
    graph: &'g KnowledgeGraph,
    index: &GraphIndex<'g>,
) -> Result<Vec<Bindings<'g>>, ExecutionError> {
    let mut results = current.to_vec();

    let mut i = 0;
    while i < pattern.elements.len() {
        match &pattern.elements[i] {
            PatternElement::Node(np) => {
                results = expand_node_pattern(np, &results, graph, index)?;
                i += 1;
            }
            PatternElement::Relationship(rp) => {
                // Relationship must be preceded by a node and followed by a node
                let source_node = if i > 0 {
                    if let PatternElement::Node(np) = &pattern.elements[i - 1] {
                        np
                    } else {
                        return Err(ExecutionError::Unsupported(
                            "Relationship must be between nodes".into(),
                        ));
                    }
                } else {
                    return Err(ExecutionError::Unsupported(
                        "Relationship must be preceded by a node".into(),
                    ));
                };
                let next_node = if i + 1 < pattern.elements.len() {
                    if let PatternElement::Node(np) = &pattern.elements[i + 1] {
                        np
                    } else {
                        return Err(ExecutionError::Unsupported(
                            "Relationship must be between nodes".into(),
                        ));
                    }
                } else {
                    return Err(ExecutionError::Unsupported(
                        "Relationship must be followed by a node".into(),
                    ));
                };

                results = expand_rel_pattern(rp, source_node, next_node, &results, graph, index)?;
                i += 2; // skip rel + node
            }
        }
    }

    Ok(results)
}

fn expand_node_pattern<'g>(
    np: &NodePattern,
    current: &[Bindings<'g>],
    _graph: &'g KnowledgeGraph,
    index: &GraphIndex<'g>,
) -> Result<Vec<Bindings<'g>>, ExecutionError> {
    // If the variable is already bound, just check constraints
    if let Some(ref var) = np.variable {
        let already_bound = current.iter().any(|b| b.contains_key(var.as_str()));
        if already_bound {
            // Filter existing bindings by label/property constraints
            let filtered: Vec<Bindings<'g>> = current
                .iter()
                .filter(|b| {
                    if let Some(node) = b.get(var.as_str()) {
                        node_matches_constraints(node, np)
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();
            return Ok(filtered);
        }
    }

    // Find candidate nodes
    let candidates: Vec<&GraphNode> = if let Some(ref label_str) = np.label {
        if let Some(label) = NodeLabel::from_str_label(label_str) {
            index.label_index.get(&label).cloned().unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        // No label filter -> all nodes (expensive, but correct)
        index
            .label_index
            .values()
            .flat_map(|v| v.iter())
            .copied()
            .collect()
    };

    // Filter by property constraints
    let candidates: Vec<&GraphNode> = candidates
        .into_iter()
        .filter(|node| node_matches_constraints(node, np))
        .collect();

    // Produce new binding sets
    let mut results = Vec::new();
    for bindings in current {
        for &node in &candidates {
            let mut new_bindings = bindings.clone();
            if let Some(ref var) = np.variable {
                new_bindings.insert(var.clone(), node);
            }
            results.push(new_bindings);
        }
    }

    Ok(results)
}

fn expand_rel_pattern<'g>(
    rp: &RelPattern,
    source_np: &NodePattern,
    target_np: &NodePattern,
    current: &[Bindings<'g>],
    graph: &'g KnowledgeGraph,
    index: &GraphIndex<'g>,
) -> Result<Vec<Bindings<'g>>, ExecutionError> {
    let mut results = Vec::new();

    let rel_type_filter: Option<RelationshipType> = rp
        .rel_type
        .as_ref()
        .and_then(|rt| RelationshipType::from_str_type(rt));

    for bindings in current {
        // Source must be the variable bound by the preceding node pattern.
        // Without a named source variable we cannot disambiguate, so skip.
        let source_var = match source_np.variable.as_ref() {
            Some(v) => v,
            None => continue,
        };
        let source_node = match bindings.get(source_var.as_str()) {
            Some(n) => *n,
            None => continue,
        };
        let source_nodes = [(source_var.as_str(), source_node)];

        for (_, source_node) in &source_nodes {
            let neighbors: Vec<(&GraphRelationship, &str)> = match rp.direction {
                Direction::Right => index
                    .outgoing
                    .get(&source_node.id)
                    .cloned()
                    .unwrap_or_default(),
                Direction::Left => index
                    .incoming
                    .get(&source_node.id)
                    .cloned()
                    .unwrap_or_default(),
                Direction::Both => {
                    let mut both = index
                        .outgoing
                        .get(&source_node.id)
                        .cloned()
                        .unwrap_or_default();
                    both.extend(
                        index
                            .incoming
                            .get(&source_node.id)
                            .cloned()
                            .unwrap_or_default(),
                    );
                    both
                }
            };

            for (rel, neighbor_id) in &neighbors {
                // Filter by relationship type
                if let Some(ref rt) = rel_type_filter {
                    if rel.rel_type != *rt {
                        continue;
                    }
                }

                // Get the target node
                let target_node = match graph.get_node(neighbor_id) {
                    Some(n) => n,
                    None => continue,
                };

                // Check target node constraints
                if !node_matches_constraints(target_node, target_np) {
                    continue;
                }

                // Check if target variable already bound
                if let Some(ref target_var) = target_np.variable {
                    if let Some(existing) = bindings.get(target_var.as_str()) {
                        if existing.id != target_node.id {
                            continue;
                        }
                    }
                }

                let mut new_bindings = bindings.clone();
                if let Some(ref target_var) = target_np.variable {
                    new_bindings.insert(target_var.clone(), target_node);
                }
                results.push(new_bindings);
            }
        }
    }

    // Deduplicate: if multiple source nodes could lead to same binding, pick unique ones
    dedup_bindings(&mut results);

    Ok(results)
}

fn dedup_bindings(bindings: &mut Vec<Bindings>) {
    let mut seen = std::collections::HashSet::new();
    bindings.retain(|b| {
        let mut key_parts: Vec<String> = b.iter().map(|(k, v)| format!("{k}={}", v.id)).collect();
        key_parts.sort();
        let key = key_parts.join(",");
        seen.insert(key)
    });
}

fn node_matches_constraints(node: &GraphNode, np: &NodePattern) -> bool {
    // Check label
    if let Some(ref label_str) = np.label {
        if node.label.as_str() != label_str {
            return false;
        }
    }

    // Check property filters
    for (key, val_expr) in &np.properties {
        let node_val = get_node_property(node, key);
        let expected = expr_to_static_value(val_expr);
        if node_val != expected {
            return false;
        }
    }

    true
}

fn get_node_property(node: &GraphNode, key: &str) -> Value {
    match key {
        "name" => Value::String(node.properties.name.clone()),
        "file_path" | "filePath" => Value::String(node.properties.file_path.clone()),
        "start_line" | "startLine" => node
            .properties
            .start_line
            .map(|v| Value::Int(v as i64))
            .unwrap_or(Value::Null),
        "end_line" | "endLine" => node
            .properties
            .end_line
            .map(|v| Value::Int(v as i64))
            .unwrap_or(Value::Null),
        "is_exported" | "isExported" => node
            .properties
            .is_exported
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        "language" => node
            .properties
            .language
            // Use the canonical lowercase string ("cpp", "csharp", "php"...)
            // rather than Debug formatting ("CPlusPlus", "CSharp", "Php"),
            // which would never match queries like `WHERE n.language = 'cpp'`.
            .map(|l| Value::String(l.as_str().to_string()))
            .unwrap_or(Value::Null),
        "description" => node
            .properties
            .description
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
        "heuristic_label" | "heuristicLabel" => node
            .properties
            .heuristic_label
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
        "cohesion" => node
            .properties
            .cohesion
            .map(Value::Float)
            .unwrap_or(Value::Null),
        "symbol_count" | "symbolCount" => node
            .properties
            .symbol_count
            .map(|v| Value::Int(v as i64))
            .unwrap_or(Value::Null),
        "parameter_count" | "parameterCount" => node
            .properties
            .parameter_count
            .map(|v| Value::Int(v as i64))
            .unwrap_or(Value::Null),
        "return_type" | "returnType" => node
            .properties
            .return_type
            .as_ref()
            .map(|s| Value::String(s.clone()))
            .unwrap_or(Value::Null),
        "id" => Value::String(node.id.clone()),
        "label" => Value::String(node.label.as_str().to_string()),
        _ => Value::Null,
    }
}

fn expr_to_static_value(expr: &Expr) -> Value {
    match expr {
        Expr::StringLit(s) => Value::String(s.clone()),
        Expr::IntLit(i) => Value::Int(*i),
        Expr::FloatLit(f) => Value::Float(*f),
        Expr::BoolLit(b) => Value::Bool(*b),
        Expr::Null => Value::Null,
        _ => Value::Null,
    }
}

// ─── Expression evaluation ───────────────────────────────────────────────

fn eval_expr(
    expr: &Expr,
    bindings: &Bindings,
    _graph: &KnowledgeGraph,
) -> Result<Value, ExecutionError> {
    match expr {
        Expr::StringLit(s) => Ok(Value::String(s.clone())),
        Expr::IntLit(i) => Ok(Value::Int(*i)),
        Expr::FloatLit(f) => Ok(Value::Float(*f)),
        Expr::BoolLit(b) => Ok(Value::Bool(*b)),
        Expr::Null => Ok(Value::Null),
        Expr::Variable(name) => {
            if let Some(node) = bindings.get(name.as_str()) {
                Ok(Value::Node(node.id.clone()))
            } else {
                Err(ExecutionError::UnboundVariable(name.clone()))
            }
        }
        Expr::PropertyAccess { variable, property } => {
            if let Some(node) = bindings.get(variable.as_str()) {
                Ok(get_node_property(node, property))
            } else {
                Err(ExecutionError::UnboundVariable(variable.clone()))
            }
        }
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr(left, bindings, _graph)?;
            let r = eval_expr(right, bindings, _graph)?;
            eval_binary_op(&l, *op, &r)
        }
        Expr::Not(inner) => {
            let val = eval_expr(inner, bindings, _graph)?;
            match val {
                Value::Bool(b) => Ok(Value::Bool(!b)),
                _ => Ok(Value::Null),
            }
        }
        Expr::FunctionCall { name, args } => {
            let arg_vals: Vec<Value> = args
                .iter()
                .map(|a| eval_expr(a, bindings, _graph))
                .collect::<Result<Vec<_>, _>>()?;
            eval_function(name, &arg_vals)
        }
        Expr::Aggregation { .. } => {
            // Aggregation in non-aggregate context returns null
            Ok(Value::Null)
        }
        Expr::List(items) => {
            let vals: Vec<Value> = items
                .iter()
                .map(|item| eval_expr(item, bindings, _graph))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(vals))
        }
    }
}

fn eval_expr_bool(
    expr: &Expr,
    bindings: &Bindings,
    graph: &KnowledgeGraph,
) -> Result<bool, ExecutionError> {
    match eval_expr(expr, bindings, graph)? {
        Value::Bool(b) => Ok(b),
        Value::Null => Ok(false),
        _ => Ok(true),
    }
}

fn eval_binary_op(
    left: &Value,
    op: BinaryOperator,
    right: &Value,
) -> Result<Value, ExecutionError> {
    match op {
        BinaryOperator::Eq => Ok(Value::Bool(values_equal(left, right))),
        BinaryOperator::Neq => Ok(Value::Bool(!values_equal(left, right))),
        BinaryOperator::Lt => Ok(Value::Bool(
            values_compare(left, right) == Some(std::cmp::Ordering::Less),
        )),
        BinaryOperator::Gt => Ok(Value::Bool(
            values_compare(left, right) == Some(std::cmp::Ordering::Greater),
        )),
        BinaryOperator::Lte => Ok(Value::Bool(matches!(
            values_compare(left, right),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ))),
        BinaryOperator::Gte => Ok(Value::Bool(matches!(
            values_compare(left, right),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        ))),
        BinaryOperator::And => {
            // Align with eval_expr_bool: Bool->b, Null->false, anything else->true
            let lb = match left {
                Value::Bool(b) => *b,
                Value::Null => false,
                _ => true,
            };
            let rb = match right {
                Value::Bool(b) => *b,
                Value::Null => false,
                _ => true,
            };
            Ok(Value::Bool(lb && rb))
        }
        BinaryOperator::Or => {
            let lb = match left {
                Value::Bool(b) => *b,
                Value::Null => false,
                _ => true,
            };
            let rb = match right {
                Value::Bool(b) => *b,
                Value::Null => false,
                _ => true,
            };
            Ok(Value::Bool(lb || rb))
        }
        BinaryOperator::Contains => match (left, right) {
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a.contains(b.as_str()))),
            _ => Ok(Value::Bool(false)),
        },
        BinaryOperator::StartsWith => match (left, right) {
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a.starts_with(b.as_str()))),
            _ => Ok(Value::Bool(false)),
        },
        BinaryOperator::EndsWith => match (left, right) {
            (Value::String(a), Value::String(b)) => Ok(Value::Bool(a.ends_with(b.as_str()))),
            _ => Ok(Value::Bool(false)),
        },
        BinaryOperator::In => match right {
            Value::List(items) => Ok(Value::Bool(
                items.iter().any(|item| values_equal(left, item)),
            )),
            _ => Ok(Value::Bool(false)),
        },
        BinaryOperator::RegexMatch => match (left, right) {
            (Value::String(text), Value::String(pattern)) => {
                let re = regex::RegexBuilder::new(pattern)
                    .size_limit(1 << 20)
                    .build()
                    .map_err(|e| ExecutionError::TypeError(format!("Invalid regex: {e}")))?;
                Ok(Value::Bool(re.is_match(text)))
            }
            _ => Ok(Value::Bool(false)),
        },
        BinaryOperator::Add => {
            // Use checked arithmetic so a query like
            // `RETURN 9223372036854775807 + 1` (or any computed sum that
            // exceeds i64::MAX) returns Null instead of panicking in debug
            // builds and silently wrapping in release builds. Same applies
            // to mixed Int/Float — promote to Float on overflow so we can
            // still return a sensible (if lossy) numeric value.
            match (left, right) {
                (Value::Int(a), Value::Int(b)) => Ok(a
                    .checked_add(*b)
                    .map(Value::Int)
                    .unwrap_or_else(|| Value::Float(*a as f64 + *b as f64))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Int(a), Value::Float(b)) | (Value::Float(b), Value::Int(a)) => {
                    Ok(Value::Float(*a as f64 + *b))
                }
                (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
                _ => Ok(Value::Null),
            }
        }
        BinaryOperator::Sub => {
            // Same overflow guard as Add — `i64::MIN - 1` would panic in
            // debug otherwise. Promote to Float on overflow.
            match (left, right) {
                (Value::Int(a), Value::Int(b)) => Ok(a
                    .checked_sub(*b)
                    .map(Value::Int)
                    .unwrap_or_else(|| Value::Float(*a as f64 - *b as f64))),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - *b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(*a - *b as f64)),
                _ => Ok(Value::Null),
            }
        }
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < f64::EPSILON,
        // Cross-type numeric coercion: Cypher treats `5 = 5.0` as true. Without
        // this, a query like `WHERE n.parameter_count = 3.0` against an integer
        // property returned false for every row even when the value matched.
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => {
            (*x as f64 - *y).abs() < f64::EPSILON
        }
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Null, Value::Null) => true,
        (Value::Node(x), Value::Node(y)) => x == y,
        _ => false,
    }
}

fn values_compare(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        // Cross-type numeric ordering — same rationale as values_equal: an
        // `ORDER BY` or `WHERE n.size > 1.5` against an int column previously
        // returned `None` (treated as "not greater"), silently dropping rows.
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn eval_function(name: &str, args: &[Value]) -> Result<Value, ExecutionError> {
    match name.to_lowercase().as_str() {
        "size" | "length" => match args.first() {
            Some(Value::List(l)) => Ok(Value::Int(l.len() as i64)),
            Some(Value::String(s)) => Ok(Value::Int(s.len() as i64)),
            _ => Ok(Value::Null),
        },
        "tostring" => match args.first() {
            Some(v) => Ok(Value::String(v.to_string())),
            None => Ok(Value::Null),
        },
        "toupper" => match args.first() {
            Some(Value::String(s)) => Ok(Value::String(s.to_uppercase())),
            _ => Ok(Value::Null),
        },
        "tolower" => match args.first() {
            Some(Value::String(s)) => Ok(Value::String(s.to_lowercase())),
            _ => Ok(Value::Null),
        },
        "coalesce" => {
            for arg in args {
                if !matches!(arg, Value::Null) {
                    return Ok(arg.clone());
                }
            }
            Ok(Value::Null)
        }
        _ => Err(ExecutionError::Unsupported(format!(
            "Unknown function: {name}"
        ))),
    }
}

// ─── Aggregation ─────────────────────────────────────────────────────────

fn is_aggregation(expr: &Expr) -> bool {
    matches!(expr, Expr::Aggregation { .. })
}

fn eval_aggregate(
    expr: &Expr,
    binding_sets: &[Bindings],
    graph: &KnowledgeGraph,
) -> Result<Value, ExecutionError> {
    match expr {
        Expr::Aggregation {
            func,
            distinct,
            expr: agg_expr,
        } => {
            let mut values: Vec<Value> = Vec::new();

            for bindings in binding_sets {
                let val = match agg_expr.as_ref() {
                    AggExpr::Star => Value::Int(1),
                    AggExpr::Expr(inner) => eval_expr(inner, bindings, graph)?,
                };
                values.push(val);
            }

            if *distinct {
                let mut seen = std::collections::HashSet::new();
                values.retain(|v| {
                    let key = format!("{v}");
                    seen.insert(key)
                });
            }

            match func {
                AggFunc::Count => Ok(Value::Int(values.len() as i64)),
                AggFunc::Collect => Ok(Value::List(values)),
                AggFunc::Sum => {
                    let sum: f64 = values
                        .iter()
                        .map(|v| match v {
                            Value::Int(i) => *i as f64,
                            Value::Float(f) => *f,
                            _ => 0.0,
                        })
                        .sum();
                    Ok(Value::Float(sum))
                }
                AggFunc::Avg => {
                    if values.is_empty() {
                        return Ok(Value::Null);
                    }
                    let sum: f64 = values
                        .iter()
                        .map(|v| match v {
                            Value::Int(i) => *i as f64,
                            Value::Float(f) => *f,
                            _ => 0.0,
                        })
                        .sum();
                    Ok(Value::Float(sum / values.len() as f64))
                }
                AggFunc::Min => values
                    .into_iter()
                    .min_by(|a, b| values_compare(a, b).unwrap_or(std::cmp::Ordering::Equal))
                    .map(Ok)
                    .unwrap_or(Ok(Value::Null)),
                AggFunc::Max => values
                    .into_iter()
                    .max_by(|a, b| values_compare(a, b).unwrap_or(std::cmp::Ordering::Equal))
                    .map(Ok)
                    .unwrap_or(Ok(Value::Null)),
            }
        }
        // Non-aggregate expression in aggregate context.
        // This executor has no GROUP BY support, so picking the first binding's
        // value would return arbitrary/misleading data. Return Null instead so
        // the caller can detect that mixed aggregate/scalar projection is unsupported.
        _ => Ok(Value::Null),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────

fn compute_column_names(items: &[ReturnItem]) -> Vec<String> {
    items
        .iter()
        .map(|ri| {
            if let Some(ref alias) = ri.alias {
                alias.clone()
            } else {
                expr_column_name(&ri.expr)
            }
        })
        .collect()
}

fn expr_column_name(expr: &Expr) -> String {
    match expr {
        Expr::Variable(name) => name.clone(),
        Expr::PropertyAccess { variable, property } => format!("{variable}.{property}"),
        Expr::Aggregation { func, .. } => format!("{func:?}").to_lowercase(),
        Expr::FunctionCall { name, .. } => name.clone(),
        _ => "expr".to_string(),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_cql;
    use code_explorer_core::graph::types::*;

    fn make_test_graph() -> KnowledgeGraph {
        let mut graph = KnowledgeGraph::new();

        // Functions
        graph.add_node(GraphNode {
            id: "Function:src/main.ts:handleLogin".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "handleLogin".to_string(),
                file_path: "src/main.ts".to_string(),
                start_line: Some(10),
                end_line: Some(30),
                is_exported: Some(true),
                ..Default::default()
            },
        });
        graph.add_node(GraphNode {
            id: "Function:src/main.ts:validateUser".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "validateUser".to_string(),
                file_path: "src/main.ts".to_string(),
                start_line: Some(35),
                end_line: Some(50),
                is_exported: Some(false),
                ..Default::default()
            },
        });
        graph.add_node(GraphNode {
            id: "Function:src/utils.ts:hash".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "hash".to_string(),
                file_path: "src/utils.ts".to_string(),
                start_line: Some(1),
                end_line: Some(5),
                is_exported: Some(true),
                ..Default::default()
            },
        });

        // Class
        graph.add_node(GraphNode {
            id: "Class:src/models.ts:User".to_string(),
            label: NodeLabel::Class,
            properties: NodeProperties {
                name: "User".to_string(),
                file_path: "src/models.ts".to_string(),
                ..Default::default()
            },
        });

        // Relationships
        graph.add_relationship(GraphRelationship {
            id: "rel1".to_string(),
            source_id: "Function:src/main.ts:handleLogin".to_string(),
            target_id: "Function:src/main.ts:validateUser".to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 1.0,
            reason: "exact".to_string(),
            step: None,
        });
        graph.add_relationship(GraphRelationship {
            id: "rel2".to_string(),
            source_id: "Function:src/main.ts:validateUser".to_string(),
            target_id: "Function:src/utils.ts:hash".to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 0.9,
            reason: "name".to_string(),
            step: None,
        });

        graph
    }

    #[test]
    fn test_execute_simple_match() {
        let graph = make_test_graph();
        let stmt = parse_cql("MATCH (n:Function) RETURN n.name").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.columns, vec!["n.name"]);
        assert_eq!(result.rows.len(), 3);
        let names: Vec<&str> = result
            .rows
            .iter()
            .map(|r| r[0].1.as_str().unwrap())
            .collect();
        assert!(names.contains(&"handleLogin"));
        assert!(names.contains(&"validateUser"));
        assert!(names.contains(&"hash"));
    }

    #[test]
    fn test_execute_where_eq() {
        let graph = make_test_graph();
        let stmt =
            parse_cql("MATCH (n:Function) WHERE n.name = 'handleLogin' RETURN n.name").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0].1, Value::String("handleLogin".into()));
    }

    #[test]
    fn test_execute_where_contains() {
        let graph = make_test_graph();
        let stmt =
            parse_cql("MATCH (n:Function) WHERE n.name CONTAINS 'User' RETURN n.name").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0].1, Value::String("validateUser".into()));
    }

    #[test]
    fn test_execute_relationship() {
        let graph = make_test_graph();
        let stmt =
            parse_cql("MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a.name, b.name").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.columns, vec!["a.name", "b.name"]);
        assert!(result.rows.len() >= 2);
    }

    #[test]
    fn test_execute_count() {
        let graph = make_test_graph();
        let stmt = parse_cql("MATCH (n:Function) RETURN count(n) AS total").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0].1, Value::Int(3));
    }

    #[test]
    fn test_execute_limit() {
        let graph = make_test_graph();
        let stmt = parse_cql("MATCH (n:Function) RETURN n.name LIMIT 2").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn test_execute_order_by() {
        let graph = make_test_graph();
        let stmt = parse_cql("MATCH (n:Function) RETURN n.name ORDER BY n.name ASC").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        let names: Vec<&str> = result
            .rows
            .iter()
            .map(|r| r[0].1.as_str().unwrap())
            .collect();
        // Should be sorted alphabetically
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn test_execute_call_fts() {
        let graph = make_test_graph();
        let stmt = parse_cql("CALL QUERY_FTS_INDEX('symbols', 'handle')").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert!(!result.rows.is_empty());
        let names: Vec<&str> = result
            .rows
            .iter()
            .map(|r| r[1].1.as_str().unwrap())
            .collect();
        assert!(names.contains(&"handleLogin"));
    }

    #[test]
    fn test_execute_class_label() {
        let graph = make_test_graph();
        let stmt = parse_cql("MATCH (n:Class) RETURN n.name").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0].1, Value::String("User".into()));
    }

    #[test]
    fn test_execute_boolean_filter() {
        let graph = make_test_graph();
        let stmt =
            parse_cql("MATCH (n:Function) WHERE n.is_exported = true RETURN n.name").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.rows.len(), 2); // handleLogin and hash
    }

    #[test]
    fn test_execute_or_condition() {
        let graph = make_test_graph();
        let stmt = parse_cql(
            "MATCH (n:Function) WHERE n.name = 'hash' OR n.name = 'handleLogin' RETURN n.name",
        )
        .unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn test_execute_empty_result() {
        let graph = make_test_graph();
        let stmt =
            parse_cql("MATCH (n:Function) WHERE n.name = 'nonexistent' RETURN n.name").unwrap();
        let result = execute(&stmt, &graph).unwrap();

        assert!(result.rows.is_empty());
    }
}
