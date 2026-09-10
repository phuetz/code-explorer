//! TUI Dashboard for exploring the Code Explorer knowledge graph.
//!
//! Three-panel layout: File Tree | Symbols | Details
//! Built with ratatui + crossterm.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};

use code_explorer_core::graph::types::{NodeLabel, RelationshipType};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::storage::repo_manager;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivePanel {
    FileTree,
    Symbols,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Files,
    Communities,
    Processes,
}

#[derive(Debug, Clone)]
struct TreeItem {
    name: String,
    path: String,
    is_dir: bool,
    depth: usize,
    expanded: bool,
    children_count: usize,
}

#[derive(Debug, Clone)]
struct SymbolItem {
    name: String,
    label: NodeLabel,
    file: String,
    lines: String,
    id: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CommunityItem {
    name: String,
    id: String,
    member_count: u32,
    cohesion: Option<f64>,
    description: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ProcessItem {
    name: String,
    id: String,
    step_count: u32,
    description: Option<String>,
}

// ─── App State ──────────────────────────────────────────────────────────────

struct App {
    graph: Arc<KnowledgeGraph>,
    repo_name: String,

    active_panel: ActivePanel,
    view_mode: ViewMode,

    // Pre-built relationship indexes
    #[allow(clippy::type_complexity)]
    outgoing: HashMap<String, Vec<(String, RelationshipType, f64, Option<u32>)>>,
    #[allow(clippy::type_complexity)]
    incoming: HashMap<String, Vec<(String, RelationshipType, f64, Option<u32>)>>,

    // File tree
    file_tree: Vec<TreeItem>,
    file_selected: usize,
    file_scroll_offset: usize,

    // Symbols (center panel in Files mode)
    symbols: Vec<SymbolItem>,
    symbol_selected: usize,
    symbol_scroll_offset: usize,

    // Communities (center panel in Communities mode)
    communities: Vec<CommunityItem>,
    community_selected: usize,
    community_scroll_offset: usize,
    community_members: Vec<SymbolItem>,
    community_member_selected: usize,
    community_member_scroll_offset: usize,
    viewing_community_members: bool,

    // Processes (center panel in Processes mode)
    processes: Vec<ProcessItem>,
    process_selected: usize,
    process_scroll_offset: usize,
    process_steps: Vec<SymbolItem>,
    process_step_selected: usize,
    process_step_scroll_offset: usize,
    viewing_process_steps: bool,

    // Details (right panel)
    callers: Vec<String>,
    callees: Vec<String>,
    community_label: Option<String>,
    detail_file: String,
    detail_lines: String,
    detail_extra: Vec<(String, String)>,
    detail_scroll_offset: usize,

    // Search
    search_mode: bool,
    search_query: String,
    all_symbols: Vec<SymbolItem>,

    should_quit: bool,
}

impl App {
    fn new(graph: KnowledgeGraph, repo_name: String) -> Self {
        let graph = Arc::new(graph);

        // Build relationship indexes
        #[allow(clippy::type_complexity)]
        let mut outgoing: HashMap<
            String,
            Vec<(String, RelationshipType, f64, Option<u32>)>,
        > = HashMap::new();
        #[allow(clippy::type_complexity)]
        let mut incoming: HashMap<
            String,
            Vec<(String, RelationshipType, f64, Option<u32>)>,
        > = HashMap::new();

        for rel in graph.iter_relationships() {
            outgoing.entry(rel.source_id.clone()).or_default().push((
                rel.target_id.clone(),
                rel.rel_type,
                rel.confidence,
                rel.step,
            ));
            incoming.entry(rel.target_id.clone()).or_default().push((
                rel.source_id.clone(),
                rel.rel_type,
                rel.confidence,
                rel.step,
            ));
        }

        // Build file tree from graph nodes
        let file_tree = build_file_tree(&graph);

        // Collect all symbols
        let mut all_symbols: Vec<SymbolItem> = Vec::new();
        for node in graph.iter_nodes() {
            if is_symbol_label(node.label) {
                let lines = match (node.properties.start_line, node.properties.end_line) {
                    (Some(s), Some(e)) => format!("{s}-{e}"),
                    (Some(s), None) => format!("{s}"),
                    _ => String::new(),
                };
                all_symbols.push(SymbolItem {
                    name: node.properties.name.clone(),
                    label: node.label,
                    file: node.properties.file_path.clone(),
                    lines,
                    id: node.id.clone(),
                });
            }
        }
        all_symbols.sort_by_key(|a| a.name.to_lowercase());

        // Build communities list
        let mut communities: Vec<CommunityItem> = Vec::new();
        for node in graph.iter_nodes() {
            if node.label == NodeLabel::Community {
                communities.push(CommunityItem {
                    name: node
                        .properties
                        .heuristic_label
                        .clone()
                        .or_else(|| Some(node.properties.name.clone()))
                        .unwrap_or_else(|| node.id.clone()),
                    id: node.id.clone(),
                    member_count: node.properties.symbol_count.unwrap_or(0),
                    cohesion: node.properties.cohesion,
                    description: node.properties.description.clone(),
                });
            }
        }
        communities.sort_by(|a, b| b.member_count.cmp(&a.member_count));

        // Build processes list
        let mut processes: Vec<ProcessItem> = Vec::new();
        for node in graph.iter_nodes() {
            if node.label == NodeLabel::Process {
                processes.push(ProcessItem {
                    name: node.properties.name.clone(),
                    id: node.id.clone(),
                    step_count: node.properties.step_count.unwrap_or(0),
                    description: node.properties.description.clone(),
                });
            }
        }
        processes.sort_by(|a, b| b.step_count.cmp(&a.step_count));

        let mut app = App {
            graph,
            repo_name,
            active_panel: ActivePanel::FileTree,
            view_mode: ViewMode::Files,
            outgoing,
            incoming,
            file_tree,
            file_selected: 0,
            file_scroll_offset: 0,
            symbols: Vec::new(),
            symbol_selected: 0,
            symbol_scroll_offset: 0,
            communities,
            community_selected: 0,
            community_scroll_offset: 0,
            community_members: Vec::new(),
            community_member_selected: 0,
            community_member_scroll_offset: 0,
            viewing_community_members: false,
            processes,
            process_selected: 0,
            process_scroll_offset: 0,
            process_steps: Vec::new(),
            process_step_selected: 0,
            process_step_scroll_offset: 0,
            viewing_process_steps: false,
            callers: Vec::new(),
            callees: Vec::new(),
            community_label: None,
            detail_file: String::new(),
            detail_lines: String::new(),
            detail_extra: Vec::new(),
            detail_scroll_offset: 0,
            search_mode: false,
            search_query: String::new(),
            all_symbols,
            should_quit: false,
        };

        // Load initial symbols if there is a file selected
        app.load_symbols_for_selected_file();
        app
    }

    // ─── File tree actions ──────────────────────────────────────────────

    fn toggle_expand(&mut self) {
        if self.file_selected >= self.file_tree.len() {
            return;
        }
        let item = &self.file_tree[self.file_selected];
        if !item.is_dir {
            // It's a file, load its symbols
            self.load_symbols_for_selected_file();
            self.active_panel = ActivePanel::Symbols;
            return;
        }
        let path = item.path.clone();
        let was_expanded = item.expanded;
        let depth = item.depth;

        if was_expanded {
            // Collapse: remove children
            self.file_tree[self.file_selected].expanded = false;
            let mut i = self.file_selected + 1;
            while i < self.file_tree.len() && self.file_tree[i].depth > depth {
                i += 1;
            }
            let remove_count = i - self.file_selected - 1;
            for _ in 0..remove_count {
                self.file_tree.remove(self.file_selected + 1);
            }
        } else {
            // Expand: insert children
            self.file_tree[self.file_selected].expanded = true;
            let children = get_children_for_path(&self.graph, &path, depth + 1);
            for (j, child) in children.into_iter().enumerate() {
                self.file_tree.insert(self.file_selected + 1 + j, child);
            }
        }
    }

    fn load_symbols_for_selected_file(&mut self) {
        self.symbols.clear();
        self.symbol_selected = 0;
        self.symbol_scroll_offset = 0;

        if self.file_selected >= self.file_tree.len() {
            return;
        }

        let item = &self.file_tree[self.file_selected];
        let path = &item.path;

        if item.is_dir {
            // Match the directory boundary explicitly. A bare `starts_with`
            // would let `path = "src/foo"` also pull in symbols from
            // `src/foobar/...` because "src/foobar/..." literally starts with
            // "src/foo". Append a trailing `/` so we only accept files that
            // are direct or nested children of this exact directory.
            // Same substring-vs-segment pattern as the cross_ref / functional
            // generator fixes earlier in this audit.
            let dir_prefix = if path.ends_with('/') {
                path.clone()
            } else {
                format!("{}/", path)
            };
            for node in self.graph.iter_nodes() {
                if is_symbol_label(node.label) && node.properties.file_path.starts_with(&dir_prefix)
                {
                    let lines = match (node.properties.start_line, node.properties.end_line) {
                        (Some(s), Some(e)) => format!("{s}-{e}"),
                        (Some(s), None) => format!("{s}"),
                        _ => String::new(),
                    };
                    self.symbols.push(SymbolItem {
                        name: node.properties.name.clone(),
                        label: node.label,
                        file: node.properties.file_path.clone(),
                        lines,
                        id: node.id.clone(),
                    });
                }
            }
        } else {
            // Load symbols for this specific file
            if let Some(ids) = self.graph.nodes_by_file(path) {
                for id in ids {
                    if let Some(node) = self.graph.get_node(id) {
                        if is_symbol_label(node.label) {
                            let lines = match (node.properties.start_line, node.properties.end_line)
                            {
                                (Some(s), Some(e)) => format!("{s}-{e}"),
                                (Some(s), None) => format!("{s}"),
                                _ => String::new(),
                            };
                            self.symbols.push(SymbolItem {
                                name: node.properties.name.clone(),
                                label: node.label,
                                file: node.properties.file_path.clone(),
                                lines,
                                id: node.id.clone(),
                            });
                        }
                    }
                }
            }
        }

        self.symbols
            .sort_by_key(|a| a.name.to_lowercase());
        self.load_details_for_selected_symbol();
    }

    fn load_details_for_selected_symbol(&mut self) {
        self.callers.clear();
        self.callees.clear();
        self.community_label = None;
        self.detail_file.clear();
        self.detail_lines.clear();
        self.detail_extra.clear();
        self.detail_scroll_offset = 0;

        let symbol = match self.current_selected_symbol() {
            Some(s) => s.clone(),
            None => return,
        };

        self.detail_file = symbol.file.clone();
        self.detail_lines = symbol.lines.clone();

        // Load callers (incoming CALLS)
        if let Some(rels) = self.incoming.get(&symbol.id) {
            for (src_id, rel_type, _conf, _step) in rels {
                if *rel_type == RelationshipType::Calls {
                    if let Some(node) = self.graph.get_node(src_id) {
                        self.callers.push(format!(
                            "{} [{}]",
                            node.properties.name,
                            label_badge(node.label)
                        ));
                    }
                }
            }
        }

        // Load callees (outgoing CALLS)
        if let Some(rels) = self.outgoing.get(&symbol.id) {
            for (tgt_id, rel_type, _conf, _step) in rels {
                if *rel_type == RelationshipType::Calls {
                    if let Some(node) = self.graph.get_node(tgt_id) {
                        self.callees.push(format!(
                            "{} [{}]",
                            node.properties.name,
                            label_badge(node.label)
                        ));
                    }
                }
            }
        }

        // Load community membership (outgoing MEMBER_OF)
        if let Some(rels) = self.outgoing.get(&symbol.id) {
            for (tgt_id, rel_type, _conf, _step) in rels {
                if *rel_type == RelationshipType::MemberOf {
                    if let Some(node) = self.graph.get_node(tgt_id) {
                        let label = node
                            .properties
                            .heuristic_label
                            .clone()
                            .unwrap_or_else(|| node.properties.name.clone());
                        let count = node.properties.symbol_count.unwrap_or(0);
                        self.community_label = Some(format!("{label} ({count})"));
                    }
                }
            }
        }

        // Additional edges
        if let Some(rels) = self.outgoing.get(&symbol.id) {
            for (tgt_id, rel_type, _conf, _step) in rels {
                match rel_type {
                    RelationshipType::Imports
                    | RelationshipType::Extends
                    | RelationshipType::Implements
                    | RelationshipType::Uses => {
                        if let Some(node) = self.graph.get_node(tgt_id) {
                            self.detail_extra.push((
                                rel_type.as_str().to_string(),
                                node.properties.name.clone(),
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn current_selected_symbol(&self) -> Option<&SymbolItem> {
        match self.view_mode {
            ViewMode::Files => {
                if self.search_mode && !self.search_query.is_empty() {
                    let filtered = self.filtered_symbols();
                    if self.symbol_selected < filtered.len() {
                        // Return from all_symbols at the right index
                        return Some(filtered[self.symbol_selected]);
                    }
                    None
                } else {
                    self.symbols.get(self.symbol_selected)
                }
            }
            ViewMode::Communities => {
                if self.viewing_community_members {
                    self.community_members.get(self.community_member_selected)
                } else {
                    None
                }
            }
            ViewMode::Processes => {
                if self.viewing_process_steps {
                    self.process_steps.get(self.process_step_selected)
                } else {
                    None
                }
            }
        }
    }

    fn filtered_symbols(&self) -> Vec<&SymbolItem> {
        if self.search_query.is_empty() {
            return self.all_symbols.iter().collect();
        }
        let query = self.search_query.to_lowercase();
        self.all_symbols
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&query))
            .collect()
    }

    fn load_community_members(&mut self) {
        self.community_members.clear();
        self.community_member_selected = 0;
        self.community_member_scroll_offset = 0;

        if self.community_selected >= self.communities.len() {
            return;
        }
        let community_id = self.communities[self.community_selected].id.clone();

        // Find all nodes that have MEMBER_OF -> this community
        if let Some(rels) = self.incoming.get(&community_id) {
            for (src_id, rel_type, _conf, _step) in rels {
                if *rel_type == RelationshipType::MemberOf {
                    if let Some(node) = self.graph.get_node(src_id) {
                        if is_symbol_label(node.label) {
                            let lines = match (node.properties.start_line, node.properties.end_line)
                            {
                                (Some(s), Some(e)) => format!("{s}-{e}"),
                                (Some(s), None) => format!("{s}"),
                                _ => String::new(),
                            };
                            self.community_members.push(SymbolItem {
                                name: node.properties.name.clone(),
                                label: node.label,
                                file: node.properties.file_path.clone(),
                                lines,
                                id: node.id.clone(),
                            });
                        }
                    }
                }
            }
        }
        self.community_members
            .sort_by_key(|a| a.name.to_lowercase());
    }

    fn load_process_steps(&mut self) {
        self.process_steps.clear();
        self.process_step_selected = 0;
        self.process_step_scroll_offset = 0;

        if self.process_selected >= self.processes.len() {
            return;
        }
        let process_id = self.processes[self.process_selected].id.clone();

        // Find all nodes that have STEP_IN_PROCESS -> this process
        let mut steps: Vec<(u32, SymbolItem)> = Vec::new();
        if let Some(rels) = self.incoming.get(&process_id) {
            for (src_id, rel_type, _conf, step) in rels {
                if *rel_type == RelationshipType::StepInProcess {
                    if let Some(node) = self.graph.get_node(src_id) {
                        let lines = match (node.properties.start_line, node.properties.end_line) {
                            (Some(s), Some(e)) => format!("{s}-{e}"),
                            (Some(s), None) => format!("{s}"),
                            _ => String::new(),
                        };
                        steps.push((
                            step.unwrap_or(0),
                            SymbolItem {
                                name: node.properties.name.clone(),
                                label: node.label,
                                file: node.properties.file_path.clone(),
                                lines,
                                id: node.id.clone(),
                            },
                        ));
                    }
                }
            }
        }
        steps.sort_by_key(|(step, _)| *step);
        self.process_steps = steps.into_iter().map(|(_, s)| s).collect();
    }

    // ─── Navigation helpers ─────────────────────────────────────────────

    fn move_up(&mut self) {
        match self.active_panel {
            ActivePanel::FileTree => {
                if self.file_selected > 0 {
                    self.file_selected -= 1;
                    self.load_symbols_for_selected_file();
                }
            }
            ActivePanel::Symbols => match self.view_mode {
                ViewMode::Files => {
                    if self.search_mode {
                        if self.symbol_selected > 0 {
                            self.symbol_selected -= 1;
                            self.load_details_for_selected_symbol();
                        }
                    } else if self.symbol_selected > 0 {
                        self.symbol_selected -= 1;
                        self.load_details_for_selected_symbol();
                    }
                }
                ViewMode::Communities => {
                    if self.viewing_community_members {
                        if self.community_member_selected > 0 {
                            self.community_member_selected -= 1;
                            self.load_details_for_selected_symbol();
                        }
                    } else if self.community_selected > 0 {
                        self.community_selected -= 1;
                    }
                }
                ViewMode::Processes => {
                    if self.viewing_process_steps {
                        if self.process_step_selected > 0 {
                            self.process_step_selected -= 1;
                            self.load_details_for_selected_symbol();
                        }
                    } else if self.process_selected > 0 {
                        self.process_selected -= 1;
                    }
                }
            },
            ActivePanel::Details => {
                if self.detail_scroll_offset > 0 {
                    self.detail_scroll_offset -= 1;
                }
            }
        }
    }

    fn move_down(&mut self) {
        match self.active_panel {
            ActivePanel::FileTree => {
                if self.file_selected + 1 < self.file_tree.len() {
                    self.file_selected += 1;
                    self.load_symbols_for_selected_file();
                }
            }
            ActivePanel::Symbols => match self.view_mode {
                ViewMode::Files => {
                    if self.search_mode {
                        let count = self.filtered_symbols().len();
                        if self.symbol_selected + 1 < count {
                            self.symbol_selected += 1;
                            self.load_details_for_selected_symbol();
                        }
                    } else if self.symbol_selected + 1 < self.symbols.len() {
                        self.symbol_selected += 1;
                        self.load_details_for_selected_symbol();
                    }
                }
                ViewMode::Communities => {
                    if self.viewing_community_members {
                        if self.community_member_selected + 1 < self.community_members.len() {
                            self.community_member_selected += 1;
                            self.load_details_for_selected_symbol();
                        }
                    } else if self.community_selected + 1 < self.communities.len() {
                        self.community_selected += 1;
                    }
                }
                ViewMode::Processes => {
                    if self.viewing_process_steps {
                        if self.process_step_selected + 1 < self.process_steps.len() {
                            self.process_step_selected += 1;
                            self.load_details_for_selected_symbol();
                        }
                    } else if self.process_selected + 1 < self.processes.len() {
                        self.process_selected += 1;
                    }
                }
            },
            ActivePanel::Details => {
                self.detail_scroll_offset += 1;
            }
        }
    }

    fn handle_enter(&mut self) {
        match self.active_panel {
            ActivePanel::FileTree => {
                self.toggle_expand();
            }
            ActivePanel::Symbols => match self.view_mode {
                ViewMode::Communities if !self.viewing_community_members => {
                    self.load_community_members();
                    self.viewing_community_members = true;
                    self.load_details_for_selected_symbol();
                }
                ViewMode::Processes if !self.viewing_process_steps => {
                    self.load_process_steps();
                    self.viewing_process_steps = true;
                    self.load_details_for_selected_symbol();
                }
                _ => {
                    // Switch to details panel
                    self.active_panel = ActivePanel::Details;
                }
            },
            ActivePanel::Details => {}
        }
    }

    fn handle_escape(&mut self) {
        if self.search_mode {
            self.search_mode = false;
            self.search_query.clear();
            self.symbol_selected = 0;
            self.symbol_scroll_offset = 0;
            return;
        }
        match self.view_mode {
            ViewMode::Communities if self.viewing_community_members => {
                self.viewing_community_members = false;
                self.community_members.clear();
            }
            ViewMode::Processes if self.viewing_process_steps => {
                self.viewing_process_steps = false;
                self.process_steps.clear();
            }
            _ => {
                if self.active_panel != ActivePanel::FileTree {
                    self.active_panel = ActivePanel::FileTree;
                }
            }
        }
    }

    fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::FileTree => ActivePanel::Symbols,
            ActivePanel::Symbols => ActivePanel::Details,
            ActivePanel::Details => ActivePanel::FileTree,
        };
    }

    fn prev_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::FileTree => ActivePanel::Details,
            ActivePanel::Symbols => ActivePanel::FileTree,
            ActivePanel::Details => ActivePanel::Symbols,
        };
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Search mode: capture text input
        if self.search_mode {
            match key.code {
                KeyCode::Esc => self.handle_escape(),
                KeyCode::Enter => {
                    // Confirm search, stay on filtered results
                    self.search_mode = false;
                    let filtered = self.filtered_symbols();
                    if let Some(sym) = filtered.get(self.symbol_selected) {
                        // Copy into symbols for detail loading
                        let sym = (*sym).clone();
                        self.symbols = vec![sym];
                        self.symbol_selected = 0;
                        self.load_details_for_selected_symbol();
                    }
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.symbol_selected = 0;
                    self.symbol_scroll_offset = 0;
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.symbol_selected = 0;
                    self.symbol_scroll_offset = 0;
                }
                KeyCode::Up => self.move_up(),
                KeyCode::Down => self.move_down(),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.prev_panel();
                } else {
                    self.next_panel();
                }
            }
            KeyCode::Char('1') => self.active_panel = ActivePanel::FileTree,
            KeyCode::Char('2') => self.active_panel = ActivePanel::Symbols,
            KeyCode::Char('3') => self.active_panel = ActivePanel::Details,
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Esc => self.handle_escape(),
            KeyCode::Char('/') => {
                self.search_mode = true;
                self.search_query.clear();
                self.symbol_selected = 0;
                self.symbol_scroll_offset = 0;
                self.active_panel = ActivePanel::Symbols;
                self.view_mode = ViewMode::Files;
            }
            KeyCode::Char('c') => {
                if self.view_mode == ViewMode::Communities {
                    self.view_mode = ViewMode::Files;
                    self.viewing_community_members = false;
                } else {
                    self.view_mode = ViewMode::Communities;
                    self.viewing_community_members = false;
                    self.active_panel = ActivePanel::Symbols;
                }
            }
            KeyCode::Char('p') => {
                if self.view_mode == ViewMode::Processes {
                    self.view_mode = ViewMode::Files;
                    self.viewing_process_steps = false;
                } else {
                    self.view_mode = ViewMode::Processes;
                    self.viewing_process_steps = false;
                    self.active_panel = ActivePanel::Symbols;
                }
            }
            _ => {}
        }
    }
}

// ─── File tree building ─────────────────────────────────────────────────────

fn build_file_tree(graph: &KnowledgeGraph) -> Vec<TreeItem> {
    // Collect unique file paths from graph
    let mut paths: Vec<String> = Vec::new();
    for node in graph.iter_nodes() {
        if node.label == NodeLabel::File {
            paths.push(node.properties.file_path.clone());
        }
    }
    paths.sort();
    paths.dedup();

    // Build directory structure
    let mut dirs: Vec<String> = Vec::new();
    for p in &paths {
        let parts: Vec<&str> = p.split('/').collect();
        let mut current = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                break; // skip the file part
            }
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            if !dirs.contains(&current) {
                dirs.push(current.clone());
            }
        }
    }
    dirs.sort();

    // Build top-level items (depth 0)
    let mut top_level_dirs: Vec<String> = Vec::new();
    let mut top_level_files: Vec<String> = Vec::new();

    for d in &dirs {
        if !d.contains('/') {
            top_level_dirs.push(d.clone());
        }
    }
    for p in &paths {
        if !p.contains('/') {
            top_level_files.push(p.clone());
        }
    }

    let mut tree: Vec<TreeItem> = Vec::new();

    // Add top-level dirs
    for d in &top_level_dirs {
        let children = count_direct_children(&dirs, &paths, d);
        tree.push(TreeItem {
            name: d.clone(),
            path: d.clone(),
            is_dir: true,
            depth: 0,
            expanded: false,
            children_count: children,
        });
    }

    // Add top-level files
    for f in &top_level_files {
        tree.push(TreeItem {
            name: f.clone(),
            path: f.clone(),
            is_dir: false,
            depth: 0,
            expanded: false,
            children_count: 0,
        });
    }

    tree
}

fn count_direct_children(dirs: &[String], files: &[String], parent: &str) -> usize {
    let prefix = format!("{parent}/");
    let mut count = 0;
    for d in dirs {
        if let Some(rest) = d.strip_prefix(&prefix) {
            if !rest.contains('/') {
                count += 1;
            }
        }
    }
    for f in files {
        if let Some(rest) = f.strip_prefix(&prefix) {
            if !rest.contains('/') {
                count += 1;
            }
        }
    }
    count
}

fn get_children_for_path(graph: &KnowledgeGraph, parent_path: &str, depth: usize) -> Vec<TreeItem> {
    let prefix = format!("{parent_path}/");

    // Collect all file paths
    let mut all_files: Vec<String> = Vec::new();
    for node in graph.iter_nodes() {
        if node.label == NodeLabel::File {
            all_files.push(node.properties.file_path.clone());
        }
    }
    all_files.sort();
    all_files.dedup();

    // Find child dirs and files
    let mut child_dirs: Vec<String> = Vec::new();
    let mut child_files: Vec<String> = Vec::new();

    // Collect all dirs
    let mut all_dirs: Vec<String> = Vec::new();
    for p in &all_files {
        let parts: Vec<&str> = p.split('/').collect();
        let mut current = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                break;
            }
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            if !all_dirs.contains(&current) {
                all_dirs.push(current.clone());
            }
        }
    }
    all_dirs.sort();

    for d in &all_dirs {
        if let Some(rest) = d.strip_prefix(&prefix) {
            if !rest.contains('/') {
                child_dirs.push(d.clone());
            }
        }
    }

    for f in &all_files {
        if let Some(rest) = f.strip_prefix(&prefix) {
            if !rest.contains('/') {
                child_files.push(f.clone());
            }
        }
    }

    let mut children: Vec<TreeItem> = Vec::new();

    // Dirs first
    for d in &child_dirs {
        let name = d.strip_prefix(&prefix).unwrap_or(d).to_string();
        let cnt = count_direct_children(&all_dirs, &all_files, d);
        children.push(TreeItem {
            name,
            path: d.clone(),
            is_dir: true,
            depth,
            expanded: false,
            children_count: cnt,
        });
    }

    // Then files
    for f in &child_files {
        let name = f.strip_prefix(&prefix).unwrap_or(f).to_string();
        children.push(TreeItem {
            name,
            path: f.clone(),
            is_dir: false,
            depth,
            expanded: false,
            children_count: 0,
        });
    }

    children
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn is_symbol_label(label: NodeLabel) -> bool {
    matches!(
        label,
        NodeLabel::Function
            | NodeLabel::Class
            | NodeLabel::Method
            | NodeLabel::Variable
            | NodeLabel::Interface
            | NodeLabel::Enum
            | NodeLabel::Struct
            | NodeLabel::Trait
            | NodeLabel::Impl
            | NodeLabel::TypeAlias
            | NodeLabel::Const
            | NodeLabel::Static
            | NodeLabel::Constructor
            | NodeLabel::Macro
            | NodeLabel::Property
            | NodeLabel::Decorator
            | NodeLabel::Type
            | NodeLabel::Record
            | NodeLabel::Delegate
            | NodeLabel::Annotation
            | NodeLabel::Union
            | NodeLabel::Namespace
            | NodeLabel::Typedef
            | NodeLabel::Template
            | NodeLabel::Route
            | NodeLabel::Tool
    )
}

fn label_badge(label: NodeLabel) -> &'static str {
    match label {
        NodeLabel::Function => "F",
        NodeLabel::Class => "C",
        NodeLabel::Method => "M",
        NodeLabel::Struct => "S",
        NodeLabel::Interface => "I",
        NodeLabel::Enum => "E",
        NodeLabel::Variable => "V",
        NodeLabel::Trait => "T",
        NodeLabel::Impl => "Im",
        NodeLabel::TypeAlias => "Ta",
        NodeLabel::Const => "K",
        NodeLabel::Static => "St",
        NodeLabel::Constructor => "Ct",
        NodeLabel::Macro => "Ma",
        NodeLabel::Property => "P",
        NodeLabel::Decorator => "D",
        NodeLabel::Type => "Ty",
        NodeLabel::Record => "R",
        NodeLabel::Route => "Rt",
        NodeLabel::Tool => "Tl",
        NodeLabel::Namespace => "Ns",
        NodeLabel::Union => "U",
        NodeLabel::Typedef => "Td",
        NodeLabel::Template => "Tp",
        NodeLabel::Delegate => "Dl",
        NodeLabel::Annotation => "An",
        _ => "?",
    }
}

fn label_color(label: NodeLabel) -> Color {
    match label {
        NodeLabel::Function => Color::Indexed(39),    // blue
        NodeLabel::Class => Color::Indexed(178),      // gold
        NodeLabel::Method => Color::Indexed(75),      // light blue
        NodeLabel::Struct => Color::Indexed(114),     // green
        NodeLabel::Interface => Color::Indexed(141),  // purple
        NodeLabel::Enum => Color::Indexed(208),       // orange
        NodeLabel::Variable => Color::Indexed(252),   // gray
        NodeLabel::Trait => Color::Indexed(170),      // magenta
        NodeLabel::Impl => Color::Indexed(109),       // teal
        NodeLabel::Const => Color::Indexed(228),      // yellow
        NodeLabel::Constructor => Color::Indexed(75), // light blue
        NodeLabel::Macro => Color::Indexed(196),      // red
        NodeLabel::Route => Color::Indexed(48),       // bright green
        NodeLabel::Tool => Color::Indexed(45),        // cyan
        _ => Color::Indexed(252),                     // gray
    }
}

fn file_color(name: &str) -> Color {
    if name.ends_with(".rs") {
        Color::Indexed(208) // orange (Rust)
    } else if name.ends_with(".ts") || name.ends_with(".tsx") {
        Color::Indexed(39) // blue (TypeScript)
    } else if name.ends_with(".js") || name.ends_with(".jsx") {
        Color::Indexed(228) // yellow (JavaScript)
    } else if name.ends_with(".py") {
        Color::Indexed(39) // blue (Python)
    } else if name.ends_with(".java") {
        Color::Indexed(196) // red (Java)
    } else if name.ends_with(".go") {
        Color::Indexed(75) // light blue (Go)
    } else if name.ends_with(".rb") {
        Color::Indexed(196) // red (Ruby)
    } else if name.ends_with(".cs") {
        Color::Indexed(141) // purple (C#)
    } else if name.ends_with(".c") || name.ends_with(".h") {
        Color::Indexed(252) // gray (C)
    } else if name.ends_with(".cpp") || name.ends_with(".hpp") {
        Color::Indexed(75) // light blue (C++)
    } else if name.ends_with(".php") {
        Color::Indexed(141) // purple (PHP)
    } else if name.ends_with(".swift") {
        Color::Indexed(208) // orange (Swift)
    } else if name.ends_with(".kt") {
        Color::Indexed(141) // purple (Kotlin)
    } else {
        Color::Indexed(252) // default gray
    }
}

// ─── Scrolling helper ───────────────────────────────────────────────────────

fn ensure_visible(selected: usize, scroll_offset: &mut usize, visible_height: usize) {
    if visible_height == 0 {
        return;
    }
    if selected < *scroll_offset {
        *scroll_offset = selected;
    } else if selected >= *scroll_offset + visible_height {
        *scroll_offset = selected - visible_height + 1;
    }
}

// ─── Rendering ──────────────────────────────────────────────────────────────

fn ui(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Main layout: title bar + body + status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title bar
            Constraint::Min(5),    // body
            Constraint::Length(3), // status bar
        ])
        .split(size);

    render_title_bar(f, app, main_chunks[0]);
    render_body(f, app, main_chunks[1]);
    render_status_bar(f, app, main_chunks[2]);
}

fn render_title_bar(f: &mut Frame, app: &App, area: Rect) {
    let node_count = app.graph.node_count();
    let rel_count = app.graph.relationship_count();
    let community_count = app.communities.len();
    let process_count = app.processes.len();

    let title = format!(
        " {} | {} nodes | {} edges | {} communities | {} processes ",
        app.repo_name, node_count, rel_count, community_count, process_count,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Indexed(240)))
        .title(Line::from(vec![
            Span::styled(
                " Code Explorer ",
                Style::default()
                    .fg(Color::Indexed(39))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Dashboard", Style::default().fg(Color::Indexed(75))),
        ]));

    let paragraph = Paragraph::new(Line::from(vec![Span::styled(
        title,
        Style::default().fg(Color::Indexed(250)),
    )]))
    .block(block);

    f.render_widget(paragraph, area);
}

fn render_body(f: &mut Frame, app: &mut App, area: Rect) {
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // file tree
            Constraint::Percentage(40), // symbols
            Constraint::Percentage(35), // details
        ])
        .split(area);

    render_file_tree(f, app, body_chunks[0]);
    render_center_panel(f, app, body_chunks[1]);
    render_details(f, app, body_chunks[2]);
}

fn render_file_tree(f: &mut Frame, app: &mut App, area: Rect) {
    let is_active = app.active_panel == ActivePanel::FileTree;
    let border_color = if is_active {
        Color::Indexed(39)
    } else {
        Color::Indexed(240)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(vec![
            Span::styled(
                " File Tree ",
                Style::default()
                    .fg(Color::Indexed(39))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("({})", app.file_tree.len()),
                Style::default().fg(Color::Indexed(245)),
            ),
        ]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.file_tree.is_empty() {
        let msg = Paragraph::new("No files found").style(Style::default().fg(Color::Indexed(245)));
        f.render_widget(msg, inner);
        return;
    }

    let visible_height = inner.height as usize;
    ensure_visible(
        app.file_selected,
        &mut app.file_scroll_offset,
        visible_height,
    );

    let items: Vec<Line> = app
        .file_tree
        .iter()
        .enumerate()
        .skip(app.file_scroll_offset)
        .take(visible_height)
        .map(|(i, item)| {
            let indent = "  ".repeat(item.depth);
            let icon = if item.is_dir {
                if item.expanded {
                    "\u{25BC} "
                } else {
                    "\u{25B6} "
                }
            } else {
                "  "
            };
            let name_color = if item.is_dir {
                Color::Indexed(39)
            } else {
                file_color(&item.name)
            };
            let suffix = if item.is_dir {
                format!(" ({})", item.children_count)
            } else {
                String::new()
            };

            let style = if i == app.file_selected {
                Style::default()
                    .bg(Color::Indexed(236))
                    .fg(name_color)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(name_color)
            };

            Line::from(vec![
                Span::raw(indent),
                Span::styled(format!("{icon}{}{suffix}", item.name), style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(items);
    f.render_widget(paragraph, inner);
}

fn render_center_panel(f: &mut Frame, app: &mut App, area: Rect) {
    match app.view_mode {
        ViewMode::Files => render_symbols_panel(f, app, area),
        ViewMode::Communities => render_communities_panel(f, app, area),
        ViewMode::Processes => render_processes_panel(f, app, area),
    }
}

fn render_symbols_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let is_active = app.active_panel == ActivePanel::Symbols;
    let border_color = if is_active {
        Color::Indexed(39)
    } else {
        Color::Indexed(240)
    };

    let title_text = if app.search_mode {
        format!(" Search: {} ", app.search_query)
    } else {
        " Symbols ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(vec![Span::styled(
            title_text,
            Style::default()
                .fg(if app.search_mode {
                    Color::Indexed(228)
                } else {
                    Color::Indexed(39)
                })
                .add_modifier(Modifier::BOLD),
        )]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Build an owned list to avoid borrow conflicts with ensure_visible
    let symbol_list: Vec<SymbolItem> = if app.search_mode && !app.search_query.is_empty() {
        app.filtered_symbols().into_iter().cloned().collect()
    } else {
        app.symbols.clone()
    };

    if symbol_list.is_empty() {
        let msg = if app.search_mode {
            "No matches"
        } else {
            "Select a file to see symbols"
        };
        let paragraph = Paragraph::new(msg).style(Style::default().fg(Color::Indexed(245)));
        f.render_widget(paragraph, inner);
        return;
    }

    let visible_height = inner.height as usize;
    ensure_visible(
        app.symbol_selected,
        &mut app.symbol_scroll_offset,
        visible_height,
    );

    let items: Vec<Line> = symbol_list
        .iter()
        .enumerate()
        .skip(app.symbol_scroll_offset)
        .take(visible_height)
        .map(|(i, sym)| {
            let badge = label_badge(sym.label);
            let color = label_color(sym.label);

            let style = if i == app.symbol_selected {
                Style::default()
                    .bg(Color::Indexed(236))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Line::from(vec![
                Span::styled(
                    format!(" [{badge}] "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(sym.name.clone(), style.fg(Color::Indexed(255))),
                Span::styled(
                    if sym.lines.is_empty() {
                        String::new()
                    } else {
                        format!("  L{}", sym.lines)
                    },
                    Style::default().fg(Color::Indexed(240)),
                ),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(items);
    f.render_widget(paragraph, inner);
}

fn render_communities_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let is_active = app.active_panel == ActivePanel::Symbols;
    let border_color = if is_active {
        Color::Indexed(39)
    } else {
        Color::Indexed(240)
    };

    if app.viewing_community_members {
        // Show community members
        let community_name = app
            .communities
            .get(app.community_selected)
            .map(|c| c.name.clone())
            .unwrap_or_default();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(vec![Span::styled(
                format!(" Community: {community_name} "),
                Style::default()
                    .fg(Color::Indexed(178))
                    .add_modifier(Modifier::BOLD),
            )]));

        let inner = block.inner(area);
        f.render_widget(block, area);

        if app.community_members.is_empty() {
            let paragraph =
                Paragraph::new("No members found").style(Style::default().fg(Color::Indexed(245)));
            f.render_widget(paragraph, inner);
            return;
        }

        let visible_height = inner.height as usize;
        ensure_visible(
            app.community_member_selected,
            &mut app.community_member_scroll_offset,
            visible_height,
        );

        let items: Vec<Line> = app
            .community_members
            .iter()
            .enumerate()
            .skip(app.community_member_scroll_offset)
            .take(visible_height)
            .map(|(i, sym)| {
                let badge = label_badge(sym.label);
                let color = label_color(sym.label);
                let style = if i == app.community_member_selected {
                    Style::default()
                        .bg(Color::Indexed(236))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(
                        format!(" [{badge}] "),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&sym.name, style.fg(Color::Indexed(255))),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(items);
        f.render_widget(paragraph, inner);
    } else {
        // Show community list
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(vec![Span::styled(
                format!(" Communities ({}) ", app.communities.len()),
                Style::default()
                    .fg(Color::Indexed(178))
                    .add_modifier(Modifier::BOLD),
            )]));

        let inner = block.inner(area);
        f.render_widget(block, area);

        if app.communities.is_empty() {
            let paragraph = Paragraph::new("No communities detected")
                .style(Style::default().fg(Color::Indexed(245)));
            f.render_widget(paragraph, inner);
            return;
        }

        let visible_height = inner.height as usize;
        ensure_visible(
            app.community_selected,
            &mut app.community_scroll_offset,
            visible_height,
        );

        let items: Vec<Line> = app
            .communities
            .iter()
            .enumerate()
            .skip(app.community_scroll_offset)
            .take(visible_height)
            .map(|(i, com)| {
                let style = if i == app.community_selected {
                    Style::default()
                        .bg(Color::Indexed(236))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let cohesion_str = com
                    .cohesion
                    .map(|c| format!(" [{:.0}%]", c * 100.0))
                    .unwrap_or_default();

                Line::from(vec![
                    Span::styled(
                        format!(" {} ", com.member_count),
                        Style::default().fg(Color::Indexed(39)),
                    ),
                    Span::styled(&com.name, style.fg(Color::Indexed(178))),
                    Span::styled(cohesion_str, Style::default().fg(Color::Indexed(245))),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(items);
        f.render_widget(paragraph, inner);
    }
}

fn render_processes_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let is_active = app.active_panel == ActivePanel::Symbols;
    let border_color = if is_active {
        Color::Indexed(39)
    } else {
        Color::Indexed(240)
    };

    if app.viewing_process_steps {
        // Show process steps
        let process_name = app
            .processes
            .get(app.process_selected)
            .map(|p| p.name.clone())
            .unwrap_or_default();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(vec![Span::styled(
                format!(" Process: {process_name} "),
                Style::default()
                    .fg(Color::Indexed(48))
                    .add_modifier(Modifier::BOLD),
            )]));

        let inner = block.inner(area);
        f.render_widget(block, area);

        if app.process_steps.is_empty() {
            let paragraph =
                Paragraph::new("No steps found").style(Style::default().fg(Color::Indexed(245)));
            f.render_widget(paragraph, inner);
            return;
        }

        let visible_height = inner.height as usize;
        ensure_visible(
            app.process_step_selected,
            &mut app.process_step_scroll_offset,
            visible_height,
        );

        let items: Vec<Line> = app
            .process_steps
            .iter()
            .enumerate()
            .skip(app.process_step_scroll_offset)
            .take(visible_height)
            .map(|(i, sym)| {
                let badge = label_badge(sym.label);
                let color = label_color(sym.label);
                let style = if i == app.process_step_selected {
                    Style::default()
                        .bg(Color::Indexed(236))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(
                        format!(" {}. ", i + 1),
                        Style::default().fg(Color::Indexed(245)),
                    ),
                    Span::styled(
                        format!("[{badge}] "),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&sym.name, style.fg(Color::Indexed(255))),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(items);
        f.render_widget(paragraph, inner);
    } else {
        // Show process list
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(vec![Span::styled(
                format!(" Processes ({}) ", app.processes.len()),
                Style::default()
                    .fg(Color::Indexed(48))
                    .add_modifier(Modifier::BOLD),
            )]));

        let inner = block.inner(area);
        f.render_widget(block, area);

        if app.processes.is_empty() {
            let paragraph = Paragraph::new("No processes detected")
                .style(Style::default().fg(Color::Indexed(245)));
            f.render_widget(paragraph, inner);
            return;
        }

        let visible_height = inner.height as usize;
        ensure_visible(
            app.process_selected,
            &mut app.process_scroll_offset,
            visible_height,
        );

        let items: Vec<Line> = app
            .processes
            .iter()
            .enumerate()
            .skip(app.process_scroll_offset)
            .take(visible_height)
            .map(|(i, proc)| {
                let style = if i == app.process_selected {
                    Style::default()
                        .bg(Color::Indexed(236))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                Line::from(vec![
                    Span::styled(
                        format!(" {} steps ", proc.step_count),
                        Style::default().fg(Color::Indexed(39)),
                    ),
                    Span::styled(&proc.name, style.fg(Color::Indexed(48))),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(items);
        f.render_widget(paragraph, inner);
    }
}

fn render_details(f: &mut Frame, app: &mut App, area: Rect) {
    let is_active = app.active_panel == ActivePanel::Details;
    let border_color = if is_active {
        Color::Indexed(39)
    } else {
        Color::Indexed(240)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Line::from(vec![Span::styled(
            " Details ",
            Style::default()
                .fg(Color::Indexed(39))
                .add_modifier(Modifier::BOLD),
        )]));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let symbol = app.current_selected_symbol().cloned();
    let symbol = match symbol {
        Some(s) => s,
        None => {
            let paragraph = Paragraph::new("Select a symbol to see details")
                .style(Style::default().fg(Color::Indexed(245)));
            f.render_widget(paragraph, inner);
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::new();

    // Symbol name and type
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {} ", symbol.name),
            Style::default()
                .fg(Color::Indexed(255))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{}]", label_badge(symbol.label)),
            Style::default()
                .fg(label_color(symbol.label))
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    // File path
    if !app.detail_file.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(" File: ", Style::default().fg(Color::Indexed(245))),
            Span::styled(
                &app.detail_file,
                Style::default().fg(file_color(&app.detail_file)),
            ),
        ]));
    }

    // Line range
    if !app.detail_lines.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(" Lines: ", Style::default().fg(Color::Indexed(245))),
            Span::styled(&app.detail_lines, Style::default().fg(Color::Indexed(252))),
        ]));
    }

    lines.push(Line::from(""));

    // Community
    if let Some(ref label) = app.community_label {
        lines.push(Line::from(vec![
            Span::styled(
                " Community: ",
                Style::default()
                    .fg(Color::Indexed(178))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(label.as_str(), Style::default().fg(Color::Indexed(178))),
        ]));
        lines.push(Line::from(""));
    }

    // Callers
    if !app.callers.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(" Callers ({})", app.callers.len()),
            Style::default()
                .fg(Color::Indexed(114))
                .add_modifier(Modifier::BOLD),
        )));
        for caller in &app.callers {
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(caller.as_str(), Style::default().fg(Color::Indexed(114))),
            ]));
        }
        lines.push(Line::from(""));
    }

    // Callees
    if !app.callees.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(" Callees ({})", app.callees.len()),
            Style::default()
                .fg(Color::Indexed(75))
                .add_modifier(Modifier::BOLD),
        )));
        for callee in &app.callees {
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(callee.as_str(), Style::default().fg(Color::Indexed(75))),
            ]));
        }
        lines.push(Line::from(""));
    }

    // Extra edges
    if !app.detail_extra.is_empty() {
        lines.push(Line::from(Span::styled(
            " Relationships",
            Style::default()
                .fg(Color::Indexed(141))
                .add_modifier(Modifier::BOLD),
        )));
        for (kind, target) in &app.detail_extra {
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(kind.as_str(), Style::default().fg(Color::Indexed(245))),
                Span::styled(" -> ", Style::default().fg(Color::Indexed(240))),
                Span::styled(target.as_str(), Style::default().fg(Color::Indexed(141))),
            ]));
        }
    }

    // If nothing interesting, show a hint
    if app.callers.is_empty()
        && app.callees.is_empty()
        && app.community_label.is_none()
        && app.detail_extra.is_empty()
    {
        lines.push(Line::from(Span::styled(
            " No relationships found",
            Style::default().fg(Color::Indexed(245)),
        )));
    }

    let paragraph = Paragraph::new(lines).scroll((app.detail_scroll_offset as u16, 0));
    f.render_widget(paragraph, inner);
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let mode_text = match app.view_mode {
        ViewMode::Files => "Files",
        ViewMode::Communities => "Communities",
        ViewMode::Processes => "Processes",
    };

    let panel_text = match app.active_panel {
        ActivePanel::FileTree => "File Tree",
        ActivePanel::Symbols => match app.view_mode {
            ViewMode::Files => "Symbols",
            ViewMode::Communities => "Communities",
            ViewMode::Processes => "Processes",
        },
        ActivePanel::Details => "Details",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Indexed(240)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let keys = if app.search_mode {
        vec![
            Span::styled(" Type to search ", Style::default().fg(Color::Indexed(228))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                "Esc",
                Style::default()
                    .fg(Color::Indexed(39))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Cancel ", Style::default().fg(Color::Indexed(250))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(Color::Indexed(39))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Confirm", Style::default().fg(Color::Indexed(250))),
        ]
    } else {
        vec![
            Span::styled(
                "Tab",
                Style::default()
                    .fg(Color::Indexed(39))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Panel ", Style::default().fg(Color::Indexed(250))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                "j/k",
                Style::default()
                    .fg(Color::Indexed(39))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Nav ", Style::default().fg(Color::Indexed(250))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(Color::Indexed(39))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Select ", Style::default().fg(Color::Indexed(250))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                "/",
                Style::default()
                    .fg(Color::Indexed(39))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Search ", Style::default().fg(Color::Indexed(250))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                "c",
                Style::default()
                    .fg(Color::Indexed(178))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Communities ", Style::default().fg(Color::Indexed(250))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                "p",
                Style::default()
                    .fg(Color::Indexed(48))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Processes ", Style::default().fg(Color::Indexed(250))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                "q",
                Style::default()
                    .fg(Color::Indexed(196))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Quit ", Style::default().fg(Color::Indexed(250))),
            Span::styled("| ", Style::default().fg(Color::Indexed(240))),
            Span::styled(
                format!(" [{mode_text}] {panel_text} "),
                Style::default().fg(Color::Indexed(245)),
            ),
        ]
    };

    let paragraph = Paragraph::new(Line::from(keys));
    f.render_widget(paragraph, inner);
}

// ─── Run loop ───────────────────────────────────────────────────────────────

pub fn run(path: Option<&str>) -> anyhow::Result<()> {
    let repo_path = match path {
        Some(p) => PathBuf::from(p)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(p)),
        None => std::env::current_dir()?,
    };

    let storage = repo_manager::get_storage_paths(&repo_path);
    let snap_path = code_explorer_db::snapshot::snapshot_path(&storage.storage_path);

    if !snap_path.exists() {
        anyhow::bail!(
            "No graph snapshot found at {}. Run `code-explorer analyze` first.",
            snap_path.display()
        );
    }

    eprintln!("Loading graph from {}...", snap_path.display());
    let graph = code_explorer_db::snapshot::load_snapshot(&snap_path)?;
    let node_count = graph.node_count();
    let edge_count = graph.relationship_count();

    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    eprintln!("Loaded {node_count} nodes, {edge_count} edges. Starting dashboard...");

    let mut app = App::new(graph, repo_name);

    // Setup terminal. If any setup step fails after `enable_raw_mode`
    // succeeds, we MUST roll back what we've done — otherwise the user's
    // shell is left in raw mode + alternate screen after we exit.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    if let Err(e) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(e.into());
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            let _ = disable_raw_mode();
            return Err(e.into());
        }
    };

    // Main event loop
    let result = run_event_loop(&mut terminal, &mut app);

    // Always restore terminal, ignoring cleanup errors. If we used `?` here
    // and a cleanup step failed, it would mask the original `result` (which
    // is the user's actual error from the event loop) — diagnosing a TUI
    // crash is much harder when the real error is replaced by a generic
    // "failed to leave alternate screen".
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Ignore key release events on Windows
                if key.kind == event::KeyEventKind::Press {
                    app.handle_key(key);
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
