// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize)]
struct VaultEntry {
    name: String,
    path: String,
    is_dir: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GraphNode {
    id: String,
    label: String,
    group: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GraphEdge {
    source: String,
    target: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct VaultGraph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

fn canonical_vault_root(vault_path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(vault_path);
    if !root.exists() {
        return Err(format!("Vault path does not exist: {vault_path}"));
    }
    root.canonicalize()
        .map_err(|err| format!("Failed to canonicalize vault path: {err}"))
}

fn resolve_note_path(vault_path: &str, note_path: &str) -> Result<PathBuf, String> {
    let root = canonical_vault_root(vault_path)?;
    let candidate = root.join(note_path);
    let canonical = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|err| format!("Failed to resolve note path: {err}"))?
    } else {
        let parent = candidate
            .parent()
            .ok_or_else(|| "Invalid note path".to_string())?
            .canonicalize()
            .map_err(|err| format!("Failed to resolve note parent: {err}"))?;
        let file_name = candidate
            .file_name()
            .ok_or_else(|| "Invalid note path".to_string())?;
        parent.join(file_name)
    };

    if !canonical.starts_with(&root) {
        return Err("Refusing to access a note outside of the vault".to_string());
    }

    Ok(canonical)
}

fn note_id_from_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
}

fn link_target_id(target: &str) -> String {
    target.trim().trim_end_matches(".md").replace('\\', "/")
}

#[tauri::command]
async fn list_vault(path: String) -> Result<Vec<VaultEntry>, String> {
    let mut entries = Vec::new();
    let root = canonical_vault_root(&path)?;

    for entry in WalkDir::new(&root)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let name = entry.file_name().to_string_lossy().to_string();
        let relative_path = entry
            .path()
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| name.clone());

        entries.push(VaultEntry {
            name,
            path: relative_path,
            is_dir: entry.file_type().is_dir(),
        });
    }
    Ok(entries)
}

#[tauri::command]
async fn get_vault_graph(vault_path: String) -> Result<VaultGraph, String> {
    let root = canonical_vault_root(&vault_path)?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seen_nodes = HashSet::new();

    let re_link = Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]").unwrap();

    for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "md") {
            let rel_path = entry
                .path()
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            let id = note_id_from_path(&root, entry.path());
            let name = entry
                .path()
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_string();

            let group = if rel_path.contains("Modules") {
                "module"
            } else if rel_path.contains("Processus") {
                "process"
            } else if rel_path.contains("Symboles") {
                "symbol"
            } else {
                "file"
            };

            if seen_nodes.insert(id.clone()) {
                nodes.push(GraphNode {
                    id: id.clone(),
                    label: name,
                    group: group.to_string(),
                });
            }

            // Extract links
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                for cap in re_link.captures_iter(&content) {
                    edges.push(GraphEdge {
                        source: id.clone(),
                        target: link_target_id(&cap[1]),
                    });
                }
            }
        }
    }

    Ok(VaultGraph { nodes, edges })
}

#[tauri::command]
async fn read_note(vault_path: String, note_path: String) -> Result<String, String> {
    let full_path = resolve_note_path(&vault_path, &note_path)?;
    std::fs::read_to_string(full_path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_note(vault_path: String, note_path: String, content: String) -> Result<(), String> {
    let full_path = resolve_note_path(&vault_path, &note_path)?;
    std::fs::write(full_path, content).map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            list_vault,
            read_note,
            save_note,
            get_vault_graph
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
