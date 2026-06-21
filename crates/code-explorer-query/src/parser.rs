//! CQL parser: converts a CQL query string into a typed AST.
//!
//! Uses `pest` to parse according to `grammar.pest`, then walks the parse
//! tree to produce [`ast::Statement`] values.

use pest::iterators::Pair;
use pest::Parser;
use thiserror::Error;

use crate::ast::*;
use crate::CqlParser;
use crate::Rule;

// ─── Error type ──────────────────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Syntax error: {0}")]
    Syntax(String),
    #[error("Unexpected rule: {0:?}")]
    UnexpectedRule(String),
    #[error("Invalid integer: {0}")]
    InvalidInt(String),
    #[error("Invalid float: {0}")]
    InvalidFloat(String),
}

impl From<pest::error::Error<Rule>> for ParseError {
    fn from(e: pest::error::Error<Rule>) -> Self {
        ParseError::Syntax(e.to_string())
    }
}

// ─── Public API ──────────────────────────────────────────────────────────

/// Parse a CQL query string into a typed AST [`Statement`].
pub fn parse_cql(input: &str) -> Result<Statement, ParseError> {
    let mut pairs = CqlParser::parse(Rule::query, input)?;
    let query_pair = pairs.next().unwrap(); // query rule
    let statement_pair = query_pair
        .into_inner()
        .find(|p| p.as_rule() == Rule::statement)
        .ok_or_else(|| ParseError::Syntax("Expected a statement".into()))?;
    parse_statement(statement_pair)
}

// ─── Statement ───────────────────────────────────────────────────────────

fn parse_statement(pair: Pair<Rule>) -> Result<Statement, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::match_statement => Ok(Statement::Match(parse_match_statement(inner)?)),
        Rule::call_statement => Ok(Statement::Call(parse_call_statement(inner)?)),
        _ => Err(ParseError::UnexpectedRule(format!("{:?}", inner.as_rule()))),
    }
}

fn parse_call_statement(pair: Pair<Rule>) -> Result<CallStatement, ParseError> {
    let fc = pair
        .into_inner()
        .find(|p| p.as_rule() == Rule::function_call)
        .unwrap();
    parse_function_call_as_call(fc)
}

fn parse_function_call_as_call(pair: Pair<Rule>) -> Result<CallStatement, ParseError> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let args = if let Some(arg_list) = inner.find(|p| p.as_rule() == Rule::arg_list) {
        arg_list
            .into_inner()
            .filter(|p| p.as_rule() == Rule::expression)
            .map(parse_expression)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };
    Ok(CallStatement {
        function_name: name,
        args,
    })
}

fn parse_match_statement(pair: Pair<Rule>) -> Result<MatchStatement, ParseError> {
    let mut patterns = Vec::new();
    let mut where_clause = None;
    let mut return_items = Vec::new();
    let mut order_by = Vec::new();
    let mut limit = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::match_clause => {
                let pattern_list = inner
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::pattern_list)
                    .unwrap();
                patterns = parse_pattern_list(pattern_list)?;
            }
            Rule::where_clause => {
                let expr = inner
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::expression)
                    .unwrap();
                where_clause = Some(parse_expression(expr)?);
            }
            Rule::return_clause => {
                let items = inner
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::return_items)
                    .unwrap();
                return_items = parse_return_items(items)?;
            }
            Rule::order_clause => {
                let items = inner
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::order_items)
                    .unwrap();
                order_by = parse_order_items(items)?;
            }
            Rule::limit_clause => {
                let int_pair = inner
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::integer)
                    .unwrap();
                limit = Some(
                    int_pair
                        .as_str()
                        .parse::<u64>()
                        .map_err(|_| ParseError::InvalidInt(int_pair.as_str().into()))?,
                );
            }
            _ => {}
        }
    }

    Ok(MatchStatement {
        patterns,
        where_clause,
        return_items,
        order_by,
        limit,
    })
}

// ─── Patterns ────────────────────────────────────────────────────────────

fn parse_pattern_list(pair: Pair<Rule>) -> Result<Vec<Pattern>, ParseError> {
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::pattern)
        .map(parse_pattern)
        .collect()
}

fn parse_pattern(pair: Pair<Rule>) -> Result<Pattern, ParseError> {
    let mut elements = Vec::new();
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::node_pattern => elements.push(PatternElement::Node(parse_node_pattern(inner)?)),
            Rule::rel_pattern => {
                elements.push(PatternElement::Relationship(parse_rel_pattern(inner)?))
            }
            _ => {}
        }
    }
    Ok(Pattern { elements })
}

fn parse_node_pattern(pair: Pair<Rule>) -> Result<NodePattern, ParseError> {
    let mut variable = None;
    let mut label = None;
    let mut properties = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::variable => {
                variable = Some(inner.as_str().to_string());
            }
            Rule::label_filter => {
                let ident = inner
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::identifier)
                    .unwrap();
                label = Some(ident.as_str().to_string());
            }
            Rule::prop_filter => {
                for prop_pair in inner
                    .into_inner()
                    .filter(|p| p.as_rule() == Rule::prop_pair)
                {
                    let mut pp_inner = prop_pair.into_inner();
                    let key = pp_inner.next().unwrap().as_str().to_string();
                    let val = parse_value(pp_inner.next().unwrap())?;
                    properties.push((key, val));
                }
            }
            _ => {}
        }
    }

    Ok(NodePattern {
        variable,
        label,
        properties,
    })
}

fn parse_rel_pattern(pair: Pair<Rule>) -> Result<RelPattern, ParseError> {
    let raw = pair.as_str();
    let direction = if raw.starts_with("<-") {
        Direction::Left
    } else if raw.ends_with("->") {
        Direction::Right
    } else {
        Direction::Both
    };

    let mut variable = None;
    let mut rel_type = None;
    let mut min_hops = None;
    let mut max_hops = None;

    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::rel_inner {
            for ri in inner.into_inner() {
                match ri.as_rule() {
                    Rule::variable => {
                        variable = Some(ri.as_str().to_string());
                    }
                    Rule::rel_type => {
                        let ident = ri
                            .into_inner()
                            .find(|p| p.as_rule() == Rule::identifier)
                            .unwrap();
                        rel_type = Some(ident.as_str().to_string());
                    }
                    Rule::length_spec => {
                        let ints: Vec<_> = ri
                            .into_inner()
                            .filter(|p| p.as_rule() == Rule::integer)
                            .collect();
                        if ints.len() == 2 {
                            min_hops =
                                Some(ints[0].as_str().parse().map_err(|_| {
                                    ParseError::InvalidInt(ints[0].as_str().into())
                                })?);
                            max_hops =
                                Some(ints[1].as_str().parse().map_err(|_| {
                                    ParseError::InvalidInt(ints[1].as_str().into())
                                })?);
                        }
                        // bare `*` means unbounded (0..inf), leave both None
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(RelPattern {
        variable,
        rel_type,
        direction,
        min_hops,
        max_hops,
    })
}

// ─── Expressions ─────────────────────────────────────────────────────────

fn parse_expression(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::or_expr => parse_or_expr(inner),
        _ => Err(ParseError::UnexpectedRule(format!("{:?}", inner.as_rule()))),
    }
}

fn parse_or_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut parts: Vec<Pair<Rule>> = pair
        .into_inner()
        .filter(|p| p.as_rule() == Rule::and_expr)
        .collect();

    if parts.len() == 1 {
        return parse_and_expr(parts.remove(0));
    }

    let mut left = parse_and_expr(parts.remove(0))?;
    for part in parts {
        let right = parse_and_expr(part)?;
        left = Expr::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Or,
            right: Box::new(right),
        };
    }
    Ok(left)
}

fn parse_and_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut parts: Vec<Pair<Rule>> = pair
        .into_inner()
        .filter(|p| p.as_rule() == Rule::not_expr)
        .collect();

    if parts.len() == 1 {
        return parse_not_expr(parts.remove(0));
    }

    let mut left = parse_not_expr(parts.remove(0))?;
    for part in parts {
        let right = parse_not_expr(part)?;
        left = Expr::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::And,
            right: Box::new(right),
        };
    }
    Ok(left)
}

fn parse_not_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut has_not = false;
    let mut comparison_pair = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::not_op => has_not = true,
            Rule::comparison => comparison_pair = Some(inner),
            _ => {}
        }
    }

    let expr = parse_comparison(comparison_pair.unwrap())?;
    if has_not {
        Ok(Expr::Not(Box::new(expr)))
    } else {
        Ok(expr)
    }
}

fn parse_comparison(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut children: Vec<Pair<Rule>> = pair.into_inner().collect();

    if children.len() == 1 {
        return parse_add_expr(children.remove(0));
    }

    // children: add_expr, comp_op, add_expr
    let left = parse_add_expr(children.remove(0))?;
    let op_pair = children.remove(0);
    let right = parse_add_expr(children.remove(0))?;
    let op = parse_comp_op(op_pair)?;

    Ok(Expr::BinaryOp {
        left: Box::new(left),
        op,
        right: Box::new(right),
    })
}

fn parse_comp_op(pair: Pair<Rule>) -> Result<BinaryOperator, ParseError> {
    let raw = pair.as_str().trim();
    // Check for multi-word ops first
    let lower = raw.to_uppercase();
    if lower.contains("STARTS") && lower.contains("WITH") {
        return Ok(BinaryOperator::StartsWith);
    }
    if lower.contains("ENDS") && lower.contains("WITH") {
        return Ok(BinaryOperator::EndsWith);
    }
    match raw {
        "=" => Ok(BinaryOperator::Eq),
        "<>" | "!=" => Ok(BinaryOperator::Neq),
        ">=" => Ok(BinaryOperator::Gte),
        "<=" => Ok(BinaryOperator::Lte),
        ">" => Ok(BinaryOperator::Gt),
        "<" => Ok(BinaryOperator::Lt),
        "=~" => Ok(BinaryOperator::RegexMatch),
        _ if raw.eq_ignore_ascii_case("CONTAINS") => Ok(BinaryOperator::Contains),
        _ if raw.eq_ignore_ascii_case("IN") => Ok(BinaryOperator::In),
        _ => Err(ParseError::UnexpectedRule(format!("comp_op: {raw}"))),
    }
}

fn parse_add_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut children: Vec<Pair<Rule>> = pair.into_inner().collect();

    if children.len() == 1 {
        return parse_atom(children.remove(0));
    }

    // atom (add_op atom)*
    let mut left = parse_atom(children.remove(0))?;
    while !children.is_empty() {
        let op_pair = children.remove(0);
        let right = parse_atom(children.remove(0))?;
        let op = match op_pair.as_str() {
            "+" => BinaryOperator::Add,
            "-" => BinaryOperator::Sub,
            _ => {
                return Err(ParseError::UnexpectedRule(format!(
                    "add_op: {}",
                    op_pair.as_str()
                )))
            }
        };
        left = Expr::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        };
    }
    Ok(left)
}

fn parse_atom(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::aggregation => parse_aggregation(inner),
        Rule::func_call_expr => parse_func_call_expr(inner),
        Rule::float => {
            let v = inner
                .as_str()
                .parse::<f64>()
                .map_err(|_| ParseError::InvalidFloat(inner.as_str().into()))?;
            Ok(Expr::FloatLit(v))
        }
        Rule::integer => {
            let v = inner
                .as_str()
                .parse::<i64>()
                .map_err(|_| ParseError::InvalidInt(inner.as_str().into()))?;
            Ok(Expr::IntLit(v))
        }
        Rule::boolean => {
            let v = inner.as_str().eq_ignore_ascii_case("true");
            Ok(Expr::BoolLit(v))
        }
        Rule::null => Ok(Expr::Null),
        Rule::string_literal => Ok(Expr::StringLit(parse_string_literal(inner))),
        Rule::property_access => {
            let mut parts = inner.into_inner();
            let var = parts.next().unwrap().as_str().to_string();
            let prop = parts.next().unwrap().as_str().to_string();
            Ok(Expr::PropertyAccess {
                variable: var,
                property: prop,
            })
        }
        Rule::list_literal => {
            let items = inner
                .into_inner()
                .filter(|p| p.as_rule() == Rule::expression)
                .map(parse_expression)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr::List(items))
        }
        Rule::paren_expr => {
            let expr = inner
                .into_inner()
                .find(|p| p.as_rule() == Rule::expression)
                .unwrap();
            parse_expression(expr)
        }
        Rule::variable => Ok(Expr::Variable(inner.as_str().to_string())),
        _ => Err(ParseError::UnexpectedRule(format!("{:?}", inner.as_rule()))),
    }
}

fn parse_value(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    // prop_pair child is now an atom rule
    match pair.as_rule() {
        Rule::atom => parse_atom(pair),
        Rule::string_literal => Ok(Expr::StringLit(parse_string_literal(pair))),
        Rule::integer => {
            let v = pair
                .as_str()
                .parse::<i64>()
                .map_err(|_| ParseError::InvalidInt(pair.as_str().into()))?;
            Ok(Expr::IntLit(v))
        }
        Rule::float => {
            let v = pair
                .as_str()
                .parse::<f64>()
                .map_err(|_| ParseError::InvalidFloat(pair.as_str().into()))?;
            Ok(Expr::FloatLit(v))
        }
        Rule::boolean => {
            let v = pair.as_str().eq_ignore_ascii_case("true");
            Ok(Expr::BoolLit(v))
        }
        Rule::null => Ok(Expr::Null),
        Rule::property_access => {
            let mut parts = pair.into_inner();
            let var = parts.next().unwrap().as_str().to_string();
            let prop = parts.next().unwrap().as_str().to_string();
            Ok(Expr::PropertyAccess {
                variable: var,
                property: prop,
            })
        }
        Rule::variable => Ok(Expr::Variable(pair.as_str().to_string())),
        _ => Err(ParseError::UnexpectedRule(format!("{:?}", pair.as_rule()))),
    }
}

fn parse_string_literal(pair: Pair<Rule>) -> String {
    // The inner content is between quotes.
    // inner_single or inner_double is the child.
    pair.into_inner()
        .next()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default()
}

fn parse_aggregation(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut func = AggFunc::Count;
    let mut distinct = false;
    let mut expr = AggExpr::Star;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::agg_func => {
                func = match inner.as_str().to_lowercase().as_str() {
                    "count" => AggFunc::Count,
                    "collect" => AggFunc::Collect,
                    "sum" => AggFunc::Sum,
                    "avg" => AggFunc::Avg,
                    "min" => AggFunc::Min,
                    "max" => AggFunc::Max,
                    _ => {
                        return Err(ParseError::UnexpectedRule(format!(
                            "agg_func: {}",
                            inner.as_str()
                        )))
                    }
                };
            }
            Rule::distinct => {
                distinct = true;
            }
            Rule::star => {
                expr = AggExpr::Star;
            }
            Rule::expression => {
                expr = AggExpr::Expr(parse_expression(inner)?);
            }
            _ => {}
        }
    }

    Ok(Expr::Aggregation {
        func,
        distinct,
        expr: Box::new(expr),
    })
}

fn parse_func_call_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let args = if let Some(arg_list) = inner.find(|p| p.as_rule() == Rule::arg_list) {
        arg_list
            .into_inner()
            .filter(|p| p.as_rule() == Rule::expression)
            .map(parse_expression)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };
    Ok(Expr::FunctionCall { name, args })
}

// ─── Return / Order ──────────────────────────────────────────────────────

fn parse_return_items(pair: Pair<Rule>) -> Result<Vec<ReturnItem>, ParseError> {
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::return_item)
        .map(parse_return_item)
        .collect()
}

fn parse_return_item(pair: Pair<Rule>) -> Result<ReturnItem, ParseError> {
    let mut expr_pair = None;
    let mut alias = None;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::expression => expr_pair = Some(inner),
            Rule::alias => {
                let ident = inner
                    .into_inner()
                    .find(|p| p.as_rule() == Rule::identifier)
                    .unwrap();
                alias = Some(ident.as_str().to_string());
            }
            _ => {}
        }
    }

    Ok(ReturnItem {
        expr: parse_expression(expr_pair.unwrap())?,
        alias,
    })
}

fn parse_order_items(pair: Pair<Rule>) -> Result<Vec<OrderItem>, ParseError> {
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::order_item)
        .map(parse_order_item)
        .collect()
}

fn parse_order_item(pair: Pair<Rule>) -> Result<OrderItem, ParseError> {
    let mut expr_pair = None;
    let mut descending = false;

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::expression => expr_pair = Some(inner),
            Rule::order_dir => {
                descending = inner.as_str().eq_ignore_ascii_case("DESC");
            }
            _ => {}
        }
    }

    Ok(OrderItem {
        expr: parse_expression(expr_pair.unwrap())?,
        descending,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_match() {
        let q = "MATCH (n:Function) RETURN n";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                assert_eq!(m.patterns.len(), 1);
                assert!(m.where_clause.is_none());
                assert_eq!(m.return_items.len(), 1);
                assert!(m.limit.is_none());
            }
            _ => panic!("Expected Match statement"),
        }
    }

    #[test]
    fn test_parse_match_with_where() {
        let q = "MATCH (n:Function) WHERE n.name = 'handleLogin' RETURN n";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                assert!(m.where_clause.is_some());
                let pattern = &m.patterns[0];
                if let PatternElement::Node(np) = &pattern.elements[0] {
                    assert_eq!(np.variable.as_deref(), Some("n"));
                    assert_eq!(np.label.as_deref(), Some("Function"));
                } else {
                    panic!("Expected node pattern");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_relationship_pattern() {
        let q = "MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a.name, b.name";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                let pattern = &m.patterns[0];
                assert_eq!(pattern.elements.len(), 3); // node, rel, node
                if let PatternElement::Relationship(rp) = &pattern.elements[1] {
                    assert_eq!(rp.rel_type.as_deref(), Some("CALLS"));
                    assert_eq!(rp.direction, Direction::Right);
                } else {
                    panic!("Expected rel pattern");
                }
                assert_eq!(m.return_items.len(), 2);
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_call_statement() {
        let q = "CALL QUERY_FTS_INDEX('symbols', 'search term')";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Call(c) => {
                assert_eq!(c.function_name, "QUERY_FTS_INDEX");
                assert_eq!(c.args.len(), 2);
                if let Expr::StringLit(s) = &c.args[0] {
                    assert_eq!(s, "symbols");
                } else {
                    panic!("Expected string arg");
                }
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_parse_with_limit() {
        let q = "MATCH (n:Function) RETURN n.name LIMIT 10";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                assert_eq!(m.limit, Some(10));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_with_order() {
        let q = "MATCH (n:Function) RETURN n.name ORDER BY n.name DESC LIMIT 5";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                assert_eq!(m.order_by.len(), 1);
                assert!(m.order_by[0].descending);
                assert_eq!(m.limit, Some(5));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_and_or() {
        let q = "MATCH (n:Function) WHERE n.name = 'foo' OR n.name = 'bar' RETURN n";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let Some(Expr::BinaryOp { op, .. }) = &m.where_clause {
                    assert_eq!(*op, BinaryOperator::Or);
                } else {
                    panic!("Expected OR expression");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_not() {
        let q = "MATCH (n:Function) WHERE NOT n.name = 'foo' RETURN n";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                assert!(matches!(m.where_clause, Some(Expr::Not(_))));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_contains() {
        let q = "MATCH (n:Function) WHERE n.name CONTAINS 'handle' RETURN n";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let Some(Expr::BinaryOp { op, .. }) = &m.where_clause {
                    assert_eq!(*op, BinaryOperator::Contains);
                } else {
                    panic!("Expected CONTAINS");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_aggregation() {
        let q = "MATCH (n:Function) RETURN count(n)";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let Expr::Aggregation { func, .. } = &m.return_items[0].expr {
                    assert_eq!(*func, AggFunc::Count);
                } else {
                    panic!("Expected aggregation");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_alias() {
        let q = "MATCH (n:Function) RETURN n.name AS fn_name";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                assert_eq!(m.return_items[0].alias.as_deref(), Some("fn_name"));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_left_arrow_rel() {
        let q = "MATCH (a:Function)<-[:CALLS]-(b:Function) RETURN a, b";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let PatternElement::Relationship(rp) = &m.patterns[0].elements[1] {
                    assert_eq!(rp.direction, Direction::Left);
                    assert_eq!(rp.rel_type.as_deref(), Some("CALLS"));
                } else {
                    panic!("Expected rel");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_node_with_properties() {
        let q = "MATCH (n:Function {name: 'foo'}) RETURN n";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let PatternElement::Node(np) = &m.patterns[0].elements[0] {
                    assert_eq!(np.properties.len(), 1);
                    assert_eq!(np.properties[0].0, "name");
                    if let Expr::StringLit(s) = &np.properties[0].1 {
                        assert_eq!(s, "foo");
                    } else {
                        panic!("Expected string value");
                    }
                } else {
                    panic!("Expected node");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_undirected_rel() {
        let q = "MATCH (a:Function)-[:CALLS]-(b:Function) RETURN a, b";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let PatternElement::Relationship(rp) = &m.patterns[0].elements[1] {
                    assert_eq!(rp.direction, Direction::Both);
                } else {
                    panic!("Expected rel");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_variable_length_rel() {
        let q = "MATCH (a:Function)-[:CALLS*1..3]->(b:Function) RETURN a, b";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let PatternElement::Relationship(rp) = &m.patterns[0].elements[1] {
                    assert_eq!(rp.min_hops, Some(1));
                    assert_eq!(rp.max_hops, Some(3));
                } else {
                    panic!("Expected rel");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_multiple_return_items() {
        let q = "MATCH (n:Function) RETURN n.name, n.file_path, count(n) AS total";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                assert_eq!(m.return_items.len(), 3);
                assert_eq!(m.return_items[2].alias.as_deref(), Some("total"));
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_syntax_error() {
        let q = "MATC (n) RETUR n";
        assert!(parse_cql(q).is_err());
    }

    #[test]
    fn test_parse_boolean_literal() {
        let q = "MATCH (n:Function) WHERE n.is_exported = true RETURN n";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let Some(Expr::BinaryOp { right, .. }) = &m.where_clause {
                    assert!(matches!(**right, Expr::BoolLit(true)));
                } else {
                    panic!("Expected binary op with bool");
                }
            }
            _ => panic!("Expected Match"),
        }
    }

    #[test]
    fn test_parse_in_operator() {
        let q = "MATCH (n:Function) WHERE n.name IN ['foo', 'bar'] RETURN n";
        let stmt = parse_cql(q).unwrap();
        match stmt {
            Statement::Match(m) => {
                if let Some(Expr::BinaryOp { op, .. }) = &m.where_clause {
                    assert_eq!(*op, BinaryOperator::In);
                } else {
                    panic!("Expected IN operator");
                }
            }
            _ => panic!("Expected Match"),
        }
    }
}
