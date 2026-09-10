//! HTML interactive export — produces a single self-contained HTML file
//! that renders the active repo's graph using Sigma.js loaded from a CDN.
//!
//! No Tauri / no Code Explorer runtime needed to view it — drop the file in any
//! browser. Useful for sharing a snapshot of the architecture with someone
//! who can't install the desktop app.

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HtmlExportRequest {
    /// Output path for the HTML file. If omitted, defaults to
    /// `<storage_path>/export-graph.html`.
    #[serde(default)]
    pub out_path: Option<String>,
    /// Maximum number of nodes to include (default: 2000).
    /// Larger graphs would lock the browser; user can override.
    #[serde(default)]
    pub max_nodes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HtmlExportResult {
    pub path: String,
    pub node_count: u32,
    pub edge_count: u32,
    /// Bytes written.
    pub size: u64,
}

#[derive(Serialize)]
struct ExportedNode<'a> {
    id: &'a str,
    label: String,
    kind: &'a str,
    file: &'a str,
}

#[derive(Serialize)]
struct ExportedEdge<'a> {
    source: &'a str,
    target: &'a str,
    kind: &'a str,
}

#[tauri::command]
pub async fn export_interactive_html(
    state: State<'_, AppState>,
    request: HtmlExportRequest,
) -> Result<HtmlExportResult, String> {
    let max_nodes = request.max_nodes.unwrap_or(2000) as usize;
    let storage = state.active_storage_path().await?;
    let (graph, _idx, _fts, repo_path) = state.get_repo(None).await?;

    // Pick the most-connected nodes first to fit under max_nodes when needed.
    let mut node_degree: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for rel in graph.iter_relationships() {
        *node_degree.entry(rel.source_id.clone()).or_insert(0) += 1;
        *node_degree.entry(rel.target_id.clone()).or_insert(0) += 1;
    }
    let mut all_nodes: Vec<(&String, u32)> = graph
        .iter_nodes()
        .map(|n| (&n.id, *node_degree.get(&n.id).unwrap_or(&0)))
        .collect();
    all_nodes.sort_by(|a, b| b.1.cmp(&a.1));
    let kept_ids: std::collections::HashSet<String> = all_nodes
        .iter()
        .take(max_nodes)
        .map(|(id, _)| (*id).clone())
        .collect();

    let exported_nodes: Vec<ExportedNode> = graph
        .iter_nodes()
        .filter(|n| kept_ids.contains(&n.id))
        .map(|n| ExportedNode {
            id: &n.id,
            label: n.properties.name.clone(),
            kind: n.label.as_str(),
            file: &n.properties.file_path,
        })
        .collect();

    let exported_edges: Vec<ExportedEdge> = graph
        .iter_relationships()
        .filter(|r| kept_ids.contains(&r.source_id) && kept_ids.contains(&r.target_id))
        .map(|r| ExportedEdge {
            source: &r.source_id,
            target: &r.target_id,
            kind: r.rel_type.as_str(),
        })
        .collect();

    let nodes_json = serde_json::to_string(&exported_nodes).map_err(|e| e.to_string())?;
    let edges_json = serde_json::to_string(&exported_edges).map_err(|e| e.to_string())?;

    let html = build_html(&repo_path, &nodes_json, &edges_json);

    let out_path = request
        .out_path
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&storage).join("export-graph.html"));

    // Containment check: out_path must be under the storage directory
    let canonical_storage = std::path::Path::new(&storage)
        .canonicalize()
        .map_err(|e| format!("Cannot resolve storage path: {e}"))?;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let canonical_out = out_path
        .canonicalize()
        .or_else(|_| {
            // File may not exist yet — canonicalize parent and append filename
            out_path
                .parent()
                .and_then(|p| p.canonicalize().ok())
                .map(|p| p.join(out_path.file_name().unwrap_or_default()))
                .ok_or_else(|| std::io::Error::other("invalid path"))
        })
        .map_err(|e| format!("Invalid out_path: {e}"))?;
    if !canonical_out.starts_with(&canonical_storage) {
        return Err("out_path must be inside the storage directory".to_string());
    }

    let mut f = std::fs::File::create(&canonical_out).map_err(|e| e.to_string())?;
    f.write_all(html.as_bytes()).map_err(|e| e.to_string())?;

    let size = std::fs::metadata(&canonical_out)
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(HtmlExportResult {
        path: canonical_out.to_string_lossy().to_string(),
        node_count: exported_nodes.len() as u32,
        edge_count: exported_edges.len() as u32,
        size,
    })
}

fn build_html(repo_label: &str, nodes_json: &str, edges_json: &str) -> String {
    // Single self-contained HTML; uses graphology + sigma from a CDN.
    // Layout is computed client-side on first paint with ForceAtlas2.
    //
    // SECURITY: hover info is rendered with textContent (never innerHTML),
    // so node labels containing arbitrary characters cannot inject markup.
    let escaped_label = repo_label
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>Code Explorer — {escaped_label}</title>
<style>
  html, body {{ margin: 0; padding: 0; height: 100%; background: #1a1b26; color: #c0caf5; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
  #header {{ position: fixed; top: 0; left: 0; right: 0; padding: 8px 14px; background: rgba(20, 22, 38, 0.9); border-bottom: 1px solid #2a2e3f; backdrop-filter: blur(8px); z-index: 10; display: flex; align-items: center; gap: 12px; font-size: 12px; }}
  #header strong {{ color: #7aa2f7; }}
  #stats {{ color: #565f89; font-size: 11px; }}
  #search {{ margin-left: auto; padding: 4px 10px; background: #1a1b26; border: 1px solid #2a2e3f; border-radius: 6px; color: #c0caf5; font-size: 11px; outline: none; width: 240px; }}
  #stage {{ position: absolute; top: 36px; bottom: 0; left: 0; right: 0; }}
  #info {{ position: fixed; bottom: 12px; right: 12px; padding: 10px 14px; background: rgba(20, 22, 38, 0.95); border: 1px solid #2a2e3f; border-radius: 8px; font-size: 11px; max-width: 320px; display: none; }}
  #info .name {{ color: #e0af68; font-weight: 600; }}
  #info .meta {{ color: #565f89; font-size: 10px; margin-top: 2px; }}
  .footer {{ position: fixed; bottom: 6px; left: 12px; font-size: 9px; color: #565f89; }}
</style>
</head>
<body>
  <div id="header">
    <strong>Code Explorer</strong>
    <span>{escaped_label}</span>
    <span id="stats"></span>
    <input id="search" placeholder="Search node…" type="text" />
  </div>
  <div id="stage"></div>
  <div id="info">
    <div class="name"></div>
    <div class="meta"></div>
  </div>
  <div class="footer">Self-contained snapshot — no Code Explorer runtime needed.</div>

  <script src="https://cdn.jsdelivr.net/npm/graphology@0.26.0/dist/graphology.umd.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/graphology-layout-forceatlas2@0.10.1/build/graphology-layout-forceatlas2.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/sigma@3.0.2/build/sigma.min.js"></script>
  <script>
    const NODES = {nodes_json};
    const EDGES = {edges_json};

    const KIND_COLORS = {{
      Function: '#7aa2f7', Method: '#7dcfff', Class: '#bb9af7',
      Interface: '#9ece6a', File: '#565f89', Folder: '#414868',
      Community: '#e0af68', Process: '#f7768e', Service: '#9ece6a',
      Controller: '#e0af68', ControllerAction: '#f7768e',
    }};

    const Graph = graphology.Graph;
    const g = new Graph();

    NODES.forEach((n, idx) => {{
      g.addNode(n.id, {{
        label: n.label,
        size: 4,
        color: KIND_COLORS[n.kind] || '#a9b1d6',
        kind: n.kind,
        file: n.file,
        x: Math.cos(idx) * 200,
        y: Math.sin(idx) * 200,
      }});
    }});
    EDGES.forEach((e, i) => {{
      if (g.hasNode(e.source) && g.hasNode(e.target) && e.source !== e.target) {{
        try {{ g.addEdgeWithKey('e' + i, e.source, e.target, {{ size: 0.5, color: '#3b4261', kind: e.kind }}); }} catch (_) {{}}
      }}
    }});

    document.getElementById('stats').textContent = g.order + ' nodes · ' + g.size + ' edges';

    // Layout (synchronous, capped iterations).
    graphologyLibrary.ForceAtlas2.assign(g, {{
      iterations: 80,
      settings: {{ gravity: 1, scalingRatio: 8, slowDown: 5, barnesHutOptimize: g.order > 500 }},
    }});

    const renderer = new Sigma(g, document.getElementById('stage'), {{
      labelDensity: 0.07,
      labelGridCellSize: 60,
      labelRenderedSizeThreshold: 6,
      defaultEdgeColor: '#3b4261',
    }});

    // Hover info — textContent only, never innerHTML.
    const info = document.getElementById('info');
    const infoName = info.querySelector('.name');
    const infoMeta = info.querySelector('.meta');
    renderer.on('enterNode', (e) => {{
      const a = g.getNodeAttributes(e.node);
      infoName.textContent = String(a.label || '');
      infoMeta.textContent = String(a.kind || '') + (a.file ? ' · ' + String(a.file) : '');
      info.style.display = 'block';
    }});
    renderer.on('leaveNode', () => {{ info.style.display = 'none'; }});

    // Search
    const search = document.getElementById('search');
    search.addEventListener('input', (ev) => {{
      const q = String(ev.target.value || '').toLowerCase();
      g.forEachNode((id, attrs) => {{
        const match = !q || String(attrs.label || '').toLowerCase().includes(q);
        g.setNodeAttribute(id, 'hidden', !match);
      }});
      renderer.refresh();
    }});
  </script>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_html_contains_data() {
        let html = build_html("foo", "[]", "[]");
        assert!(html.contains("Code Explorer"));
        assert!(html.contains("Sigma"));
        assert!(html.contains("foo"));
    }

    #[test]
    fn test_build_html_escapes_label() {
        let html = build_html("<bad>", "[]", "[]");
        assert!(html.contains("&lt;bad&gt;"));
    }
}
