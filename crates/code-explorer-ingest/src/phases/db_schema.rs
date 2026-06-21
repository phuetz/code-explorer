//! Phase: Database schema extraction (Theme D).
//!
//! Scans SQL migrations, Prisma schemas, and ORM classes to produce
//! `DbEntity` + `DbColumn` nodes with `HasColumn` and `ReferencesTable`
//! relationships. ASP.NET MVC's EF6 path is owned by `aspnet_mvc.rs`, so we
//! do NOT touch `.cs` or `.edmx` files here.
//!
//! ## Sources supported (MVP)
//!
//! - SQL migrations: `**/migrations/**.sql`, `**/*.schema.sql`, any `*.sql`
//!   containing `CREATE TABLE` statements.
//! - Prisma: `**/prisma/schema.prisma` models.
//! - SQLAlchemy (Python): `class X(Base):` classes linked to the nearest
//!   `__tablename__` or class name.
//! - TypeORM (TS/JS): `@Entity(...)` decorators.
//!
//! ## TODO(theme-d)
//!
//! - Rails schema.rb parsing.
//! - Sequelize `sequelize.define(...)`.
//! - Hibernate XML mappings.
//! - Alembic op.create_table.
//! - Django models.Model.

use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashMap;

use crate::phases::structure::FileEntry;

#[derive(Debug, Default, Clone, Copy)]
pub struct DbSchemaStats {
    pub tables: usize,
    pub columns: usize,
    pub foreign_keys: usize,
    pub orm_mappings: usize,
}

/// One parsed table with its columns and FKs, agnostic of source.
#[derive(Debug, Clone)]
struct ParsedTable {
    name: String,
    file_path: String,
    start_line: u32,
    columns: Vec<ParsedColumn>,
    /// Pairs of (local_column, target_table)
    foreign_keys: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct ParsedColumn {
    name: String,
    column_type: String,
    is_primary_key: bool,
    is_nullable: bool,
}

/// ORM class mapping: a code-level Class node name + file path → table name.
#[derive(Debug, Clone)]
struct OrmMapping {
    class_name: String,
    file_path: String,
    table_name: String,
}

// ─── Regexes ────────────────────────────────────────────────────────────────

static RE_CREATE_TABLE_HEAD: Lazy<Regex> = Lazy::new(|| {
    // Match the head of a CREATE TABLE statement up to the opening `(`.
    // We use this regex only to locate tables; the body is extracted by a
    // manual paren-matcher that handles nested `(` / `)` (decimal sizes,
    // CHECK constraints, etc.) which regex-only parsing fumbles.
    Regex::new(r#"(?is)CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?[`"\[]?(\w+)[`"\]]?\s*\("#)
        .expect("create table head regex compiles")
});

static RE_FOREIGN_KEY_FULL: Lazy<Regex> = Lazy::new(|| {
    // FOREIGN KEY (col) REFERENCES target_table[(col)]
    Regex::new(r#"(?is)FOREIGN\s+KEY\s*\(\s*(\w+)\s*\)\s*REFERENCES\s+(\w+)"#)
        .expect("fk full regex compiles")
});

static RE_INLINE_REFERENCES: Lazy<Regex> = Lazy::new(|| {
    // Inline form: `col INT REFERENCES target(col)`. We only capture the
    // target table name; the originating column comes from the column line.
    Regex::new(r#"(?is)REFERENCES\s+(\w+)"#).expect("inline ref regex compiles")
});

static RE_PRISMA_MODEL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?ms)^\s*model\s+(\w+)\s*\{([^}]*)\}"#).expect("prisma model regex compiles")
});

static RE_TYPEORM_ENTITY: Lazy<Regex> = Lazy::new(|| {
    // @Entity(...) followed by a later `export class Foo`.
    Regex::new(r#"@Entity\s*\(\s*(?:['"]([^'"]+)['"])?\s*[^)]*\)"#)
        .expect("typeorm entity regex compiles")
});

static RE_TS_CLASS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"export\s+(?:abstract\s+)?class\s+(\w+)"#).expect("ts class regex compiles")
});

static RE_SQLALCHEMY_CLASS: Lazy<Regex> = Lazy::new(|| {
    // class Foo(Base):  or  class Foo(db.Model):
    Regex::new(r#"(?m)^\s*class\s+(\w+)\s*\(\s*(?:Base|db\.Model|declarative_base\(\))"#)
        .expect("sqlalchemy class regex compiles")
});

static RE_SQLALCHEMY_TABLENAME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"__tablename__\s*=\s*['"]([^'"]+)['"]"#)
        .expect("sqlalchemy tablename regex compiles")
});

// ─── Entry point ────────────────────────────────────────────────────────────

pub fn extract_db_schema(graph: &mut KnowledgeGraph, files: &[FileEntry]) -> DbSchemaStats {
    let per_file: Vec<(Vec<ParsedTable>, Vec<OrmMapping>)> = files
        .par_iter()
        .filter(|f| !should_skip(&f.path))
        .map(scan_file)
        .collect();

    let mut stats = DbSchemaStats::default();
    let mut tables: HashMap<String, ParsedTable> = HashMap::new();
    let mut mappings: Vec<OrmMapping> = Vec::new();

    for (t, m) in per_file {
        for table in t {
            // Later definitions replace earlier ones deterministically. In
            // practice a migration repo may contain `CREATE TABLE` then
            // `ALTER` — we only capture the former here. TODO(theme-d):
            // handle ALTER TABLE ADD COLUMN to refine the schema.
            tables.insert(table.name.clone(), table);
        }
        mappings.extend(m);
    }

    // Emit DbEntity + DbColumn nodes.
    let mut fk_pairs: Vec<(String, String)> = Vec::new();
    for table in tables.values() {
        let entity_id = generate_id("DbEntity", &table.name);
        graph.add_node(GraphNode {
            id: entity_id.clone(),
            label: NodeLabel::DbEntity,
            properties: NodeProperties {
                name: table.name.clone(),
                file_path: table.file_path.clone(),
                start_line: Some(table.start_line),
                db_table_name: Some(table.name.clone()),
                ..Default::default()
            },
        });
        stats.tables += 1;

        for col in &table.columns {
            let col_id = generate_id("DbColumn", &format!("{}.{}", table.name, col.name));
            graph.add_node(GraphNode {
                id: col_id.clone(),
                label: NodeLabel::DbColumn,
                properties: NodeProperties {
                    name: col.name.clone(),
                    file_path: table.file_path.clone(),
                    column_type: Some(col.column_type.clone()),
                    is_primary_key: Some(col.is_primary_key),
                    is_nullable: Some(col.is_nullable),
                    ..Default::default()
                },
            });
            graph.add_relationship(GraphRelationship {
                id: format!("has_column_{}_{}", entity_id, col_id),
                source_id: entity_id.clone(),
                target_id: col_id,
                rel_type: RelationshipType::HasColumn,
                confidence: 1.0,
                reason: "db_schema".to_string(),
                step: None,
            });
            stats.columns += 1;
        }

        for (_col, target) in &table.foreign_keys {
            fk_pairs.push((table.name.clone(), target.clone()));
        }
    }

    // Emit ReferencesTable edges only when both ends exist.
    for (src_table, tgt_table) in fk_pairs {
        let src_id = generate_id("DbEntity", &src_table);
        let tgt_id = generate_id("DbEntity", &tgt_table);
        if graph.get_node(&src_id).is_some() && graph.get_node(&tgt_id).is_some() {
            graph.add_relationship(GraphRelationship {
                id: format!("refs_{}_{}", src_id, tgt_id),
                source_id: src_id,
                target_id: tgt_id,
                rel_type: RelationshipType::ReferencesTable,
                confidence: 0.9,
                reason: "foreign_key".to_string(),
                step: None,
            });
            stats.foreign_keys += 1;
        }
    }

    // ORM class → DbEntity mappings: emit RepresentedBy edges from existing
    // Class nodes (if the parsing phase captured them) to the entity.
    // Resolve the class IDs first (immutable borrow) before mutating the graph.
    let mut resolved: Vec<(String, String, String)> = Vec::new(); // (class_id, entity_id, table_name)
    for m in &mappings {
        let entity_id = generate_id("DbEntity", &m.table_name);
        let class_id = graph
            .iter_nodes()
            .find(|n| {
                n.label == NodeLabel::Class
                    && n.properties.name == m.class_name
                    && n.properties.file_path == m.file_path
            })
            .map(|n| n.id.clone());
        if let Some(cid) = class_id {
            resolved.push((cid, entity_id, m.table_name.clone()));
        }
    }
    for (class_id, entity_id, table_name) in resolved {
        // Lazy-create the DbEntity if it wasn't in any migration.
        if graph.get_node(&entity_id).is_none() {
            graph.add_node(GraphNode {
                id: entity_id.clone(),
                label: NodeLabel::DbEntity,
                properties: NodeProperties {
                    name: table_name.clone(),
                    file_path: String::new(),
                    db_table_name: Some(table_name),
                    ..Default::default()
                },
            });
            stats.tables += 1;
        }
        graph.add_relationship(GraphRelationship {
            id: format!("repr_{}_{}", class_id, entity_id),
            source_id: class_id,
            target_id: entity_id,
            rel_type: RelationshipType::RepresentedBy,
            confidence: 0.85,
            reason: "orm_mapping".to_string(),
            step: None,
        });
        stats.orm_mappings += 1;
    }

    stats
}

fn should_skip(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    // EF6 / .edmx is handled by aspnet_mvc.
    if lower.ends_with(".edmx") {
        return true;
    }
    if lower.contains("/node_modules/")
        || lower.contains("/target/")
        || lower.contains("/dist/")
        || lower.contains("/build/")
        || lower.contains("/vendor/")
    {
        return true;
    }
    false
}

fn scan_file(file: &FileEntry) -> (Vec<ParsedTable>, Vec<OrmMapping>) {
    if file.content.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let lower = file.path.to_ascii_lowercase();

    if lower.ends_with(".sql") {
        return (parse_sql(file), Vec::new());
    }
    if lower.ends_with("schema.prisma") {
        return (parse_prisma(file), Vec::new());
    }
    if lower.ends_with(".py") {
        return (Vec::new(), parse_sqlalchemy(file));
    }
    if lower.ends_with(".ts") || lower.ends_with(".js") || lower.ends_with(".tsx") {
        return (Vec::new(), parse_typeorm(file));
    }
    (Vec::new(), Vec::new())
}

// ─── SQL migrations ─────────────────────────────────────────────────────────

fn parse_sql(file: &FileEntry) -> Vec<ParsedTable> {
    let mut tables = Vec::new();
    let content = &file.content;
    let mut cursor = 0usize;
    while let Some(cap) = RE_CREATE_TABLE_HEAD.captures_at(content, cursor) {
        let head = cap.get(0).unwrap();
        let table_name = cap.get(1).unwrap().as_str().to_string();
        let start_line = offset_to_line(content, head.start());
        // The regex ends on the `(`. Walk forward from that point balancing
        // parens to find the matching `)`.
        let body_start = head.end(); // right after `(`
        let body_end = match find_matching_paren(content, body_start) {
            Some(end) => end,
            None => {
                cursor = head.end();
                continue;
            }
        };
        let body = &content[body_start..body_end];
        let (columns, foreign_keys) = parse_sql_body(body);
        tables.push(ParsedTable {
            name: table_name,
            file_path: file.path.clone(),
            start_line,
            columns,
            foreign_keys,
        });
        cursor = body_end + 1;
    }
    tables
}

/// Given `content` and a byte offset just AFTER an opening `(`, return the
/// offset of the matching closing `)`, or None if unbalanced.
fn find_matching_paren(content: &str, start: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    let mut depth = 1i32;
    let mut i = start;
    let mut in_string: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match in_string {
            Some(q) => {
                if b == q {
                    in_string = None;
                } else if b == b'\\' && i + 1 < bytes.len() {
                    i += 1;
                }
            }
            None => match b {
                b'\'' | b'"' | b'`' => in_string = Some(b),
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

fn parse_sql_body(body: &str) -> (Vec<ParsedColumn>, Vec<(String, String)>) {
    let mut cols = Vec::new();
    let mut fks = Vec::new();
    // Split on commas at the top level — this is a pragmatic approximation
    // that misbehaves on nested parens (CHECK constraints, composite types).
    // Good enough for the vast majority of schemas; TODO(theme-d) for a
    // proper tokenizer.
    for raw in split_top_level_commas(body) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        // Constraint lines (PRIMARY KEY / FOREIGN KEY / CONSTRAINT … FK).
        if upper.starts_with("PRIMARY KEY")
            || upper.starts_with("UNIQUE")
            || upper.starts_with("CHECK")
        {
            continue;
        }
        if upper.starts_with("FOREIGN KEY") || upper.starts_with("CONSTRAINT") {
            if let Some(cap) = RE_FOREIGN_KEY_FULL.captures(line) {
                let col = cap.get(1).unwrap().as_str().to_string();
                let target = cap.get(2).unwrap().as_str().to_string();
                fks.push((col, target));
            }
            continue;
        }
        // Column definition: first token = name, next token = type.
        let mut parts = line.split_whitespace();
        let name = match parts.next() {
            Some(n) => n
                .trim_matches(|c: char| c == '`' || c == '"' || c == '[' || c == ']')
                .to_string(),
            None => continue,
        };
        if name.is_empty() {
            continue;
        }
        let col_type = parts.next().unwrap_or("").to_string();
        let is_pk = upper.contains("PRIMARY KEY");
        let is_nullable = !upper.contains("NOT NULL");
        // Inline REFERENCES: `user_id INT REFERENCES users(id)`.
        if let Some(cap) = RE_INLINE_REFERENCES.captures(line) {
            if let Some(target) = cap.get(1) {
                fks.push((name.clone(), target.as_str().to_string()));
            }
        }
        cols.push(ParsedColumn {
            name,
            column_type: col_type,
            is_primary_key: is_pk,
            is_nullable,
        });
    }
    (cols, fks)
}

fn split_top_level_commas(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut depth = 0i32;
    for ch in body.chars() {
        match ch {
            '(' => {
                depth += 1;
                buf.push(ch);
            }
            ')' => {
                depth -= 1;
                buf.push(ch);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut buf));
            }
            _ => buf.push(ch),
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf);
    }
    out
}

fn offset_to_line(content: &str, offset: usize) -> u32 {
    let mut line = 1u32;
    for (i, ch) in content.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
        }
    }
    line
}

// ─── Prisma ─────────────────────────────────────────────────────────────────

fn parse_prisma(file: &FileEntry) -> Vec<ParsedTable> {
    let mut tables = Vec::new();
    for cap in RE_PRISMA_MODEL.captures_iter(&file.content) {
        let name = cap.get(1).unwrap().as_str().to_string();
        let body = cap.get(2).unwrap().as_str();
        let mat = cap.get(0).unwrap();
        let start_line = offset_to_line(&file.content, mat.start());

        let mut cols = Vec::new();
        let mut fks = Vec::new();
        for line in body.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with("//") || t.starts_with("@@") {
                continue;
            }
            // Prisma field:  fieldName  Type  modifiers...
            let mut parts = t.split_whitespace();
            let Some(fname) = parts.next() else { continue };
            let Some(ftype) = parts.next() else { continue };
            // Foreign keys: `@relation(references: [id]) userId Int @relation(...)`
            let has_relation = t.contains("@relation");
            if has_relation {
                // The target type is the Prisma field type (strip `?` and `[]`).
                let target = ftype.trim_end_matches('?').trim_end_matches("[]");
                fks.push((fname.to_string(), target.to_string()));
            }
            cols.push(ParsedColumn {
                name: fname.to_string(),
                column_type: ftype.to_string(),
                is_primary_key: t.contains("@id"),
                is_nullable: ftype.ends_with('?'),
            });
        }

        tables.push(ParsedTable {
            name,
            file_path: file.path.clone(),
            start_line,
            columns: cols,
            foreign_keys: fks,
        });
    }
    tables
}

// ─── SQLAlchemy ─────────────────────────────────────────────────────────────

fn parse_sqlalchemy(file: &FileEntry) -> Vec<OrmMapping> {
    let mut out = Vec::new();
    let content = &file.content;
    for cap in RE_SQLALCHEMY_CLASS.captures_iter(content) {
        let class_name = cap.get(1).unwrap().as_str().to_string();
        let class_start = cap.get(0).unwrap().start();
        // Find the nearest __tablename__ within the next ~2000 bytes. Snap the
        // arithmetic offset to a char boundary — a raw `class_start + 2000` can
        // land inside a multi-byte char (e.g. an em-dash in a docstring) and
        // panic the slice.
        let window_end = super::text_util::floor_char_boundary(content, class_start + 2000);
        let window = &content[class_start..window_end];
        let table_name = RE_SQLALCHEMY_TABLENAME
            .captures(window)
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
            // Fall back to a snake-cased class name.
            .unwrap_or_else(|| to_snake_case(&class_name));
        out.push(OrmMapping {
            class_name,
            file_path: file.path.clone(),
            table_name,
        });
    }
    out
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

// ─── TypeORM ────────────────────────────────────────────────────────────────

fn parse_typeorm(file: &FileEntry) -> Vec<OrmMapping> {
    let mut out = Vec::new();
    let content = &file.content;
    let mut cursor = 0;
    while let Some(cap) = RE_TYPEORM_ENTITY.captures_at(content, cursor) {
        let entity_mat = cap.get(0).unwrap();
        let annotated_table = cap.get(1).map(|m| m.as_str().to_string());
        // Find the class declaration after this annotation.
        let rest = &content[entity_mat.end()..];
        if let Some(cls_cap) = RE_TS_CLASS.captures(rest) {
            let class_name = cls_cap.get(1).unwrap().as_str().to_string();
            let table_name = annotated_table.unwrap_or_else(|| to_snake_case(&class_name));
            out.push(OrmMapping {
                class_name,
                file_path: file.path.clone(),
                table_name,
            });
        }
        cursor = entity_mat.end();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::config::languages::SupportedLanguage;

    fn fe(path: &str, content: &str, lang: SupportedLanguage) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            content: content.to_string(),
            language: Some(lang),
            size: content.len(),
        }
    }

    fn fe_sql(path: &str, content: &str) -> FileEntry {
        // SQL is not in SupportedLanguage; language = None is fine because
        // scan_file dispatches on file extension, not the language field.
        FileEntry {
            path: path.to_string(),
            content: content.to_string(),
            language: None,
            size: content.len(),
        }
    }

    #[test]
    fn test_parse_sqlalchemy_unicode_docstring_no_panic() {
        // Regression: parse_sqlalchemy sliced `&content[class_start..class_start+2000]`
        // on raw byte offsets and panicked when byte 2000 landed inside a multi-byte
        // char (an em-dash in a docstring crashed indexing of hermes-agent).
        let mut content = String::from("class Widget(Base):\n    \"\"\"");
        // Push an em-dash so it straddles byte index 2000 (class_start == 0 here).
        let dash_start = 1999usize; // em-dash occupies dash_start..dash_start+3
        let pad = dash_start.saturating_sub(content.len());
        content.push_str(&"a".repeat(pad));
        content.push('—'); // 3 bytes; byte 2000 is interior
        content.push_str("\n    docstring tail\n    \"\"\"\n    __tablename__ = \"widgets\"\n");
        // Preconditions that make this exercise the bug:
        assert!(content.len() > 2000, "window must not be clamped to len");
        assert!(!content.is_char_boundary(2000), "byte 2000 must be mid-char");

        let file = fe("models.py", &content, SupportedLanguage::Python);
        // Must not panic.
        let mappings = parse_sqlalchemy(&file);
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].class_name, "Widget");
    }

    #[test]
    fn test_split_top_level_commas_ignores_nested() {
        let parts = split_top_level_commas("a INT, b DECIMAL(10, 2), c TEXT");
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_parse_sql_basic() {
        let file = fe_sql(
            "db/migrations/001.sql",
            "CREATE TABLE users (\n  id INT PRIMARY KEY,\n  email VARCHAR(255) NOT NULL\n);",
        );
        let tables = parse_sql(&file);
        assert_eq!(tables.len(), 1);
        let t = &tables[0];
        assert_eq!(t.name, "users");
        assert_eq!(t.columns.len(), 2);
        assert!(t.columns[0].is_primary_key);
        assert!(!t.columns[1].is_nullable);
    }

    #[test]
    fn test_parse_sql_foreign_key() {
        let file = fe_sql(
            "schema.sql",
            "CREATE TABLE orders (id INT, user_id INT, FOREIGN KEY (user_id) REFERENCES users(id));",
        );
        let tables = parse_sql(&file);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].foreign_keys.len(), 1);
        assert_eq!(tables[0].foreign_keys[0].1, "users");
    }

    #[test]
    fn test_parse_prisma_model() {
        let file = fe(
            "prisma/schema.prisma",
            "model User {\n  id Int @id\n  email String\n  posts Post[] @relation\n}\n",
            SupportedLanguage::TypeScript, // language is irrelevant here
        );
        let tables = parse_prisma(&file);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "User");
        assert!(tables[0].columns.iter().any(|c| c.is_primary_key));
    }

    #[test]
    fn test_parse_sqlalchemy() {
        let file = fe(
            "models.py",
            "class User(Base):\n    __tablename__ = 'users'\n    id = Column(Integer, primary_key=True)\n",
            SupportedLanguage::Python,
        );
        let maps = parse_sqlalchemy(&file);
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].class_name, "User");
        assert_eq!(maps[0].table_name, "users");
    }

    #[test]
    fn test_parse_typeorm_entity() {
        let file = fe(
            "user.entity.ts",
            "import { Entity } from 'typeorm';\n@Entity('users')\nexport class User {}\n",
            SupportedLanguage::TypeScript,
        );
        let maps = parse_typeorm(&file);
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].table_name, "users");
    }

    #[test]
    fn test_extract_end_to_end() {
        let mut graph = KnowledgeGraph::new();
        // Pre-seed a Class node that the ORM mapping can link to.
        graph.add_node(GraphNode {
            id: "Class:models.py:User".to_string(),
            label: NodeLabel::Class,
            properties: NodeProperties {
                name: "User".to_string(),
                file_path: "models.py".to_string(),
                ..Default::default()
            },
        });
        let sql = fe_sql(
            "db/001.sql",
            "CREATE TABLE users (id INT PRIMARY KEY, email VARCHAR(255));",
        );
        let py = fe(
            "models.py",
            "class User(Base):\n    __tablename__ = 'users'\n",
            SupportedLanguage::Python,
        );
        let stats = extract_db_schema(&mut graph, &[sql, py]);
        assert_eq!(stats.tables, 1);
        assert_eq!(stats.columns, 2);
        assert_eq!(stats.orm_mappings, 1);
        assert!(graph
            .iter_nodes()
            .any(|n| n.label == NodeLabel::DbEntity && n.properties.name == "users"));
        assert!(graph
            .iter_relationships()
            .any(|r| r.rel_type == RelationshipType::RepresentedBy));
    }
}
