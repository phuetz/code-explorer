//! Phase: Config / env-var inventory (Theme D).
//!
//! Scans the repository for environment-variable declarations in config
//! files (`.env*`, `appsettings*.json`, `application.properties`,
//! `application.yml`, `config.toml`) and code references
//! (`process.env.X`, `os.getenv("X")`, `Environment.GetEnvironmentVariable`,
//! `System.getenv`, `Configuration["X"]`), then produces `EnvVar` nodes with
//! cross-linked `UsesEnvVar` edges.
//!
//! Each EnvVar is tagged:
//! - `declared_in`: path of the first config file declaring it (or `None`).
//! - `used_in_count`: number of distinct (file,line) code references.
//! - `unused`: declared but never referenced.
//! - `undeclared`: referenced but not declared in any config.
//!
//! ## Limitations
//!
//! Regex-based; dynamic names like `process.env[someVar]` are ignored.

use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::id::generate_id;
use once_cell::sync::Lazy;
use rayon::prelude::*;
use regex::Regex;
use std::collections::{HashMap, HashSet};

use crate::phases::structure::FileEntry;

#[derive(Debug, Default, Clone, Copy)]
pub struct ConfigInventoryStats {
    pub declared: usize,
    pub referenced: usize,
    pub unused: usize,
    pub undeclared: usize,
    pub edges: usize,
}

// ─── Regexes ───────────────────────────────────────────────────────────────

static RE_DOTENV_LINE: Lazy<Regex> = Lazy::new(|| {
    // KEY=value, KEY="value", KEY='value'. Ignore comments and blank lines.
    Regex::new(r#"(?m)^\s*([A-Z][A-Z0-9_]*)\s*="#).expect("dotenv regex")
});

static RE_PROPS_LINE: Lazy<Regex> = Lazy::new(|| {
    // application.properties: key.with.dots=value
    Regex::new(r#"(?m)^\s*([a-zA-Z][\w.\-]*)\s*="#).expect("props regex")
});

static RE_YAML_KEY: Lazy<Regex> = Lazy::new(|| {
    // Top-level keys only. We intentionally don't recurse — YAML is nested
    // and we'd need a parser for full fidelity (TODO(theme-d)).
    Regex::new(r#"(?m)^([A-Za-z][\w\-]*)\s*:"#).expect("yaml regex")
});

static RE_PROCESS_ENV_DOT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"process\.env\.([A-Z][A-Z0-9_]*)"#).expect("process.env regex"));

static RE_PROCESS_ENV_BRACKET: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"process\.env\[\s*['"]([^'"]+)['"]\s*\]"#).expect("process.env[...] regex")
});

static RE_OS_GETENV: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"os\.getenv\(\s*['"]([^'"]+)['"]"#).expect("os.getenv regex"));

static RE_OS_ENVIRON: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"os\.environ\[\s*['"]([^'"]+)['"]\s*\]"#).expect("os.environ regex"));

static RE_DOTNET_ENV: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?:Environment\.GetEnvironmentVariable|Configuration)\s*(?:\(\s*|\[\s*)['"]([^'"]+)['"]"#,
    )
    .expect("dotnet env regex")
});

static RE_JAVA_GETENV: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"System\.getenv\(\s*['"]([^'"]+)['"]"#).expect("java getenv regex"));

// ─── Internal models ───────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct Declaration {
    file: String,
    line: u32,
}

#[derive(Debug, Clone)]
struct Reference {
    file: String,
    line: u32,
}

/// Parsed declarations and references collected from a single config/source file.
type ScanResult = (Vec<(String, Declaration)>, Vec<(String, Reference)>);

// ─── Entry point ───────────────────────────────────────────────────────────

pub fn extract_env_vars(graph: &mut KnowledgeGraph, files: &[FileEntry]) -> ConfigInventoryStats {
    let scanned: Vec<ScanResult> = files
        .par_iter()
        .filter(|f| !should_skip(&f.path))
        .map(scan_file)
        .collect();

    let mut decls: HashMap<String, Declaration> = HashMap::new();
    let mut refs: HashMap<String, Vec<Reference>> = HashMap::new();

    for (d, r) in scanned {
        for (name, dec) in d {
            // First declaration wins.
            decls.entry(name).or_insert(dec);
        }
        for (name, re) in r {
            refs.entry(name).or_default().push(re);
        }
    }

    let mut stats = ConfigInventoryStats::default();
    let mut all_names: HashSet<String> = HashSet::new();
    all_names.extend(decls.keys().cloned());
    all_names.extend(refs.keys().cloned());

    for name in all_names {
        let node_id = generate_id("EnvVar", &name);
        let decl = decls.get(&name);
        let ref_list = refs.get(&name).cloned().unwrap_or_default();
        let used_count = ref_list.len() as u32;
        let unused = decl.is_some() && ref_list.is_empty();
        let undeclared = decl.is_none() && !ref_list.is_empty();

        if decl.is_some() {
            stats.declared += 1;
        }
        if !ref_list.is_empty() {
            stats.referenced += 1;
        }
        if unused {
            stats.unused += 1;
        }
        if undeclared {
            stats.undeclared += 1;
        }

        let file_path = decl
            .map(|d| d.file.clone())
            .or_else(|| ref_list.first().map(|r| r.file.clone()))
            .unwrap_or_default();
        let start_line = decl
            .map(|d| d.line)
            .or_else(|| ref_list.first().map(|r| r.line))
            .unwrap_or(1);

        graph.add_node(GraphNode {
            id: node_id.clone(),
            label: NodeLabel::EnvVar,
            properties: NodeProperties {
                name: name.clone(),
                file_path,
                start_line: Some(start_line),
                declared_in: decl.map(|d| d.file.clone()),
                used_in_count: Some(used_count),
                unused: if unused { Some(true) } else { None },
                undeclared: if undeclared { Some(true) } else { None },
                ..Default::default()
            },
        });

        // Emit UsesEnvVar edges from each referencing File to the EnvVar.
        // We dedupe by file path so a variable used five times in one file
        // produces one edge, not five.
        let mut linked_files: HashSet<String> = HashSet::new();
        for r in ref_list {
            if !linked_files.insert(r.file.clone()) {
                continue;
            }
            let file_id = generate_id("File", &r.file);
            if graph.get_node(&file_id).is_none() {
                // Structure phase should already have created the file node;
                // if it didn't, skip the edge rather than fabricating a file.
                continue;
            }
            graph.add_relationship(GraphRelationship {
                id: format!("uses_env_{}_{}", file_id, node_id),
                source_id: file_id,
                target_id: node_id.clone(),
                rel_type: RelationshipType::UsesEnvVar,
                confidence: 1.0,
                reason: "env_scan".to_string(),
                step: None,
            });
            stats.edges += 1;
        }
    }

    stats
}

fn should_skip(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if lower.contains("/node_modules/")
        || lower.contains("/target/")
        || lower.contains("/dist/")
        || lower.contains("/build/")
        || lower.contains("/vendor/")
        || lower.starts_with("node_modules/")
        || lower.starts_with("target/")
        || lower.starts_with("dist/")
        || lower.starts_with("build/")
        || lower.starts_with("vendor/")
    {
        return true;
    }
    false
}

fn scan_file(file: &FileEntry) -> ScanResult {
    let mut decls: Vec<(String, Declaration)> = Vec::new();
    let mut refs: Vec<(String, Reference)> = Vec::new();
    if file.content.is_empty() {
        return (decls, refs);
    }

    if is_env_file(&file.path) {
        for cap in RE_DOTENV_LINE.captures_iter(&file.content) {
            let name = cap.get(1).unwrap().as_str().to_string();
            let line = offset_to_line(&file.content, cap.get(0).unwrap().start());
            decls.push((
                name,
                Declaration {
                    file: file.path.clone(),
                    line,
                },
            ));
        }
        return (decls, refs);
    }

    if is_appsettings_json(&file.path) {
        // Very loose: every top-level key. A proper implementation would
        // flatten nested paths via "Section:Subsection:Key" — TODO(theme-d).
        for (idx, line) in file.content.lines().enumerate() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    let key = &rest[..end];
                    // Skip structural keys that are unlikely to be env vars.
                    if !key.is_empty() {
                        decls.push((
                            key.to_string(),
                            Declaration {
                                file: file.path.clone(),
                                line: (idx + 1) as u32,
                            },
                        ));
                    }
                }
            }
        }
        return (decls, refs);
    }

    if is_properties_file(&file.path) {
        for cap in RE_PROPS_LINE.captures_iter(&file.content) {
            let name = cap.get(1).unwrap().as_str().to_string();
            let line = offset_to_line(&file.content, cap.get(0).unwrap().start());
            decls.push((
                name,
                Declaration {
                    file: file.path.clone(),
                    line,
                },
            ));
        }
        return (decls, refs);
    }

    if is_yaml_file(&file.path) {
        for cap in RE_YAML_KEY.captures_iter(&file.content) {
            let name = cap.get(1).unwrap().as_str().to_string();
            let line = offset_to_line(&file.content, cap.get(0).unwrap().start());
            decls.push((
                name,
                Declaration {
                    file: file.path.clone(),
                    line,
                },
            ));
        }
        return (decls, refs);
    }

    if file.path.to_ascii_lowercase().ends_with("config.toml") {
        for cap in RE_DOTENV_LINE.captures_iter(&file.content) {
            let name = cap.get(1).unwrap().as_str().to_string();
            let line = offset_to_line(&file.content, cap.get(0).unwrap().start());
            decls.push((
                name,
                Declaration {
                    file: file.path.clone(),
                    line,
                },
            ));
        }
        return (decls, refs);
    }

    // Source code → collect references.
    for (idx, line) in file.content.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let scan = |re: &Regex, text: &str, out: &mut Vec<(String, Reference)>| {
            for cap in re.captures_iter(text) {
                if let Some(m) = cap.get(1) {
                    out.push((
                        m.as_str().to_string(),
                        Reference {
                            file: file.path.clone(),
                            line: line_num,
                        },
                    ));
                }
            }
        };
        scan(&RE_PROCESS_ENV_DOT, line, &mut refs);
        scan(&RE_PROCESS_ENV_BRACKET, line, &mut refs);
        scan(&RE_OS_GETENV, line, &mut refs);
        scan(&RE_OS_ENVIRON, line, &mut refs);
        scan(&RE_DOTNET_ENV, line, &mut refs);
        scan(&RE_JAVA_GETENV, line, &mut refs);
    }

    (decls, refs)
}

fn is_env_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or("");
    // .env, .env.local, .env.production, etc.
    filename == ".env" || filename.starts_with(".env.")
}

fn is_appsettings_json(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or("");
    filename.starts_with("appsettings") && filename.ends_with(".json")
}

fn is_properties_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or("");
    filename == "application.properties" || filename.ends_with(".properties")
}

fn is_yaml_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or("");
    // Only pick up application.yml/yaml — generic YAML files are usually
    // Kubernetes manifests or CI config, not env var declarations.
    filename == "application.yml" || filename == "application.yaml"
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

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::config::languages::SupportedLanguage;

    fn fe(path: &str, content: &str) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            content: content.to_string(),
            language: SupportedLanguage::from_filename(path),
            size: content.len(),
        }
    }

    #[test]
    fn test_dotenv_declaration() {
        let file = fe(
            ".env",
            "DATABASE_URL=postgres://...\nAPI_KEY=xxx\n# comment\n",
        );
        let (decls, refs) = scan_file(&file);
        assert_eq!(decls.len(), 2);
        assert!(refs.is_empty());
        let names: Vec<_> = decls.iter().map(|(n, _)| n.clone()).collect();
        assert!(names.contains(&"DATABASE_URL".to_string()));
        assert!(names.contains(&"API_KEY".to_string()));
    }

    #[test]
    fn test_process_env_references() {
        let file = fe(
            "src/config.js",
            "const url = process.env.DATABASE_URL;\nconst key = process.env['API_KEY'];\n",
        );
        let (decls, refs) = scan_file(&file);
        assert!(decls.is_empty());
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_python_references() {
        let file = fe(
            "src/app.py",
            "import os\nDB = os.getenv('DB_URL')\nKEY = os.environ['SECRET']\n",
        );
        let (_, refs) = scan_file(&file);
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_dotnet_env_reference() {
        let file = fe(
            "Program.cs",
            "var x = Environment.GetEnvironmentVariable(\"APP_ENV\");\n",
        );
        let (_, refs) = scan_file(&file);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0, "APP_ENV");
    }

    #[test]
    fn test_java_references() {
        let file = fe("Foo.java", "String v = System.getenv(\"JAVA_HOME\");\n");
        let (_, refs) = scan_file(&file);
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn test_extract_end_to_end_marks_unused_and_undeclared() {
        let mut graph = KnowledgeGraph::new();
        // Required: File nodes for files referencing env vars.
        graph.add_node(GraphNode {
            id: "File:src/app.js".to_string(),
            label: NodeLabel::File,
            properties: NodeProperties {
                name: "app.js".to_string(),
                file_path: "src/app.js".to_string(),
                ..Default::default()
            },
        });
        let env = fe(".env", "USED_VAR=1\nUNUSED_VAR=2\n");
        let js = fe(
            "src/app.js",
            "console.log(process.env.USED_VAR, process.env.NEW_VAR);",
        );
        let stats = extract_env_vars(&mut graph, &[env, js]);
        assert!(stats.declared >= 2);
        assert!(stats.referenced >= 2);
        // Expect UNUSED_VAR flagged unused, NEW_VAR flagged undeclared.
        let unused = graph
            .iter_nodes()
            .find(|n| n.properties.name == "UNUSED_VAR");
        assert!(unused.unwrap().properties.unused.unwrap_or(false));
        let new_var = graph.iter_nodes().find(|n| n.properties.name == "NEW_VAR");
        assert!(new_var.unwrap().properties.undeclared.unwrap_or(false));
    }
}
