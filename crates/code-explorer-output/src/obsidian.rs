//! Obsidian Vault generator.
//! Produces a Markdown-based knowledge base compatible with Obsidian.md.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use code_explorer_core::graph::types::{GraphNode, NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;

/// Strip the Windows extended-length path prefix `\\?\` from a path, if
/// present. The prefix is useful to bypass MAX_PATH limits but it disables
/// path normalization — which trips up several std::fs routines that
/// receive further path components via `.join()`. Stripping it restores
/// normal Windows path semantics and avoids `ERROR_INVALID_NAME` (123)
/// crashes deep inside `create_dir_all` / `File::create`.
#[cfg(windows)]
fn strip_verbatim_prefix(p: &Path) -> PathBuf {
    let s = p.as_os_str().to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        // Do NOT strip the `\\?\UNC\` form — that's an actual UNC path.
        if rest.starts_with("UNC\\") {
            p.to_path_buf()
        } else {
            PathBuf::from(rest.to_string())
        }
    } else {
        p.to_path_buf()
    }
}

#[cfg(not(windows))]
fn strip_verbatim_prefix(p: &Path) -> PathBuf {
    p.to_path_buf()
}

/// Create a directory, logging the exact path on failure so we can
/// diagnose Windows `ERROR_INVALID_NAME` crashes by reading the trace.
fn create_dir_all_traced(path: &Path) -> Result<()> {
    tracing::debug!("obsidian: create_dir_all {}", path.display());
    std::fs::create_dir_all(path)
        .with_context(|| format!("create_dir_all failed for: {}", path.display()))
}

/// Create (or truncate) a file, logging the exact path on failure. See
/// `create_dir_all_traced` — same rationale.
fn file_create_traced(path: &Path) -> Result<std::fs::File> {
    tracing::debug!("obsidian: File::create {}", path.display());
    std::fs::File::create(path)
        .with_context(|| format!("File::create failed for: {}", path.display()))
}

/// Context for community information
pub struct CommunityInfo {
    pub label: String,
    pub description: Option<String>,
    pub member_ids: Vec<String>,
}

fn stable_suffix(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")[..8].to_string()
}

fn truncate_sanitized_component(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let cut = value
        .char_indices()
        .take_while(|(i, _)| *i <= max_len)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);
    let trimmed = value[..cut]
        .trim_end_matches(|c: char| c.is_whitespace() || c == '.' || c == '_')
        .to_string();

    if trimmed.is_empty() {
        "_unnamed".to_string()
    } else {
        trimmed
    }
}

fn unique_note_stem(display_name: &str, stable_id: &str) -> String {
    let suffix = stable_suffix(stable_id);
    let base = sanitize_obsidian_filename(display_name);
    let available = MAX_FILENAME_LEN.saturating_sub(suffix.len() + 2).max(1);
    format!(
        "{}--{}",
        truncate_sanitized_component(&base, available),
        suffix
    )
}

fn node_display_for_stem(node: &GraphNode) -> &str {
    if node.label == NodeLabel::File && !node.properties.file_path.is_empty() {
        &node.properties.file_path
    } else {
        &node.properties.name
    }
}

fn node_note_stem(node: &GraphNode) -> String {
    unique_note_stem(node_display_for_stem(node), &node.id)
}

fn node_note_path(node: &GraphNode) -> String {
    match node.label {
        NodeLabel::File => format!("Fichiers/{}", node_note_stem(node)),
        NodeLabel::Process => format!("Processus/{}", node_note_stem(node)),
        NodeLabel::Community => format!("Modules/{}", node_note_stem(node)),
        _ => format!(
            "Symboles/{}/{}",
            sanitize_obsidian_filename(node.label.as_str()),
            node_note_stem(node)
        ),
    }
}

fn community_note_path(id: &str, info: &CommunityInfo) -> String {
    format!("Modules/{}", unique_note_stem(&info.label, id))
}

fn wikilink(path: &str, alias: &str) -> String {
    format!("[[{path}|{alias}]]")
}

fn wikilink_to_node(node: &GraphNode, alias: &str) -> String {
    wikilink(&node_note_path(node), alias)
}

fn wikilink_to_community(id: &str, info: &CommunityInfo) -> String {
    wikilink(&community_note_path(id, info), &info.label)
}

fn wikilink_for_target(
    target: &GraphNode,
    communities: &BTreeMap<String, CommunityInfo>,
) -> Option<String> {
    match target.label {
        NodeLabel::Document | NodeLabel::DocChunk => None,
        NodeLabel::Community => communities
            .get(&target.id)
            .map(|info| wikilink_to_community(&target.id, info)),
        _ => Some(wikilink_to_node(target, &target.properties.name)),
    }
}

fn wikilink_to_file_path(graph: &KnowledgeGraph, file_path: &str) -> Option<String> {
    graph
        .iter_nodes()
        .find(|node| node.label == NodeLabel::File && node.properties.file_path == file_path)
        .map(|node| wikilink_to_node(node, file_path))
}

fn yaml_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', " ")
        .replace('\n', " ")
}

pub fn generate_obsidian_vault(
    graph: &KnowledgeGraph,
    output_dir: &Path,
    communities: &BTreeMap<String, CommunityInfo>,
) -> Result<String> {
    // Strip the Windows `\\?\` extended-length prefix if present. Path
    // ops downstream (including recursive create_dir_all) misbehave on
    // verbatim paths once you start `.join()`ing human-readable
    // components, causing ERROR_INVALID_NAME (os error 123).
    let output_dir = strip_verbatim_prefix(output_dir);
    tracing::debug!("obsidian: normalized output_dir = {}", output_dir.display());

    let vault_dir = output_dir.join("obsidian_vault");
    if vault_dir.exists() {
        tracing::debug!("obsidian: remove_dir_all {}", vault_dir.display());
        std::fs::remove_dir_all(&vault_dir)
            .with_context(|| format!("remove_dir_all failed for: {}", vault_dir.display()))?;
    }
    create_dir_all_traced(&vault_dir)?;

    let nodes_dir = vault_dir.join("Symboles");
    let comms_dir = vault_dir.join("Modules");
    let proc_dir = vault_dir.join("Processus");
    let files_dir = vault_dir.join("Fichiers");

    create_dir_all_traced(&nodes_dir)?;
    create_dir_all_traced(&comms_dir)?;
    create_dir_all_traced(&proc_dir)?;
    create_dir_all_traced(&files_dir)?;

    let mut edge_map: HashMap<String, Vec<(String, RelationshipType)>> = HashMap::new();
    let mut incoming_map: HashMap<String, Vec<(String, RelationshipType)>> = HashMap::new();
    for rel in graph.iter_relationships() {
        edge_map
            .entry(rel.source_id.clone())
            .or_default()
            .push((rel.target_id.clone(), rel.rel_type));
        incoming_map
            .entry(rel.target_id.clone())
            .or_default()
            .push((rel.source_id.clone(), rel.rel_type));
    }

    // 1. Generate Index.md
    generate_index(&vault_dir, graph, communities)?;

    // 1b. Generate Log.md
    generate_log(&vault_dir, graph)?;

    // 1c. Generate .claudecode
    generate_claudecode_instructions(&vault_dir)?;

    // 2. Generate File pages
    for node in graph.iter_nodes().filter(|n| n.label == NodeLabel::File) {
        generate_file_page(&files_dir, node, graph, &edge_map, communities)?;
    }

    // 3. Generate Community pages
    for (id, info) in communities {
        generate_community_page(&comms_dir, id, info, graph, &edge_map, communities)?;
    }

    // 4. Generate Node pages
    for node in graph.iter_nodes().filter(|n| {
        n.label != NodeLabel::File
            && n.label != NodeLabel::Community
            && n.label != NodeLabel::Process
            && n.label != NodeLabel::Document
            && n.label != NodeLabel::DocChunk
    }) {
        generate_node_page(
            &nodes_dir,
            node,
            graph,
            &edge_map,
            &incoming_map,
            communities,
        )?;
    }

    // 5. Generate Process pages
    for node in graph.iter_nodes().filter(|n| n.label == NodeLabel::Process) {
        generate_process_page(&proc_dir, node, graph, &edge_map, communities)?;
    }

    Ok(vault_dir.to_string_lossy().to_string())
}

fn generate_index(
    dir: &Path,
    graph: &KnowledgeGraph,
    communities: &BTreeMap<String, CommunityInfo>,
) -> Result<()> {
    let out_path = dir.join("Index.md");
    let mut f = file_create_traced(&out_path)?;

    let project_name = dir
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Project".to_string());

    writeln!(f, "# 🧠 Cerveau Numérique - {}", project_name)?;
    writeln!(f)?;
    writeln!(f, "> Ce coffre-fort Obsidian contient une cartographie complète du code source, générée par Code Explorer.")?;
    writeln!(f)?;

    writeln!(f, "## 📊 Statistiques")?;

    let mut counts = HashMap::new();
    for node in graph.iter_nodes() {
        *counts.entry(node.label).or_insert(0) += 1;
    }

    writeln!(
        f,
        "- **Fichiers** : {}",
        counts.get(&NodeLabel::File).unwrap_or(&0)
    )?;
    writeln!(
        f,
        "- **Classes** : {}",
        counts.get(&NodeLabel::Class).unwrap_or(&0)
    )?;
    writeln!(
        f,
        "- **Fonctions/Méthodes** : {}",
        counts.get(&NodeLabel::Function).unwrap_or(&0)
    )?;
    writeln!(f, "- **Modules Fonctionnels** : {}", communities.len())?;
    writeln!(f)?;

    writeln!(f, "## 🧱 Modules Principaux")?;
    for (id, info) in communities {
        writeln!(f, "- {}", wikilink_to_community(id, info))?;
    }
    writeln!(f)?;

    writeln!(f, "## ⚙️ Processus Métier")?;
    for node in graph.iter_nodes().filter(|n| n.label == NodeLabel::Process) {
        writeln!(f, "- {}", wikilink_to_node(node, &node.properties.name))?;
    }
    writeln!(f)?;

    writeln!(f, "## 🔍 Vérification du Cerveau (Lint)")?;
    writeln!(f, "> État de santé de la base de connaissances.")?;
    writeln!(f)?;

    let mut edge_map: HashMap<String, Vec<(String, RelationshipType)>> = HashMap::new();
    let mut incoming_count: HashMap<String, usize> = HashMap::new();
    for rel in graph.iter_relationships() {
        edge_map
            .entry(rel.source_id.clone())
            .or_default()
            .push((rel.target_id.clone(), rel.rel_type));
        *incoming_count.entry(rel.target_id.clone()).or_insert(0) += 1;
    }

    let orphans: Vec<_> = graph
        .iter_nodes()
        .filter(|n| {
            n.label != NodeLabel::File
                && n.label != NodeLabel::Community
                && n.label != NodeLabel::Document
                && n.label != NodeLabel::DocChunk
        })
        .filter(|n| !edge_map.contains_key(&n.id) && !incoming_count.contains_key(&n.id))
        .take(5)
        .collect();

    if orphans.is_empty() {
        writeln!(
            f,
            "- ✅ Aucun symbole orphelin détecté. Le graphe est bien connecté."
        )?;
    } else {
        let example = wikilink_to_node(orphans[0], &orphans[0].properties.name);
        writeln!(
            f,
            "- ⚠️ {} symboles isolés détectés (ex: {})",
            orphans.len(),
            example
        )?;
    }

    writeln!(f)?;
    writeln!(f, "## 📑 Journal")?;
    writeln!(f, "- [[Log|Consulter le journal d'ingestion]]")?;
    writeln!(f)?;

    writeln!(f, "## 🤖 IA & Chat")?;
    writeln!(f, "Vous pouvez interroger ce cerveau numérique :")?;
    writeln!(
        f,
        "1. **Via CLI** : Lancez `code-explorer ask \"votre question\"`."
    )?;
    writeln!(f, "2. **Via Obsidian** : Utilisez les extensions *Claude Code* ou *Smart Connections* sur ce dossier.")?;
    writeln!(
        f,
        "3. **Via le Wiki HTML** : Lancez `code-explorer serve` et utilisez le widget de chat intégré."
    )?;

    Ok(())
}

fn generate_log(dir: &Path, graph: &KnowledgeGraph) -> Result<()> {
    let out_path = dir.join("Log.md");
    let mut f = file_create_traced(&out_path)?;

    writeln!(f, "# 📑 Journal d'Ingestion")?;
    writeln!(f)?;
    writeln!(f, "| Date | Événement | Détails |")?;
    writeln!(f, "|------|-----------|---------|")?;

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    writeln!(
        f,
        "| {} | Génération initiale | Export complet du graphe Code Explorer |",
        now
    )?;

    let files: Vec<_> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::File)
        .take(10)
        .collect();

    for node in files {
        writeln!(
            f,
            "| {} | Indexation | Fichier {} traité |",
            now,
            wikilink_to_node(node, &node.properties.name)
        )?;
    }

    Ok(())
}

fn generate_claudecode_instructions(dir: &Path) -> Result<()> {
    let out_path = dir.join(".claudecode");
    let mut f = file_create_traced(&out_path)?;

    writeln!(
        f,
        "You are an AI expert assistant working inside an Obsidian Vault generated by Code Explorer."
    )?;
    writeln!(
        f,
        "This vault represents the 'Digital Brain' of a software project."
    )?;
    writeln!(f)?;
    writeln!(f, "Structure:")?;
    writeln!(f, "- Modules/: High-level functional areas")?;
    writeln!(f, "- Processus/: Business processes and workflows")?;
    writeln!(
        f,
        "- Symboles/: Classes, Functions, Methods organized by type"
    )?;
    writeln!(f, "- Fichiers/: Source code files mapping")?;
    writeln!(f)?;
    writeln!(f, "Guidelines:")?;
    writeln!(
        f,
        "1. Always check 'Index.md' first to understand the project structure."
    )?;
    writeln!(
        f,
        "2. Use wikilinks [[Path/To/Note]] to navigate between entities."
    )?;
    writeln!(
        f,
        "3. When asked about a feature, look into 'Modules/' or 'Processus/'."
    )?;
    writeln!(
        f,
        "4. When asked about implementation, look into 'Symboles/'."
    )?;
    writeln!(f, "5. If you find a broken link or orphan info, notify the user so they can run 'code-explorer analyze'.")?;

    Ok(())
}

fn generate_node_page(
    dir: &Path,
    node: &GraphNode,
    graph: &KnowledgeGraph,
    edge_map: &HashMap<String, Vec<(String, RelationshipType)>>,
    incoming_map: &HashMap<String, Vec<(String, RelationshipType)>>,
    communities: &BTreeMap<String, CommunityInfo>,
) -> Result<()> {
    // Use a sanitized label string for the subdir so labels like
    // DocChunk / Document — if the upstream filter ever misses them —
    // still yield a valid path component.
    let label_dir = dir.join(sanitize_obsidian_filename(node.label.as_str()));
    if !label_dir.exists() {
        create_dir_all_traced(&label_dir)?;
    }

    let filename = format!("{}.md", node_note_stem(node));
    let out_path = label_dir.join(&filename);
    let mut f = file_create_traced(&out_path)?;

    // Frontmatter
    writeln!(f, "---")?;
    writeln!(f, "type: {:?}", node.label)?;
    writeln!(f, "name: \"{}\"", yaml_string(&node.properties.name))?;
    writeln!(f, "node_id: \"{}\"", yaml_string(&node.id))?;
    writeln!(f, "file: \"{}\"", yaml_string(&node.properties.file_path))?;
    if let Some(s) = node.properties.start_line {
        writeln!(f, "start_line: {s}")?;
    }
    writeln!(f, "---")?;
    writeln!(f)?;

    writeln!(f, "# {}", node.properties.name)?;
    writeln!(f)?;
    writeln!(f, "Type: **{:?}**", node.label)?;
    if let Some(file_link) = wikilink_to_file_path(graph, &node.properties.file_path) {
        writeln!(f, "Fichier: {file_link}")?;
    } else {
        writeln!(f, "Fichier: `{}`", node.properties.file_path)?;
    }
    writeln!(f)?;

    // Outgoing
    if let Some(edges) = edge_map.get(&node.id) {
        writeln!(f, "## 🔗 Dépendances (Sortant)")?;
        for (target_id, rel) in edges {
            if let Some(target) = graph.get_node(target_id) {
                let rel_str = format!("{:?}", rel);
                if let Some(link) = wikilink_for_target(target, communities) {
                    writeln!(f, "- {rel_str}: {link}")?;
                }
            }
        }
        writeln!(f)?;
    }

    // Incoming edges (O(1) lookup via pre-built reverse map)
    writeln!(f, "## 📥 Utilisé par (Entrant)")?;
    if let Some(incoming) = incoming_map.get(&node.id) {
        for (src_id, rel) in incoming {
            if let Some(src) = graph.get_node(src_id) {
                if let Some(link) = wikilink_for_target(src, communities) {
                    writeln!(f, "- {link} ({rel:?})")?;
                }
            }
        }
    }

    Ok(())
}

fn generate_file_page(
    dir: &Path,
    node: &GraphNode,
    graph: &KnowledgeGraph,
    edge_map: &HashMap<String, Vec<(String, RelationshipType)>>,
    communities: &BTreeMap<String, CommunityInfo>,
) -> Result<()> {
    let filename = format!("{}.md", node_note_stem(node));
    let out_path = dir.join(&filename);
    let mut f = file_create_traced(&out_path)?;

    writeln!(f, "---")?;
    writeln!(f, "type: File")?;
    writeln!(f, "node_id: \"{}\"", yaml_string(&node.id))?;
    writeln!(f, "path: \"{}\"", yaml_string(&node.properties.file_path))?;
    if let Some(l) = &node.properties.language {
        writeln!(f, "language: {:?}", l)?;
    }
    writeln!(f, "---")?;
    writeln!(f)?;

    writeln!(f, "# Fichier: {}", node.properties.name)?;
    writeln!(f)?;

    writeln!(f, "## 🧩 Contenu")?;
    if let Some(edges) = edge_map.get(&node.id) {
        for (target_id, rel) in edges {
            if *rel == RelationshipType::Defines {
                if let Some(target) = graph.get_node(target_id) {
                    if let Some(link) = wikilink_for_target(target, communities) {
                        writeln!(f, "- {link}")?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn generate_community_page(
    dir: &Path,
    id: &str,
    info: &CommunityInfo,
    graph: &KnowledgeGraph,
    _edge_map: &HashMap<String, Vec<(String, RelationshipType)>>,
    communities: &BTreeMap<String, CommunityInfo>,
) -> Result<()> {
    let filename = format!("{}.md", unique_note_stem(&info.label, id));
    let out_path = dir.join(&filename);
    let mut f = file_create_traced(&out_path)?;

    writeln!(f, "---")?;
    writeln!(f, "type: Module")?;
    writeln!(f, "community_id: \"{}\"", yaml_string(id))?;
    writeln!(f, "name: \"{}\"", yaml_string(&info.label))?;
    writeln!(f, "---")?;
    writeln!(f)?;

    writeln!(f, "# Module: {}", info.label)?;
    writeln!(f)?;
    if let Some(d) = &info.description {
        writeln!(f, "{d}")?;
        writeln!(f)?;
    }

    writeln!(f, "## 👥 Membres")?;
    for mid in &info.member_ids {
        if let Some(m) = graph.get_node(mid) {
            if let Some(link) = wikilink_for_target(m, communities) {
                writeln!(f, "- {link}")?;
            }
        }
    }

    Ok(())
}

fn generate_process_page(
    dir: &Path,
    node: &GraphNode,
    graph: &KnowledgeGraph,
    _edge_map: &HashMap<String, Vec<(String, RelationshipType)>>,
    communities: &BTreeMap<String, CommunityInfo>,
) -> Result<()> {
    let filename = format!("{}.md", node_note_stem(node));
    let out_path = dir.join(&filename);
    let mut f = file_create_traced(&out_path)?;

    writeln!(f, "---")?;
    writeln!(f, "type: Process")?;
    writeln!(f, "node_id: \"{}\"", yaml_string(&node.id))?;
    writeln!(f, "name: \"{}\"", yaml_string(&node.properties.name))?;
    writeln!(f, "---")?;
    writeln!(f)?;

    writeln!(f, "# Processus: {}", node.properties.name)?;
    writeln!(f)?;

    if let Some(desc) = &node.properties.description {
        writeln!(f, "{desc}")?;
        writeln!(f)?;
    }

    // Sequence/Flow
    if let Some(entry_id) = &node.properties.entry_point_id {
        if let Some(entry) = graph.get_node(entry_id) {
            if let Some(link) = wikilink_for_target(entry, communities) {
                writeln!(f, "Point d'entrée: {link}")?;
            }
        } else {
            writeln!(f, "Point d'entrée: `{entry_id}`")?;
        }
    }

    Ok(())
}

/// Maximum filename length we'll emit. Windows MAX_PATH is 260 chars
/// including the full parent path, so we leave generous headroom.
const MAX_FILENAME_LEN: usize = 200;

/// Turn an arbitrary node/module/process name into a filename that is
/// safe on Windows NTFS, macOS HFS+/APFS, and Linux ext4, AND still
/// reasonably readable for a human browsing an Obsidian vault.
///
/// Rules applied in order:
/// 1. Drop ASCII control characters (`\x00`–`\x1F`, `\x7F`) — Windows
///    refuses them as filename characters and most of them have no
///    visual representation anyway.
/// 2. Replace Windows reserved characters (`< > : " / \ | ? *`) and
///    the Obsidian-problematic ones (`# ^ [ ]`) with `_`.
/// 3. Collapse runs of `_` into a single `_` so a name full of bad
///    chars doesn't become a wall of underscores.
/// 4. Trim leading/trailing whitespace, dots, and underscores. Windows
///    rejects filenames ending in a dot or a space; dots at the start
///    create hidden files on Unix; underscores at the edges are just
///    ugly.
/// 5. If the result is empty (input was entirely garbage), return
///    `_unnamed` so we never produce an empty filename.
/// 6. If the result matches a Windows reserved device name
///    (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`,
///    case-insensitive), prepend `_` to disarm it.
/// 7. Truncate to `MAX_FILENAME_LEN` bytes so we never overshoot
///    Windows' 260-char MAX_PATH when joined with a parent directory.
fn sanitize_obsidian_filename(name: &str) -> String {
    // Step 1 & 2: char-by-char rewrite.
    let mut out = String::with_capacity(name.len());
    let mut last_was_us = false;
    for c in name.chars() {
        let replacement = if (c as u32) < 0x20 || c == '\x7f' {
            // Drop control chars entirely.
            None
        } else if matches!(
            c,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' | '#' | '^' | '[' | ']'
        ) {
            Some('_')
        } else {
            Some(c)
        };
        if let Some(ch) = replacement {
            // Step 3: collapse consecutive underscores.
            if ch == '_' {
                if last_was_us {
                    continue;
                }
                last_was_us = true;
            } else {
                last_was_us = false;
            }
            out.push(ch);
        }
    }

    // Step 4: trim whitespace, dots, and underscores from both ends.
    let trimmed = out.trim_matches(|c: char| c.is_whitespace() || c == '.' || c == '_');
    let mut out = trimmed.to_string();

    // Step 5: never return empty.
    if out.is_empty() {
        return "_unnamed".to_string();
    }

    // Step 6: disarm Windows reserved device names.
    if is_windows_reserved(&out) {
        out = format!("_{out}");
    }

    // Step 7: truncate if still too long. Use `char_indices` to avoid
    // slicing in the middle of a multi-byte UTF-8 sequence.
    if out.len() > MAX_FILENAME_LEN {
        let cut = out
            .char_indices()
            .take_while(|(i, _)| *i <= MAX_FILENAME_LEN)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        out.truncate(cut);
        // Re-trim in case the cut landed on a dot or space.
        out = out
            .trim_end_matches(|c: char| c.is_whitespace() || c == '.' || c == '_')
            .to_string();
        if out.is_empty() {
            out = "_unnamed".to_string();
        }
    }

    out
}

fn is_windows_reserved(name: &str) -> bool {
    // Reserved device names on Windows, case-insensitive, with or
    // without extension. We match only the stem.
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::NodeProperties;

    fn test_node(id: &str, label: NodeLabel, name: &str, file_path: &str) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label,
            properties: NodeProperties {
                name: name.to_string(),
                file_path: file_path.to_string(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn unique_note_stems_disambiguate_duplicate_names() {
        let first = unique_note_stem("run", "Function:src/a.rs:run");
        let second = unique_note_stem("run", "Function:src/b.rs:run");

        assert_ne!(first, second);
        assert!(first.starts_with("run--"));
        assert!(second.starts_with("run--"));
        assert!(first.len() <= MAX_FILENAME_LEN);
        assert!(second.len() <= MAX_FILENAME_LEN);
    }

    #[test]
    fn node_note_paths_use_stable_ids_and_file_paths() {
        let file = test_node(
            "File:crates/code-explorer-cli/src/main.rs:main.rs",
            NodeLabel::File,
            "main.rs",
            "crates/code-explorer-cli/src/main.rs",
        );
        let method = test_node(
            "Method:src/foo.rs:run",
            NodeLabel::Method,
            "run",
            "src/foo.rs",
        );

        assert!(node_note_path(&file).starts_with("Fichiers/crates_code-explorer-cli_src_main.rs--"));
        assert!(node_note_path(&method).starts_with("Symboles/Method/run--"));
    }

    #[test]
    fn community_note_paths_do_not_collapse_same_labels() {
        let first = CommunityInfo {
            label: "commands".to_string(),
            description: None,
            member_ids: Vec::new(),
        };
        let second = CommunityInfo {
            label: "commands".to_string(),
            description: None,
            member_ids: Vec::new(),
        };

        assert_ne!(
            community_note_path("Community:one", &first),
            community_note_path("Community:two", &second)
        );
    }

    #[test]
    fn basic_chars_preserved() {
        assert_eq!(sanitize_obsidian_filename("HelloWorld"), "HelloWorld");
        assert_eq!(sanitize_obsidian_filename("Hello World"), "Hello World");
        assert_eq!(sanitize_obsidian_filename("file-name_42"), "file-name_42");
    }

    #[test]
    fn generics_get_underscored() {
        assert_eq!(sanitize_obsidian_filename("List<T>"), "List_T");
        assert_eq!(
            sanitize_obsidian_filename("Dictionary<K, V>"),
            "Dictionary_K, V"
        );
    }

    #[test]
    fn windows_reserved_chars_replaced() {
        assert_eq!(
            sanitize_obsidian_filename("foo/bar\\baz:qux"),
            "foo_bar_baz_qux"
        );
        assert_eq!(sanitize_obsidian_filename("a|b*c?d\"e"), "a_b_c_d_e");
    }

    #[test]
    fn obsidian_special_chars_replaced() {
        assert_eq!(sanitize_obsidian_filename("#header"), "header");
        assert_eq!(sanitize_obsidian_filename("foo^bar"), "foo_bar");
        assert_eq!(sanitize_obsidian_filename("[link]"), "link");
    }

    #[test]
    fn internal_dots_preserved_edges_trimmed() {
        // Internal dots are fine — keeps the .cs / .docx suffix visible.
        assert_eq!(
            sanitize_obsidian_filename("file.name.ext.md"),
            "file.name.ext.md"
        );
        // Leading dots are trimmed (Unix hidden files + Windows quirks).
        assert_eq!(sanitize_obsidian_filename(".hidden"), "hidden");
        // Trailing dots / spaces are trimmed (Windows rejects them).
        assert_eq!(sanitize_obsidian_filename("trailing. "), "trailing");
        assert_eq!(sanitize_obsidian_filename("weird... "), "weird");
    }

    #[test]
    fn empty_and_whitespace_only() {
        assert_eq!(sanitize_obsidian_filename(""), "_unnamed");
        assert_eq!(sanitize_obsidian_filename("   "), "_unnamed");
        assert_eq!(sanitize_obsidian_filename("\t\n  \r"), "_unnamed");
        // Only reserved chars collapse to one `_` which then gets trimmed.
        assert_eq!(sanitize_obsidian_filename("<>:|"), "_unnamed");
    }

    #[test]
    fn consecutive_underscores_collapse() {
        assert_eq!(sanitize_obsidian_filename("a<<>>b"), "a_b");
        assert_eq!(sanitize_obsidian_filename("___test___"), "test");
    }

    #[test]
    fn control_chars_dropped() {
        assert_eq!(sanitize_obsidian_filename("foo\x00bar"), "foobar");
        assert_eq!(sanitize_obsidian_filename("foo\x1fbar"), "foobar");
        assert_eq!(sanitize_obsidian_filename("foo\x7fbar"), "foobar");
        // Real Windows \r\n also goes.
        assert_eq!(sanitize_obsidian_filename("foo\r\nbar"), "foobar");
    }

    #[test]
    fn windows_reserved_names_disarmed() {
        assert_eq!(sanitize_obsidian_filename("CON"), "_CON");
        assert_eq!(sanitize_obsidian_filename("prn"), "_prn");
        assert_eq!(sanitize_obsidian_filename("AUX.txt"), "_AUX.txt");
        assert_eq!(sanitize_obsidian_filename("COM1"), "_COM1");
        assert_eq!(sanitize_obsidian_filename("LPT9"), "_LPT9");
        // Not reserved — keep as-is.
        assert_eq!(sanitize_obsidian_filename("COMMON"), "COMMON");
        assert_eq!(sanitize_obsidian_filename("LAPTOP"), "LAPTOP");
    }

    #[test]
    fn long_names_truncated() {
        let input = "a".repeat(300);
        let out = sanitize_obsidian_filename(&input);
        assert!(
            out.len() <= MAX_FILENAME_LEN,
            "truncation failed: len={}",
            out.len()
        );
        assert!(out.chars().all(|c| c == 'a'));
    }

    #[test]
    fn real_alise_names_survive() {
        // Sample names harvested from the sample-app graph during the
        // bug-hunt session that motivated this rewrite.
        let samples = [
            "Chunk 26 of Domaines d'application  Sample v2 (54)/ALISEV2-JOURNAL APPLICATIF WINDOWS (QUALIF).docx",
            "SFD_Mutli barèmes groupe aides ASS.v0.2.4.C1.docx",
            "Acme.Sample.ihm/Views/Home/Index.cshtml",
            "IDictionary<string, object>",
            "Task<IActionResult>",
        ];
        for s in samples {
            let out = sanitize_obsidian_filename(s);
            // Must be non-empty
            assert!(!out.is_empty(), "empty result for {s:?}");
            // Must not contain any Windows-reserved chars
            for bad in ['<', '>', ':', '"', '/', '\\', '|', '?', '*'] {
                assert!(
                    !out.contains(bad),
                    "output {out:?} still contains {bad:?} (from input {s:?})"
                );
            }
            // Must not contain control chars
            assert!(
                !out.chars().any(|c| (c as u32) < 0x20 || c == '\x7f'),
                "output {out:?} still contains control chars (from input {s:?})"
            );
            // Must not start or end with a dot or space.
            assert!(!out.starts_with('.'), "output {out:?} starts with a dot");
            assert!(!out.ends_with('.'), "output {out:?} ends with a dot");
            assert!(!out.starts_with(' '), "output {out:?} starts with space");
            assert!(!out.ends_with(' '), "output {out:?} ends with space");
            // Must fit.
            assert!(out.len() <= MAX_FILENAME_LEN);
        }
    }

    #[cfg(windows)]
    #[test]
    fn strip_verbatim_prefix_handles_both_forms() {
        use std::path::PathBuf;
        // Plain extended path → prefix stripped.
        let p = PathBuf::from(r"\\?\D:\taf\sample-app\.codeexplorer\docs");
        assert_eq!(
            strip_verbatim_prefix(&p).to_string_lossy(),
            r"D:\taf\sample-app\.codeexplorer\docs"
        );
        // UNC extended path → kept as-is (it's actually needed).
        let p = PathBuf::from(r"\\?\UNC\server\share\foo");
        assert_eq!(
            strip_verbatim_prefix(&p),
            PathBuf::from(r"\\?\UNC\server\share\foo")
        );
        // Plain path → untouched.
        let p = PathBuf::from(r"D:\normal\path");
        assert_eq!(strip_verbatim_prefix(&p), PathBuf::from(r"D:\normal\path"));
    }
}
