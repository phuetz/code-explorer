//! Cypher-subset parser and executor for the in-memory backend.
//!
//! Supports a limited but useful subset of Cypher:
//! - `MATCH (n:Label) RETURN n`
//! - `MATCH (n:Label) WHERE n.name = 'value' RETURN n`
//! - `MATCH (n:Label) WHERE n.name CONTAINS 'text' RETURN n`
//! - `MATCH (n)-[:TYPE]->(m) WHERE n.name = 'foo' RETURN m`
//! - Projected returns: `RETURN n.name, n.filePath`
//! - Aggregation: `RETURN count(n)`
//! - `ORDER BY n.field` / `ORDER BY n.field DESC`
//! - `LIMIT N`
//! - `CALL QUERY_FTS_INDEX('table', 'query')` for full-text search

use std::collections::HashMap;

use code_explorer_core::graph::types::{GraphNode, NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;
use serde_json::Value;

use crate::error::DbError;

// ─── AST Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CypherStatement {
    Match(MatchQuery),
    Call(CallQuery),
}

#[derive(Debug, Clone)]
pub struct MatchQuery {
    pub patterns: Vec<Pattern>,
    pub where_clause: Option<WhereExpr>,
    pub return_clause: ReturnClause,
    pub order_by: Option<(String, String, bool)>, // (var, field, ascending)
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Node {
        var: String,
        label: Option<String>,
    },
    Relationship {
        src_var: String,
        src_label: Option<String>,
        rel_var: Option<String>,
        rel_type: Option<String>,
        dst_var: String,
        dst_label: Option<String>,
        direction: Direction,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

#[derive(Debug, Clone)]
pub enum WhereExpr {
    Eq(String, String, CypherValue),    // var.field = value
    NotEq(String, String, CypherValue), // var.field <> value  OR  var.field != value
    Contains(String, String, String),   // var.field CONTAINS text
    StartsWith(String, String, String), // var.field STARTS WITH text
    EndsWith(String, String, String),   // var.field ENDS WITH text
    And(Box<WhereExpr>, Box<WhereExpr>),
    Or(Box<WhereExpr>, Box<WhereExpr>),
    Not(Box<WhereExpr>), // NOT expr
}

#[derive(Debug, Clone)]
pub enum CypherValue {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct ReturnClause {
    pub items: Vec<ReturnItem>,
    pub distinct: bool,
}

#[derive(Debug, Clone)]
pub enum ReturnItem {
    Var(String),           // n → full node as JSON
    Field(String, String), // n.name → specific property
    Count(String),         // count(n)
}

#[derive(Debug, Clone)]
pub struct CallQuery {
    pub function_name: String,
    pub args: Vec<String>,
}

// ─── Tokens ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Int(i64),
    Float(f64),
    LParen,
    RParen,
    LBracket,
    RBracket,
    Colon,
    Dot,
    Comma,
    Eq,
    Arrow,  // ->
    Dash,   // -
    LAngle, // <
    NotEq,  // <> or !=
    Star,
    Eof,
}

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];

        // Skip whitespace
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        match c {
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '[' => {
                tokens.push(Token::LBracket);
                i += 1;
            }
            ']' => {
                tokens.push(Token::RBracket);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            '.' => {
                tokens.push(Token::Dot);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '=' => {
                tokens.push(Token::Eq);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Star);
                i += 1;
            }
            '<' => {
                if i + 1 < len && chars[i + 1] == '>' {
                    tokens.push(Token::NotEq);
                    i += 2;
                } else {
                    tokens.push(Token::LAngle);
                    i += 1;
                }
            }
            '!' => {
                if i + 1 < len && chars[i + 1] == '=' {
                    tokens.push(Token::NotEq);
                    i += 2;
                } else {
                    i += 1; // skip lone '!'
                }
            }
            '-' => {
                if i + 1 < len && chars[i + 1] == '>' {
                    tokens.push(Token::Arrow);
                    i += 2;
                } else {
                    tokens.push(Token::Dash);
                    i += 1;
                }
            }
            '\'' => {
                // String literal — process escape sequences
                i += 1;
                let mut s = String::new();
                while i < len && chars[i] != '\'' {
                    if chars[i] == '\\' && i + 1 < len {
                        // Skip backslash, take the next character literally
                        i += 1;
                        s.push(chars[i]);
                        i += 1;
                    } else {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
                tokens.push(Token::Str(s));
                if i < len {
                    i += 1; // skip closing quote
                }
            }
            _ if c.is_ascii_digit() => {
                let start = i;
                let mut dot_seen = false;
                while i < len && (chars[i].is_ascii_digit() || (chars[i] == '.' && !dot_seen)) {
                    if chars[i] == '.' {
                        dot_seen = true;
                    }
                    i += 1;
                }
                let num_str: String = chars[start..i].iter().collect();
                if num_str.contains('.') {
                    let f = num_str.parse::<f64>().map_err(|_| {
                        format!("Invalid numeric literal '{num_str}' at position {start}")
                    })?;
                    tokens.push(Token::Float(f));
                } else if let Ok(n) = num_str.parse::<i64>() {
                    tokens.push(Token::Int(n));
                }
            }
            _ if c.is_alphanumeric() || c == '_' => {
                let start = i;
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                tokens.push(Token::Ident(word));
            }
            _ => return Err(format!("Unexpected character '{}' at position {}", c, i)),
        }
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}

// ─── Parser ─────────────────────────────────────────────────────────────

/// Parse a Cypher-like query string into a `CypherStatement`.
pub fn parse(input: &str) -> Result<CypherStatement, DbError> {
    let tokens = tokenize(input).map_err(parse_err)?;
    let mut parser = CypherParser { tokens, pos: 0 };
    parser.parse()
}

/// Recursive-descent parser for the Cypher subset.
struct CypherParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl CypherParser {
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        tok
    }

    fn expect_ident(&mut self) -> Result<String, DbError> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            other => Err(parse_err(format!("Expected identifier, got {:?}", other))),
        }
    }

    fn expect_token(&mut self, expected: &Token) -> Result<(), DbError> {
        let tok = self.advance();
        if &tok == expected {
            Ok(())
        } else {
            Err(parse_err(format!("Expected {:?}, got {:?}", expected, tok)))
        }
    }

    fn check_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Token::Ident(s) if s.eq_ignore_ascii_case(name))
    }

    fn parse(&mut self) -> Result<CypherStatement, DbError> {
        if self.check_ident("MATCH") {
            self.parse_match().map(CypherStatement::Match)
        } else if self.check_ident("CALL") {
            self.parse_call().map(CypherStatement::Call)
        } else {
            Err(parse_err(format!(
                "Expected MATCH or CALL, got {:?}",
                self.peek()
            )))
        }
    }

    fn parse_call(&mut self) -> Result<CallQuery, DbError> {
        self.advance(); // CALL
        let func_name = self.expect_ident()?;
        self.expect_token(&Token::LParen)?;

        let mut args = Vec::new();
        loop {
            match self.peek().clone() {
                Token::RParen => {
                    self.advance();
                    break;
                }
                Token::Str(s) => {
                    args.push(s);
                    self.advance();
                    if matches!(self.peek(), Token::Comma) {
                        self.advance();
                    }
                }
                Token::Ident(s) => {
                    args.push(s);
                    self.advance();
                    if matches!(self.peek(), Token::Comma) {
                        self.advance();
                    }
                }
                _ => {
                    return Err(parse_err(format!(
                        "Unexpected token in CALL args: {:?}",
                        self.peek()
                    )));
                }
            }
        }

        Ok(CallQuery {
            function_name: func_name,
            args,
        })
    }

    fn parse_match(&mut self) -> Result<MatchQuery, DbError> {
        self.advance(); // MATCH

        let patterns = self.parse_patterns()?;

        let where_clause = if self.check_ident("WHERE") {
            self.advance();
            Some(self.parse_where()?)
        } else {
            None
        };

        if !self.check_ident("RETURN") {
            return Err(parse_err("Expected RETURN".to_string()));
        }
        self.advance();
        let return_clause = self.parse_return()?;

        let order_by = if self.check_ident("ORDER") {
            self.advance();
            if !self.check_ident("BY") {
                return Err(parse_err("Expected BY after ORDER".to_string()));
            }
            self.advance();
            let var = self.expect_ident()?;
            self.expect_token(&Token::Dot)?;
            let field = self.expect_ident()?;
            let ascending = if self.check_ident("DESC") {
                self.advance();
                false
            } else {
                if self.check_ident("ASC") {
                    self.advance();
                }
                true
            };
            Some((var, field, ascending))
        } else {
            None
        };

        let limit = if self.check_ident("LIMIT") {
            self.advance();
            match self.advance() {
                Token::Int(n) => {
                    // Reject negative limits explicitly. `n as usize` on a
                    // negative i64 wraps to a huge positive value — the
                    // executor then silently treats `LIMIT -1` as "no limit"
                    // instead of rejecting the malformed query, which can
                    // mask off-by-one bugs in callers constructing queries
                    // dynamically.
                    if n < 0 {
                        return Err(parse_err(format!("LIMIT must be non-negative, got {}", n)));
                    }
                    Some(n as usize)
                }
                other => {
                    return Err(parse_err(format!(
                        "Expected integer after LIMIT, got {:?}",
                        other
                    )))
                }
            }
        } else {
            None
        };

        Ok(MatchQuery {
            patterns,
            where_clause,
            return_clause,
            order_by,
            limit,
        })
    }

    fn parse_patterns(&mut self) -> Result<Vec<Pattern>, DbError> {
        self.expect_token(&Token::LParen)?;
        let (var1, label1) = self.parse_node_inner()?;
        self.expect_token(&Token::RParen)?;

        // Check for relationship: -, <-, or ->
        if matches!(self.peek(), Token::Dash | Token::LAngle | Token::Arrow) {
            let (direction, rel_var, rel_type) = self.parse_rel_and_direction()?;

            self.expect_token(&Token::LParen)?;
            let (var2, label2) = self.parse_node_inner()?;
            self.expect_token(&Token::RParen)?;

            Ok(vec![Pattern::Relationship {
                src_var: var1,
                src_label: label1,
                rel_var,
                rel_type,
                dst_var: var2,
                dst_label: label2,
                direction,
            }])
        } else {
            Ok(vec![Pattern::Node {
                var: var1,
                label: label1,
            }])
        }
    }

    /// Parse a relationship pattern and determine direction.
    ///
    /// Handles:
    /// - `-[:TYPE]->`  => Outgoing (Dash [bracket] Arrow)
    /// - `<-[:TYPE]-`  => Incoming (LAngle Dash [bracket] Dash)
    /// - `-[:TYPE]-`   => Both (Dash [bracket] Dash)
    /// - `-->`         => Outgoing (no bracket)
    fn parse_rel_and_direction(
        &mut self,
    ) -> Result<(Direction, Option<String>, Option<String>), DbError> {
        let starts_incoming = if matches!(self.peek(), Token::LAngle) {
            self.advance(); // <
            true
        } else {
            false
        };

        // Expect Dash or Arrow
        let first = self.advance();
        match &first {
            Token::Dash => {}
            Token::Arrow if !starts_incoming => {
                // Just "->" with no bracket, meaning outgoing with no type
                return Ok((Direction::Outgoing, None, None));
            }
            _ => {
                return Err(parse_err(format!(
                    "Expected - in relationship, got {:?}",
                    first
                )));
            }
        }

        // Optional bracket with rel type
        let (rel_var, rel_type) = if matches!(self.peek(), Token::LBracket) {
            self.advance(); // [
            let mut rv = None;
            let mut rt = None;

            match self.peek().clone() {
                Token::Colon => {
                    self.advance();
                    rt = Some(self.expect_ident()?);
                }
                Token::Ident(_) => {
                    rv = Some(self.expect_ident()?);
                    if matches!(self.peek(), Token::Colon) {
                        self.advance();
                        rt = Some(self.expect_ident()?);
                    }
                }
                _ => {}
            }

            self.expect_token(&Token::RBracket)?;
            (rv, rt)
        } else {
            (None, None)
        };

        // Now determine ending: Dash (both/incoming) or Arrow (outgoing)
        let end = self.advance();
        let direction = match &end {
            Token::Arrow => {
                if starts_incoming {
                    return Err(parse_err(
                        "Cannot have both <- and -> in same relationship".to_string(),
                    ));
                }
                Direction::Outgoing
            }
            Token::Dash => {
                if starts_incoming {
                    Direction::Incoming
                } else {
                    Direction::Both
                }
            }
            _ => {
                return Err(parse_err(format!(
                    "Expected - or -> at end of relationship, got {:?}",
                    end
                )));
            }
        };

        Ok((direction, rel_var, rel_type))
    }

    fn parse_node_inner(&mut self) -> Result<(String, Option<String>), DbError> {
        let var = self.expect_ident()?;
        let label = if matches!(self.peek(), Token::Colon) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        Ok((var, label))
    }

    fn parse_where(&mut self) -> Result<WhereExpr, DbError> {
        let expr = self.parse_where_and()?;
        if self.check_ident("OR") {
            self.advance();
            let right = self.parse_where()?;
            Ok(WhereExpr::Or(Box::new(expr), Box::new(right)))
        } else {
            Ok(expr)
        }
    }

    fn parse_where_and(&mut self) -> Result<WhereExpr, DbError> {
        let negated = if self.check_ident("NOT") {
            self.advance();
            true
        } else {
            false
        };

        let mut expr = self.parse_where_atom()?;

        if negated {
            expr = WhereExpr::Not(Box::new(expr));
        }

        if self.check_ident("AND") {
            self.advance();
            let right = self.parse_where_and()?;
            Ok(WhereExpr::And(Box::new(expr), Box::new(right)))
        } else {
            Ok(expr)
        }
    }

    fn parse_where_atom(&mut self) -> Result<WhereExpr, DbError> {
        let var = self.expect_ident()?;
        self.expect_token(&Token::Dot)?;
        let field = self.expect_ident()?;

        if self.check_ident("CONTAINS") {
            self.advance();
            match self.advance() {
                Token::Str(s) => Ok(WhereExpr::Contains(var, field, s)),
                other => Err(parse_err(format!(
                    "Expected string after CONTAINS, got {:?}",
                    other
                ))),
            }
        } else if self.check_ident("STARTS") {
            self.advance();
            if !self.check_ident("WITH") {
                return Err(parse_err("Expected WITH after STARTS".to_string()));
            }
            self.advance();
            match self.advance() {
                Token::Str(s) => Ok(WhereExpr::StartsWith(var, field, s)),
                other => Err(parse_err(format!(
                    "Expected string after STARTS WITH, got {:?}",
                    other
                ))),
            }
        } else if self.check_ident("ENDS") {
            self.advance();
            if !self.check_ident("WITH") {
                return Err(parse_err("Expected WITH after ENDS".to_string()));
            }
            self.advance();
            match self.advance() {
                Token::Str(s) => Ok(WhereExpr::EndsWith(var, field, s)),
                other => Err(parse_err(format!(
                    "Expected string after ENDS WITH, got {:?}",
                    other
                ))),
            }
        } else {
            let is_not_eq = matches!(self.peek(), Token::NotEq);
            if is_not_eq {
                self.advance();
            } else {
                self.expect_token(&Token::Eq)?;
            }
            let value = match self.advance() {
                Token::Str(s) => CypherValue::Str(s),
                Token::Int(n) => CypherValue::Int(n),
                Token::Float(f) => CypherValue::Float(f),
                Token::Ident(s) if s.eq_ignore_ascii_case("true") => CypherValue::Bool(true),
                Token::Ident(s) if s.eq_ignore_ascii_case("false") => CypherValue::Bool(false),
                other => {
                    return Err(parse_err(format!(
                        "Expected value after operator, got {:?}",
                        other
                    )))
                }
            };
            if is_not_eq {
                Ok(WhereExpr::NotEq(var, field, value))
            } else {
                Ok(WhereExpr::Eq(var, field, value))
            }
        }
    }

    fn parse_return(&mut self) -> Result<ReturnClause, DbError> {
        let distinct = if self.check_ident("DISTINCT") {
            self.advance();
            true
        } else {
            false
        };

        let mut items = Vec::new();
        loop {
            let item = self.parse_return_item()?;
            items.push(item);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(ReturnClause { items, distinct })
    }

    fn parse_return_item(&mut self) -> Result<ReturnItem, DbError> {
        if self.check_ident("count") {
            self.advance();
            self.expect_token(&Token::LParen)?;
            let var = match self.advance() {
                Token::Ident(s) => s,
                Token::Star => "*".to_string(),
                other => {
                    return Err(parse_err(format!(
                        "Expected variable in count(), got {:?}",
                        other
                    )))
                }
            };
            self.expect_token(&Token::RParen)?;
            return Ok(ReturnItem::Count(var));
        }

        let var = self.expect_ident()?;
        if matches!(self.peek(), Token::Dot) {
            self.advance();
            let field = self.expect_ident()?;
            Ok(ReturnItem::Field(var, field))
        } else {
            Ok(ReturnItem::Var(var))
        }
    }
}

fn parse_err(msg: String) -> DbError {
    DbError::QueryError {
        query: String::new(),
        cause: format!("Cypher parse error: {msg}"),
    }
}

// ─── Executor ───────────────────────────────────────────────────────────

/// Indexes built over the graph for efficient query execution.
pub struct GraphIndexes {
    /// node_id -> vec of (target_id, relationship_type)
    pub outgoing: HashMap<String, Vec<(String, RelationshipType)>>,
    /// node_id -> vec of (source_id, relationship_type)
    pub incoming: HashMap<String, Vec<(String, RelationshipType)>>,
    /// label -> vec of node IDs
    pub label_index: HashMap<NodeLabel, Vec<String>>,
}

impl GraphIndexes {
    /// Build indexes from a `KnowledgeGraph`.
    pub fn build(graph: &KnowledgeGraph) -> Self {
        let mut outgoing: HashMap<String, Vec<(String, RelationshipType)>> = HashMap::new();
        let mut incoming: HashMap<String, Vec<(String, RelationshipType)>> = HashMap::new();
        let mut label_index: HashMap<NodeLabel, Vec<String>> = HashMap::new();

        for node in graph.iter_nodes() {
            label_index
                .entry(node.label)
                .or_default()
                .push(node.id.clone());
        }

        for rel in graph.iter_relationships() {
            outgoing
                .entry(rel.source_id.clone())
                .or_default()
                .push((rel.target_id.clone(), rel.rel_type));
            incoming
                .entry(rel.target_id.clone())
                .or_default()
                .push((rel.source_id.clone(), rel.rel_type));
        }

        Self {
            outgoing,
            incoming,
            label_index,
        }
    }
}

/// Execute a parsed Cypher statement against the graph.
pub fn execute(
    stmt: &CypherStatement,
    graph: &KnowledgeGraph,
    indexes: &GraphIndexes,
    fts_index: &super::fts::FtsIndex,
) -> Result<Vec<Value>, DbError> {
    match stmt {
        CypherStatement::Match(mq) => execute_match(mq, graph, indexes),
        CypherStatement::Call(cq) => execute_call(cq, graph, fts_index),
    }
}

fn execute_call(
    cq: &CallQuery,
    graph: &KnowledgeGraph,
    fts_index: &super::fts::FtsIndex,
) -> Result<Vec<Value>, DbError> {
    if cq.function_name.eq_ignore_ascii_case("QUERY_FTS_INDEX") {
        if cq.args.len() < 2 {
            return Err(DbError::QueryError {
                query: format!("CALL {}(...)", cq.function_name),
                cause: "QUERY_FTS_INDEX requires 2 arguments: table_name, query".to_string(),
            });
        }

        let table_name = &cq.args[0];
        let query_text = &cq.args[1];
        let table_filter = super::fts::parse_fts_table_filter(table_name);

        let results = fts_index.search(graph, query_text, table_filter.as_deref(), 100);

        Ok(results.iter().map(super::fts::fts_result_to_json).collect())
    } else {
        Err(DbError::QueryError {
            query: format!("CALL {}", cq.function_name),
            cause: format!("Unknown function: {}", cq.function_name),
        })
    }
}

fn execute_match(
    mq: &MatchQuery,
    graph: &KnowledgeGraph,
    indexes: &GraphIndexes,
) -> Result<Vec<Value>, DbError> {
    // Determine what variables are bound and collect candidate bindings.
    // A "binding" is a map from variable name -> node ID.
    let mut bindings: Vec<HashMap<String, String>> = Vec::new();

    for pattern in &mq.patterns {
        match pattern {
            Pattern::Node { var, label } => {
                let candidates = get_label_candidates(label.as_deref(), indexes, graph);
                for node_id in candidates {
                    let mut b = HashMap::new();
                    b.insert(var.clone(), node_id);
                    bindings.push(b);
                }
            }
            Pattern::Relationship {
                src_var,
                src_label,
                rel_type,
                dst_var,
                dst_label,
                direction,
                ..
            } => {
                // Get source candidates
                let src_candidates = get_label_candidates(src_label.as_deref(), indexes, graph);

                let rel_type_filter = rel_type
                    .as_deref()
                    .and_then(RelationshipType::from_str_type);

                for src_id in &src_candidates {
                    let neighbors = match direction {
                        Direction::Outgoing => indexes.outgoing.get(src_id.as_str()),
                        Direction::Incoming => indexes.incoming.get(src_id.as_str()),
                        Direction::Both => None, // handle below
                    };

                    let mut targets: Vec<String> = Vec::new();

                    if *direction == Direction::Both {
                        // Union of outgoing and incoming
                        if let Some(out) = indexes.outgoing.get(src_id.as_str()) {
                            for (tid, rt) in out {
                                if rel_type_filter.map_or(true, |f| f == *rt) {
                                    targets.push(tid.clone());
                                }
                            }
                        }
                        if let Some(inc) = indexes.incoming.get(src_id.as_str()) {
                            for (tid, rt) in inc {
                                if rel_type_filter.map_or(true, |f| f == *rt) {
                                    targets.push(tid.clone());
                                }
                            }
                        }
                    } else if let Some(edges) = neighbors {
                        for (tid, rt) in edges {
                            if rel_type_filter.map_or(true, |f| f == *rt) {
                                targets.push(tid.clone());
                            }
                        }
                    }

                    // Filter targets by dst_label
                    for tid in targets {
                        if let Some(dl) = dst_label.as_deref() {
                            if let Some(node) = graph.get_node(&tid) {
                                if node.label.as_str() != dl {
                                    continue;
                                }
                            } else {
                                continue;
                            }
                        }
                        let mut b = HashMap::new();
                        b.insert(src_var.clone(), src_id.clone());
                        b.insert(dst_var.clone(), tid);
                        bindings.push(b);
                    }
                }
            }
        }
    }

    // Apply WHERE clause filter
    if let Some(where_expr) = &mq.where_clause {
        bindings.retain(|b| eval_where(where_expr, b, graph));
    }

    // Special case: count aggregation (BEFORE ORDER BY and LIMIT, per SQL semantics)
    // COUNT(*) should count all matching rows, not limited rows.
    if mq.return_clause.items.len() == 1 {
        if let ReturnItem::Count(ref _var) = mq.return_clause.items[0] {
            let total_count = bindings.len();
            let mut map = serde_json::Map::new();
            map.insert("count".to_string(), serde_json::json!(total_count));
            return Ok(vec![Value::Object(map)]);
        }
    }

    // Apply DISTINCT: deduplicate bindings based on projected return values
    if mq.return_clause.distinct {
        let mut seen = std::collections::HashSet::new();
        bindings.retain(|b| {
            let key: Vec<String> = mq
                .return_clause
                .items
                .iter()
                .map(|item| match item {
                    ReturnItem::Var(var) => b.get(var).cloned().unwrap_or_default(),
                    ReturnItem::Field(var, field) => b
                        .get(var)
                        .and_then(|id| get_field_str(graph, id, field))
                        .unwrap_or_default(),
                    ReturnItem::Count(_) => String::new(),
                })
                .collect();
            seen.insert(key)
        });
    }

    // Apply ORDER BY
    if let Some((var, field, ascending)) = &mq.order_by {
        // Compare using a "tagged" ordering so the comparator is a *total
        // order*. The previous version fell through to lexical comparison
        // whenever either value failed to parse as f64 — which, combined
        // with using numeric comparison when both parsed, produced a
        // non-transitive comparator for mixed data. Counter-example that
        // used to return garbage / panic in debug:
        //   a = "9"   (numeric 9)
        //   b = "100" (numeric 100)
        //   c = "1a"  (not numeric)
        //   cmp(a,b) = Less    (both numeric: 9 < 100)
        //   cmp(b,c) = Less    (lexical: '0' < 'a')
        //   cmp(a,c) = Greater (lexical: '9' > '1')
        // Transitivity requires a < c, but we got a > c. Rust's sort_by
        // considers this UB and may panic in newer versions.
        //
        // Fix: put every value into one of two classes (numeric, string)
        // and compare classes first. Within a class the comparison is
        // always homogeneous and therefore transitive. Numeric values
        // sort together numerically; non-numeric values sort together
        // lexicographically; numerics come first so `ORDER BY` on a
        // column of mostly-numbers keeps numeric semantics.
        bindings.sort_by(|a, b| {
            let va = a.get(var).and_then(|id| get_field_str(graph, id, field));
            let vb = b.get(var).and_then(|id| get_field_str(graph, id, field));
            let a_str = va.as_deref().unwrap_or("");
            let b_str = vb.as_deref().unwrap_or("");
            let a_num = a_str.parse::<f64>().ok();
            let b_num = b_str.parse::<f64>().ok();
            let cmp = match (a_num, b_num) {
                (Some(na), Some(nb)) => na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a_str.cmp(b_str),
            };
            if *ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });
    }

    // Apply LIMIT (after count, after order by)
    if let Some(limit) = mq.limit {
        bindings.truncate(limit);
    }

    // Project RETURN clause
    let mut rows: Vec<Value> = Vec::new();

    for binding in &bindings {
        let mut row = serde_json::Map::new();
        for item in &mq.return_clause.items {
            match item {
                ReturnItem::Var(var) => {
                    if let Some(node_id) = binding.get(var) {
                        if let Some(node) = graph.get_node(node_id) {
                            let node_json = node_to_json(node);
                            row.insert(var.clone(), node_json);
                        }
                    }
                }
                ReturnItem::Field(var, field) => {
                    let key = format!("{}.{}", var, field);
                    if let Some(node_id) = binding.get(var) {
                        let val = get_field_value(graph, node_id, field);
                        row.insert(key, val);
                    } else {
                        row.insert(key, Value::Null);
                    }
                }
                ReturnItem::Count(_) => {
                    // Already handled above
                }
            }
        }
        rows.push(Value::Object(row));
    }

    Ok(rows)
}

fn get_label_candidates(
    label: Option<&str>,
    indexes: &GraphIndexes,
    graph: &KnowledgeGraph,
) -> Vec<String> {
    if let Some(label_str) = label {
        if let Some(nl) = NodeLabel::from_str_label(label_str) {
            indexes.label_index.get(&nl).cloned().unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        // No label filter: all nodes
        graph.iter_nodes().map(|n| n.id.clone()).collect()
    }
}

fn eval_where(expr: &WhereExpr, binding: &HashMap<String, String>, graph: &KnowledgeGraph) -> bool {
    match expr {
        WhereExpr::Eq(var, field, value) => {
            if let Some(node_id) = binding.get(var) {
                if let Some(node) = graph.get_node(node_id) {
                    let node_val = get_node_field_str(node, field);
                    match value {
                        CypherValue::Str(s) => node_val.as_deref() == Some(s.as_str()),
                        CypherValue::Int(n) => node_val.as_deref() == Some(&n.to_string()),
                        CypherValue::Float(f) => node_val.as_deref() == Some(&f.to_string()),
                        CypherValue::Bool(b) => node_val.as_deref() == Some(&b.to_string()),
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        WhereExpr::NotEq(var, field, value) => {
            if let Some(node_id) = binding.get(var) {
                if let Some(node) = graph.get_node(node_id) {
                    let node_val = get_node_field_str(node, field);
                    match value {
                        // Treat a missing field as "we cannot evaluate" → false,
                        // matching SQL three-valued logic and the rest of this
                        // executor (Eq/Contains/StartsWith/EndsWith all return
                        // false on missing). Returning true here previously
                        // produced phantom rows for any binding pointing at a
                        // node that had been removed but still had stale edges.
                        CypherValue::Str(s) => match node_val {
                            Some(v) => v != *s,
                            None => false,
                        },
                        CypherValue::Int(n) => match node_val {
                            Some(v) => v != n.to_string(),
                            None => false,
                        },
                        CypherValue::Float(f) => match node_val {
                            Some(v) => v != f.to_string(),
                            None => false,
                        },
                        CypherValue::Bool(b) => match node_val {
                            Some(v) => v != b.to_string(),
                            None => false,
                        },
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        WhereExpr::Contains(var, field, text) => {
            if let Some(node_id) = binding.get(var) {
                if let Some(node) = graph.get_node(node_id) {
                    if let Some(val) = get_node_field_str(node, field) {
                        val.contains(text.as_str())
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        WhereExpr::StartsWith(var, field, text) => {
            if let Some(node_id) = binding.get(var) {
                if let Some(node) = graph.get_node(node_id) {
                    if let Some(val) = get_node_field_str(node, field) {
                        val.starts_with(text.as_str())
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        WhereExpr::EndsWith(var, field, text) => {
            if let Some(node_id) = binding.get(var) {
                if let Some(node) = graph.get_node(node_id) {
                    if let Some(val) = get_node_field_str(node, field) {
                        val.ends_with(text.as_str())
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        }
        WhereExpr::And(left, right) => {
            eval_where(left, binding, graph) && eval_where(right, binding, graph)
        }
        WhereExpr::Or(left, right) => {
            eval_where(left, binding, graph) || eval_where(right, binding, graph)
        }
        WhereExpr::Not(inner) => !eval_where(inner, binding, graph),
    }
}

fn get_node_field_str(node: &GraphNode, field: &str) -> Option<String> {
    // Hot fields are short-circuited so neither WHERE evaluation nor ORDER BY
    // pays the cost of serializing the entire NodeProperties struct (~40
    // optional fields) per row. We also accept both `snake_case` (the Rust
    // field name) and `camelCase` (the serde-renamed JSON form) so queries
    // generated by either side of the codebase resolve identically — the
    // code-explorer-query executor already does this aliasing.
    match field {
        "_label" | "label" => Some(node.label.as_str().to_string()),
        "_id" | "id" => Some(node.id.clone()),
        "name" => Some(node.properties.name.clone()),
        "file_path" | "filePath" => Some(node.properties.file_path.clone()),
        "start_line" | "startLine" => node.properties.start_line.map(|v| v.to_string()),
        "end_line" | "endLine" => node.properties.end_line.map(|v| v.to_string()),
        _ => {
            let val = serde_json::to_value(&node.properties).ok()?;
            // Try the requested key first, then fall back to the camelCase
            // form for snake_case input (and vice versa) so any uncovered
            // field is also accessible by either casing.
            let resolved = val.get(field).or_else(|| {
                if field.contains('_') {
                    let mut camel = String::with_capacity(field.len());
                    let mut upper_next = false;
                    for c in field.chars() {
                        if c == '_' {
                            upper_next = true;
                        } else if upper_next {
                            camel.push(c.to_ascii_uppercase());
                            upper_next = false;
                        } else {
                            camel.push(c);
                        }
                    }
                    val.get(camel)
                } else {
                    None
                }
            })?;
            match resolved {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                Value::Null => None,
                other => Some(other.to_string()),
            }
        }
    }
}

fn get_field_str(graph: &KnowledgeGraph, node_id: &str, field: &str) -> Option<String> {
    graph
        .get_node(node_id)
        .and_then(|n| get_node_field_str(n, field))
}

fn get_field_value(graph: &KnowledgeGraph, node_id: &str, field: &str) -> Value {
    if let Some(node) = graph.get_node(node_id) {
        // Mirror the same hot-field + aliasing logic as get_node_field_str so
        // RETURN expressions and ORDER BY agree on which keys exist.
        match field {
            "_label" | "label" => Value::String(node.label.as_str().to_string()),
            "_id" | "id" => Value::String(node.id.clone()),
            "name" => Value::String(node.properties.name.clone()),
            "file_path" | "filePath" => Value::String(node.properties.file_path.clone()),
            "start_line" | "startLine" => node
                .properties
                .start_line
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
            "end_line" | "endLine" => node
                .properties
                .end_line
                .map(|v| Value::Number(v.into()))
                .unwrap_or(Value::Null),
            _ => {
                let val = serde_json::to_value(&node.properties).unwrap_or(Value::Null);
                if let Some(v) = val.get(field) {
                    v.clone()
                } else if field.contains('_') {
                    let mut camel = String::with_capacity(field.len());
                    let mut upper_next = false;
                    for c in field.chars() {
                        if c == '_' {
                            upper_next = true;
                        } else if upper_next {
                            camel.push(c.to_ascii_uppercase());
                            upper_next = false;
                        } else {
                            camel.push(c);
                        }
                    }
                    val.get(camel).cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            }
        }
    } else {
        Value::Null
    }
}

fn node_to_json(node: &GraphNode) -> Value {
    let mut map = match serde_json::to_value(&node.properties) {
        Ok(Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    map.insert(
        "_label".to_string(),
        Value::String(node.label.as_str().to_string()),
    );
    map.insert("_id".to_string(), Value::String(node.id.clone()));
    Value::Object(map)
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
                start_line: Some(1),
                end_line: Some(100),
                ..Default::default()
            },
        });

        g.add_node(GraphNode {
            id: "Function:src/user.ts:getUser".to_string(),
            label: NodeLabel::Function,
            properties: NodeProperties {
                name: "getUser".to_string(),
                file_path: "src/user.ts".to_string(),
                start_line: Some(50),
                end_line: Some(70),
                ..Default::default()
            },
        });

        // Relationships
        g.add_relationship(GraphRelationship {
            id: "r1".to_string(),
            source_id: "Function:src/auth.ts:handleLogin".to_string(),
            target_id: "Function:src/auth.ts:validateToken".to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 1.0,
            reason: "exact".to_string(),
            step: None,
        });

        g.add_relationship(GraphRelationship {
            id: "r2".to_string(),
            source_id: "Function:src/auth.ts:handleLogin".to_string(),
            target_id: "Function:src/user.ts:getUser".to_string(),
            rel_type: RelationshipType::Calls,
            confidence: 0.9,
            reason: "fuzzy".to_string(),
            step: None,
        });

        g.add_relationship(GraphRelationship {
            id: "r3".to_string(),
            source_id: "Class:src/user.ts:UserService".to_string(),
            target_id: "Function:src/user.ts:getUser".to_string(),
            rel_type: RelationshipType::HasMethod,
            confidence: 1.0,
            reason: "ast".to_string(),
            step: None,
        });

        g
    }

    #[test]
    fn test_parse_simple_match() {
        let stmt = parse("MATCH (n:Function) RETURN n").unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                assert_eq!(mq.patterns.len(), 1);
                match &mq.patterns[0] {
                    Pattern::Node { var, label } => {
                        assert_eq!(var, "n");
                        assert_eq!(label.as_deref(), Some("Function"));
                    }
                    _ => panic!("Expected Node pattern"),
                }
                assert!(mq.where_clause.is_none());
                assert_eq!(mq.return_clause.items.len(), 1);
            }
            _ => panic!("Expected Match statement"),
        }
    }

    #[test]
    fn test_parse_match_with_where() {
        let stmt = parse("MATCH (n:Function) WHERE n.name = 'handleLogin' RETURN n").unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                assert!(mq.where_clause.is_some());
                match mq.where_clause.unwrap() {
                    WhereExpr::Eq(var, field, CypherValue::Str(val)) => {
                        assert_eq!(var, "n");
                        assert_eq!(field, "name");
                        assert_eq!(val, "handleLogin");
                    }
                    _ => panic!("Expected Eq expression"),
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_match_with_contains() {
        let stmt = parse("MATCH (n:Function) WHERE n.name CONTAINS 'handle' RETURN n").unwrap();
        match stmt {
            CypherStatement::Match(mq) => match mq.where_clause.unwrap() {
                WhereExpr::Contains(var, field, text) => {
                    assert_eq!(var, "n");
                    assert_eq!(field, "name");
                    assert_eq!(text, "handle");
                }
                _ => panic!("Expected Contains"),
            },
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_relationship() {
        let stmt =
            parse("MATCH (n)-[:CALLS]->(m) WHERE n.name = 'handleLogin' RETURN m.name").unwrap();
        match stmt {
            CypherStatement::Match(mq) => match &mq.patterns[0] {
                Pattern::Relationship {
                    src_var,
                    rel_type,
                    dst_var,
                    direction,
                    ..
                } => {
                    assert_eq!(src_var, "n");
                    assert_eq!(dst_var, "m");
                    assert_eq!(rel_type.as_deref(), Some("CALLS"));
                    assert_eq!(*direction, Direction::Outgoing);
                }
                _ => panic!("Expected Relationship pattern"),
            },
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_limit() {
        let stmt = parse("MATCH (n:Function) RETURN n LIMIT 5").unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                assert_eq!(mq.limit, Some(5));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_order_by() {
        let stmt = parse("MATCH (n:Function) RETURN n ORDER BY n.name DESC LIMIT 10").unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                let (var, field, ascending) = mq.order_by.unwrap();
                assert_eq!(var, "n");
                assert_eq!(field, "name");
                assert!(!ascending);
                assert_eq!(mq.limit, Some(10));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_count() {
        let stmt = parse("MATCH (n:Function) RETURN count(n)").unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                assert_eq!(mq.return_clause.items.len(), 1);
                match &mq.return_clause.items[0] {
                    ReturnItem::Count(var) => assert_eq!(var, "n"),
                    _ => panic!("Expected Count"),
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_call_fts() {
        let stmt = parse("CALL QUERY_FTS_INDEX('fts_Function', 'auth')").unwrap();
        match stmt {
            CypherStatement::Call(cq) => {
                assert_eq!(cq.function_name, "QUERY_FTS_INDEX");
                assert_eq!(cq.args, vec!["fts_Function", "auth"]);
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_parse_projected_return() {
        let stmt =
            parse("MATCH (n)-[:CALLS]->(m) WHERE n.name = 'handleLogin' RETURN m.name, m.filePath")
                .unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                assert_eq!(mq.return_clause.items.len(), 2);
                match &mq.return_clause.items[0] {
                    ReturnItem::Field(var, field) => {
                        assert_eq!(var, "m");
                        assert_eq!(field, "name");
                    }
                    _ => panic!("Expected Field"),
                }
                match &mq.return_clause.items[1] {
                    ReturnItem::Field(var, field) => {
                        assert_eq!(var, "m");
                        assert_eq!(field, "filePath");
                    }
                    _ => panic!("Expected Field"),
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_execute_label_scan() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt = parse("MATCH (n:Function) RETURN n").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        // Should find 3 Function nodes
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_execute_where_eq() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt = parse("MATCH (n:Function) WHERE n.name = 'handleLogin' RETURN n").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 1);
        let row = &results[0];
        assert_eq!(row["n"]["name"], "handleLogin");
    }

    #[test]
    fn test_execute_where_contains() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt = parse("MATCH (n:Function) WHERE n.name CONTAINS 'handle' RETURN n").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["n"]["name"], "handleLogin");
    }

    #[test]
    fn test_execute_relationship_traversal() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt =
            parse("MATCH (n)-[:CALLS]->(m) WHERE n.name = 'handleLogin' RETURN m.name").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results
            .iter()
            .filter_map(|r| r["m.name"].as_str())
            .collect();
        assert!(names.contains(&"validateToken"));
        assert!(names.contains(&"getUser"));
    }

    #[test]
    fn test_execute_count() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt = parse("MATCH (n:Function) RETURN count(n)").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["count"], 3);
    }

    #[test]
    fn test_execute_limit() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt = parse("MATCH (n:Function) RETURN n LIMIT 2").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_execute_order_by() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt = parse("MATCH (n:Function) RETURN n.name ORDER BY n.name ASC").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        let names: Vec<&str> = results
            .iter()
            .filter_map(|r| r["n.name"].as_str())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn test_execute_fts_call() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::build(&graph);

        let stmt = parse("CALL QUERY_FTS_INDEX('fts_Function', 'handleLogin')").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0]["name"], "handleLogin");
    }

    #[test]
    fn test_parse_and_where() {
        let stmt = parse(
            "MATCH (n:Function) WHERE n.name = 'handleLogin' AND n.filePath = 'src/auth.ts' RETURN n",
        ).unwrap();
        match stmt {
            CypherStatement::Match(mq) => match mq.where_clause.unwrap() {
                WhereExpr::And(left, right) => {
                    match *left {
                        WhereExpr::Eq(ref var, ref field, _) => {
                            assert_eq!(var, "n");
                            assert_eq!(field, "name");
                        }
                        _ => panic!("Expected Eq in left"),
                    }
                    match *right {
                        WhereExpr::Eq(ref var, ref field, _) => {
                            assert_eq!(var, "n");
                            assert_eq!(field, "filePath");
                        }
                        _ => panic!("Expected Eq in right"),
                    }
                }
                _ => panic!("Expected And"),
            },
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_execute_incoming_relationship() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt =
            parse("MATCH (n)<-[:CALLS]-(m) WHERE n.name = 'validateToken' RETURN m.name").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["m.name"], "handleLogin");
    }

    #[test]
    fn test_parse_or_where() {
        let stmt = parse(
            "MATCH (n:Function) WHERE n.name = 'handleLogin' OR n.name = 'validateToken' RETURN n",
        )
        .unwrap();
        match stmt {
            CypherStatement::Match(mq) => match mq.where_clause.unwrap() {
                WhereExpr::Or(left, right) => {
                    match *left {
                        WhereExpr::Eq(ref var, ref field, _) => {
                            assert_eq!(var, "n");
                            assert_eq!(field, "name");
                        }
                        _ => panic!("Expected Eq in left"),
                    }
                    match *right {
                        WhereExpr::Eq(ref var, ref field, _) => {
                            assert_eq!(var, "n");
                            assert_eq!(field, "name");
                        }
                        _ => panic!("Expected Eq in right"),
                    }
                }
                _ => panic!("Expected Or"),
            },
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_execute_or_where() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt = parse(
            "MATCH (n:Function) WHERE n.name = 'handleLogin' OR n.name = 'validateToken' RETURN n.name",
        ).unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results
            .iter()
            .filter_map(|r| r.get("n.name").and_then(|v| v.as_str()))
            .collect();
        assert!(names.contains(&"handleLogin"));
        assert!(names.contains(&"validateToken"));
    }

    #[test]
    fn test_and_has_higher_precedence_than_or() {
        // "a AND b OR c" should parse as "(a AND b) OR c"
        let stmt = parse(
            "MATCH (n:Function) WHERE n.name = 'handleLogin' AND n.filePath = 'src/auth.ts' OR n.name = 'validateToken' RETURN n",
        ).unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                match mq.where_clause.unwrap() {
                    WhereExpr::Or(left, _right) => {
                        // Left should be AND
                        matches!(*left, WhereExpr::And(_, _));
                    }
                    _ => panic!("Expected Or at top level"),
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_not_equal() {
        let stmt = parse("MATCH (n:Function) WHERE n.name <> 'handleLogin' RETURN n").unwrap();
        match stmt {
            CypherStatement::Match(mq) => match mq.where_clause.unwrap() {
                WhereExpr::NotEq(var, field, CypherValue::Str(val)) => {
                    assert_eq!(var, "n");
                    assert_eq!(field, "name");
                    assert_eq!(val, "handleLogin");
                }
                _ => panic!("Expected NotEq"),
            },
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_not_equal_bang() {
        let stmt = parse("MATCH (n:Function) WHERE n.name != 'handleLogin' RETURN n").unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                matches!(mq.where_clause.unwrap(), WhereExpr::NotEq(_, _, _));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_execute_not_equal() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt = parse("MATCH (n:Function) WHERE n.name <> 'handleLogin' RETURN n.name").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        // Should get all functions except handleLogin
        assert!(!results.is_empty());
        for r in &results {
            assert_ne!(r["n.name"], "handleLogin");
        }
    }

    #[test]
    fn test_parse_starts_with() {
        let stmt = parse("MATCH (n:Function) WHERE n.name STARTS WITH 'handle' RETURN n").unwrap();
        match stmt {
            CypherStatement::Match(mq) => match mq.where_clause.unwrap() {
                WhereExpr::StartsWith(var, field, text) => {
                    assert_eq!(var, "n");
                    assert_eq!(field, "name");
                    assert_eq!(text, "handle");
                }
                _ => panic!("Expected StartsWith"),
            },
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_execute_starts_with() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt =
            parse("MATCH (n:Function) WHERE n.name STARTS WITH 'handle' RETURN n.name").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["n.name"], "handleLogin");
    }

    #[test]
    fn test_parse_ends_with() {
        let stmt = parse("MATCH (n:Function) WHERE n.name ENDS WITH 'Token' RETURN n").unwrap();
        match stmt {
            CypherStatement::Match(mq) => match mq.where_clause.unwrap() {
                WhereExpr::EndsWith(var, field, text) => {
                    assert_eq!(var, "n");
                    assert_eq!(field, "name");
                    assert_eq!(text, "Token");
                }
                _ => panic!("Expected EndsWith"),
            },
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_execute_ends_with() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt =
            parse("MATCH (n:Function) WHERE n.name ENDS WITH 'Token' RETURN n.name").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["n.name"], "validateToken");
    }

    #[test]
    fn test_parse_not_where() {
        let stmt = parse("MATCH (n:Function) WHERE NOT n.name = 'handleLogin' RETURN n").unwrap();
        match stmt {
            CypherStatement::Match(mq) => match mq.where_clause.unwrap() {
                WhereExpr::Not(inner) => {
                    matches!(*inner, WhereExpr::Eq(_, _, _));
                }
                _ => panic!("Expected Not"),
            },
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_execute_not_where() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        let stmt =
            parse("MATCH (n:Function) WHERE NOT n.name = 'handleLogin' RETURN n.name").unwrap();
        let results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        assert!(!results.is_empty());
        for r in &results {
            assert_ne!(r["n.name"], "handleLogin");
        }
    }

    #[test]
    fn test_parse_distinct() {
        let stmt = parse("MATCH (n:Function) RETURN DISTINCT n.name").unwrap();
        match stmt {
            CypherStatement::Match(mq) => {
                assert!(mq.return_clause.distinct);
                assert_eq!(mq.return_clause.items.len(), 1);
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_execute_distinct() {
        let graph = make_test_graph();
        let indexes = GraphIndexes::build(&graph);
        let fts = super::super::fts::FtsIndex::new();

        // Without DISTINCT - get all functions
        let stmt = parse("MATCH (n:Function) RETURN n.name").unwrap();
        let all_results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        // With DISTINCT - should deduplicate
        let stmt = parse("MATCH (n:Function) RETURN DISTINCT n.name").unwrap();
        let distinct_results = execute(&stmt, &graph, &indexes, &fts).unwrap();

        // In this test graph, names are already unique, so counts should match
        assert_eq!(all_results.len(), distinct_results.len());
    }
}
