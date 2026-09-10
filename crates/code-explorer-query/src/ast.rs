//! Strongly-typed AST nodes for the Code Query Language (CQL).

use serde::{Deserialize, Serialize};

/// Top-level query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Statement {
    Match(MatchStatement),
    Call(CallStatement),
}

// ─── CALL ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallStatement {
    pub function_name: String,
    pub args: Vec<Expr>,
}

// ─── MATCH ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchStatement {
    pub patterns: Vec<Pattern>,
    pub where_clause: Option<Expr>,
    pub return_items: Vec<ReturnItem>,
    pub order_by: Vec<OrderItem>,
    pub limit: Option<u64>,
}

// ─── Patterns ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub elements: Vec<PatternElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternElement {
    Node(NodePattern),
    Relationship(RelPattern),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePattern {
    pub variable: Option<String>,
    pub label: Option<String>,
    pub properties: Vec<(String, Expr)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelPattern {
    pub variable: Option<String>,
    pub rel_type: Option<String>,
    pub direction: Direction,
    pub min_hops: Option<u64>,
    pub max_hops: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Right, // -[]->>
    Left,  // <<-[]-
    Both,  // -[]-
}

// ─── Expressions ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// String literal: 'hello' or "hello"
    StringLit(String),
    /// Integer literal
    IntLit(i64),
    /// Float literal
    FloatLit(f64),
    /// Boolean literal
    BoolLit(bool),
    /// null
    Null,
    /// Variable reference: n
    Variable(String),
    /// Property access: n.name
    PropertyAccess { variable: String, property: String },
    /// Binary operation
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
    },
    /// Unary NOT
    Not(Box<Expr>),
    /// Function call: size(n), coalesce(a, b)
    FunctionCall { name: String, args: Vec<Expr> },
    /// Aggregation: count(n), collect(DISTINCT n.name)
    Aggregation {
        func: AggFunc,
        distinct: bool,
        expr: Box<AggExpr>,
    },
    /// List literal: [1, 2, 3]
    List(Vec<Expr>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggExpr {
    Expr(Expr),
    Star,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
    Contains,
    StartsWith,
    EndsWith,
    In,
    RegexMatch,
    Add,
    Sub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggFunc {
    Count,
    Collect,
    Sum,
    Avg,
    Min,
    Max,
}

// ─── Return / Order ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnItem {
    pub expr: Expr,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub expr: Expr,
    pub descending: bool,
}
