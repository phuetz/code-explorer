//! Qualified Rust call paths (`crate::`, `self::`, `super::`, child modules,
//! `use` aliases, and path-dependencies of the current package).
//!
//! A path-shaped qualifier is never resolved by a bare-name guess. Zero
//! targets stay unresolved. Several distinct callable nodes are reported as
//! ambiguous instead of picking one.
//!
//! A single identifier that is not a module (`Type`, `Self`, `Vec`) is
//! [`RustCallDecision::NotAModule`]. The caller may use the historical
//! same-file tier only when the qualifier is `Self` inside the enclosing
//! impl, a type or trait that owns `fn` in this file, or an inline module.
//! It must not import-scope or globally guess: `Vec::new()` is not a call
//! to another file's `new`, and `fs::write` is not a local `fn write`.
//!
//! Arguments of macros are not calls (tree-sitter does not surface them).
//! That limit is older than this resolver and stays out of scope.
//!
//! A `use` written inside a function or a block applies only inside that
//! block. A module-level `use` is visible in that module. An inline child
//! sees it only through its own `use super::*` (the glob is not expanded
//! name by name; the parent's bindings are reopened). A local type-namespace
//! item (`mod`, `struct`, `enum`, `trait`, `type`, `union`) hides that name.
//! A function does not: `fn git` does not hide the module `git`.
//!
//! `#[path = "file.rs"]` applies only to the `mod` declaration it precedes.
//! A nested `#[path]` does not retarget a homonym at file scope. When several
//! `mod name;` declarations stay possible (`#[cfg(unix)]` against
//! `#[cfg(windows)]`, two paths, an unknown `cfg`), the call is ambiguous:
//! no 0.95 edge. The one exact case is `#[cfg(test)]` against
//! `#[cfg(not(test))]`: a call that is not itself under `cfg(test)` resolves
//! to the production module, and a call inside `cfg(test)` resolves to the
//! test module.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[path = "rust_qualified_scan.rs"]
mod scan;

use scan::{file_has_method, glob_match, glob_may_descend, innermost_block, is_glob, is_type_like, mod_decls, parse_ufcs, split_type_tail, type_base, CallMode, CfgKind, ImplBlock, InlineMod, ModDecl};

const PUB_USE_MAX_DEPTH: usize = 8;

use code_explorer_core::symbol::{SymbolDefinition, SymbolTable};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustCallDecision {
    /// Exactly one callable node.
    Link { node_id: String, reason: String },
    /// More than one distinct callable node. No edge should be created.
    Ambiguous { note: String },
    /// The qualifier is a module path (or a path-shaped qualifier that did not
    /// resolve). Do not guess a homonym.
    Unresolved,
    /// Single identifier that is not a module (`S`, `Self`). Not a path.
    /// The caller may fall back to the historical same-file tier only.
    NotAModule,
}

#[derive(Debug, Clone)]
pub struct Package {
    /// Package name with `-` rewritten to `_` (Rust extern crate name).
    pub name: String,
    /// Directory of the package's Cargo.toml, relative to the repo, `""` at the root.
    pub dir: String,
    /// Source directory (`dir/src` when present, otherwise `dir`).
    pub src: String,
    /// Path dependencies: extern name → package directory.
    pub deps: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct TypeImport {
    local: String,
    type_name: String,
    modules: Vec<String>,
    /// `Some` when the `use` sits in a function or a block: visible only there.
    block: Option<(usize, usize)>,
    /// Inline module that contains a module-level `use` (`None` = the file).
    module: Option<usize>,
}

#[derive(Debug, Clone)]
struct Alias {
    local: String,
    files: Vec<String>,
    block: Option<(usize, usize)>,
    module: Option<usize>,
}

#[derive(Debug, Clone)]
struct PubUse {
    exported: String,
    module_path: String,
    item: String,
}

#[derive(Debug, Clone)]
struct FileFacts {
    aliases: Vec<Alias>,
    type_imports: Vec<TypeImport>,
    inline_mods: Vec<InlineMod>,
    impls: Vec<ImplBlock>,
    pub_uses: Vec<PubUse>,
    /// Accolades `#[cfg(test)]` de ce fichier. Un octet dedans est un appel de test.
    test_ranges: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
enum Loc {
    File(String),
    Inline { file: String, index: usize },
}

#[derive(Clone)]
struct WsInherit {
    root: String,
    deps: Vec<(String, String)>,
}

impl Default for WsInherit {
    fn default() -> Self {
        Self {
            root: String::new(),
            deps: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct RustWorkspace {
    files: HashSet<String>,
    packages: Vec<Package>,
    repo: Option<PathBuf>,
    contents: RefCell<HashMap<String, String>>,
    facts: RefCell<HashMap<String, Arc<FileFacts>>>,
    /// `mod name;` lu une fois par fichier. `child_modules` est appelé à chaque segment.
    mod_decl_cache: RefCell<HashMap<String, Vec<ModDecl>>>,
    /// How many times this workspace parsed a file's `use` list.
    facts_builds: AtomicUsize,
}

impl RustWorkspace {
    pub fn new(files: HashSet<String>, packages: Vec<Package>) -> Self {
        Self {
            files,
            packages,
            repo: None,
            contents: RefCell::new(HashMap::new()),
            facts: RefCell::new(HashMap::new()),
            mod_decl_cache: RefCell::new(HashMap::new()),
            facts_builds: AtomicUsize::new(0),
        }
    }

    /// Read Cargo.toml manifests under `repo`. Missing or unreadable manifests
    /// yield a workspace that only knows the file set (paths then stay unresolved).
    pub fn load(repo: &Path, files: &HashSet<String>) -> Self {
        let mut packages = Vec::new();
        let mut seen = HashSet::new();
        ingest_manifest(
            repo,
            "",
            files,
            &mut packages,
            &mut seen,
            &WsInherit::default(),
        );
        Self {
            files: files.clone(),
            packages,
            repo: Some(repo.to_path_buf()),
            contents: RefCell::new(HashMap::new()),
            facts: RefCell::new(HashMap::new()),
            mod_decl_cache: RefCell::new(HashMap::new()),
            facts_builds: AtomicUsize::new(0),
        }
    }

    /// Remember file texts so `pub use` and imported types can be read once.
    pub fn preload_contents(&self, pairs: impl IntoIterator<Item = (String, String)>) {
        let mut map = self.contents.borrow_mut();
        for (path, content) in pairs {
            map.entry(path).or_insert(content);
        }
    }

    pub fn decide(
        &self,
        from_file: &str,
        content: &str,
        qualifier: &str,
        called_name: &str,
        symbols: &SymbolTable,
    ) -> RustCallDecision {
        self.decide_at(from_file, content, qualifier, called_name, symbols, None)
    }

    pub fn decide_at(
        &self,
        from_file: &str,
        content: &str,
        qualifier: &str,
        called_name: &str,
        symbols: &SymbolTable,
        call_byte: Option<usize>,
    ) -> RustCallDecision {
        let facts = self.facts_for(from_file, content);
        let mode = call_mode_at(&facts.test_ranges, call_byte);
        let (locs, ambiguous) = self.resolve_locations(
            from_file,
            qualifier,
            Some(&facts.aliases),
            call_byte,
            Some(&facts.inline_mods),
            true,
            mode,
        );
        // Plusieurs modules restent possibles : aucun lien à 0,95.
        if ambiguous {
            return RustCallDecision::Ambiguous {
                note: format!("{qualifier}::{called_name} (plusieurs modules possibles)"),
            };
        }
        if !locs.is_empty() {
            let targets = self.lookup_locs(&locs, called_name, symbols, 0, &mut HashSet::new());
            return decision_of(&targets, qualifier, called_name, "rust-path");
        }
        if let Some(decision) =
            self.resolve_type_call(from_file, qualifier, called_name, symbols, &facts, call_byte)
        {
            return decision;
        }
        // `crate::missing`, `super::nope`, `a::b` stay unresolved.
        // `S` / `Self` are types, not modules: the caller keeps same-file.
        if qualifier_is_path_shaped(qualifier) {
            return RustCallDecision::Unresolved;
        }
        RustCallDecision::NotAModule
    }

    /// Same-file tier allowed by the counter-review rule. `None` means no edge.
    pub fn same_file_target(
        &self,
        from_file: &str,
        content: &str,
        qualifier: &str,
        called_name: &str,
        call_byte: Option<usize>,
        symbols: &SymbolTable,
    ) -> Option<RustCallDecision> {
        let facts = self.facts_for(from_file, content);
        let q = qualifier.trim();
        if let Some((ty, tr)) = parse_ufcs(q) {
            let ty_name = type_base(ty);
            let tr_name = type_base(tr);
            let mut names = vec![ty_name.to_string()];
            if tr_name != ty_name {
                names.push(tr_name.to_string());
            }
            if names.iter().any(|n| {
                (n == "Self" && self_encloses(&facts.impls, call_byte, called_name))
                    || file_has_method(&facts.impls, n, called_name)
            }) {
                return link_same_file(symbols, from_file, called_name);
            }
            return None;
        }
        if q.contains("::") {
            return None;
        }
        let base = type_base(q);
        if base == "Self" {
            if self_encloses(&facts.impls, call_byte, called_name) {
                return link_same_file(symbols, from_file, called_name);
            }
            return None;
        }
        if is_type_like(base) {
            if file_has_method(&facts.impls, base, called_name) {
                return link_same_file(symbols, from_file, called_name);
            }
            return None;
        }
        let scope = call_byte.and_then(|b| scan::enclosing_inline(&facts.inline_mods, b));
        if let Some(m) = facts.inline_mods.iter().find(|m| {
            m.name == base && m.parent == scope && m.functions.iter().any(|f| f == called_name)
        }) {
            if m.functions.iter().any(|f| f == called_name) {
                return link_same_file(symbols, from_file, called_name);
            }
        }
        // `inner::f()` at file scope: parent of the inline mod is None.
        if scope.is_none() {
            if facts.inline_mods.iter().any(|m| {
                m.parent.is_none()
                    && m.name == base
                    && m.functions.iter().any(|f| f == called_name)
            }) {
                return link_same_file(symbols, from_file, called_name);
            }
        }
        None
    }

    fn resolve_type_call(
        &self,
        from_file: &str,
        qualifier: &str,
        called_name: &str,
        symbols: &SymbolTable,
        facts: &FileFacts,
        call_byte: Option<usize>,
    ) -> Option<RustCallDecision> {
        let mode = call_mode_at(&facts.test_ranges, call_byte);
        if let Some((module_path, type_name)) = split_type_tail(qualifier) {
            let (locs, ambiguous) = self.resolve_locations(
                from_file,
                &module_path,
                Some(&facts.aliases),
                call_byte,
                Some(&facts.inline_mods),
                true,
                mode,
            );
            if ambiguous {
                return Some(RustCallDecision::Ambiguous {
                    note: format!("{qualifier}::{called_name} (plusieurs modules possibles)"),
                });
            }
            let files = loc_files(&locs);
            if files.is_empty() {
                return None;
            }
            let targets = self.methods_named(&files, &type_name, called_name, symbols);
            if targets.is_empty() {
                return None;
            }
            return Some(decision_of(&targets, qualifier, called_name, "rust-type"));
        }
        if let Some((ty, tr)) = parse_ufcs(qualifier) {
            let mut files = Vec::new();
            let mut names = Vec::new();
            for expr in [ty, tr] {
                let (name, found) = self.files_for_type_expr(from_file, expr, facts, call_byte, mode);
                names.push(name);
                for f in found {
                    if !files.contains(&f) {
                        files.push(f);
                    }
                }
            }
            let targets = self.methods_of_names(&files, &names, called_name, symbols);
            if targets.is_empty() {
                return None;
            }
            return Some(decision_of(&targets, qualifier, called_name, "rust-type"));
        }
        let local = type_base(qualifier.trim());
        if qualifier.trim().contains("::") || local == "Self" || !is_type_like(local) {
            return None;
        }
        let imports =
            visible_type_imports(&facts.type_imports, local, call_byte, &facts.inline_mods);
        if imports.is_empty() {
            return None;
        }
        let mut files = Vec::new();
        let mut names = Vec::new();
        for import in imports {
            if !names.contains(&import.type_name) {
                names.push(import.type_name.clone());
            }
            for module in &import.modules {
                if !files.contains(module) {
                    files.push(module.clone());
                }
            }
        }
        let targets = self.methods_of_names(&files, &names, called_name, symbols);
        if targets.is_empty() {
            return None;
        }
        Some(decision_of(&targets, qualifier, called_name, "rust-type"))
    }

    fn files_for_type_expr(
        &self,
        from_file: &str,
        expr: &str,
        facts: &FileFacts,
        call_byte: Option<usize>,
        mode: CallMode,
    ) -> (String, Vec<String>) {
        let expr = expr.trim();
        if let Some((module_path, type_name)) = split_type_tail(expr) {
            let (locs, ambiguous) = self.resolve_locations(
                from_file,
                &module_path,
                Some(&facts.aliases),
                call_byte,
                Some(&facts.inline_mods),
                true,
                mode,
            );
            if ambiguous {
                return (type_name, Vec::new());
            }
            return (type_name, loc_files(&locs));
        }
        let name = type_base(expr).to_string();
        let imports =
            visible_type_imports(&facts.type_imports, &name, call_byte, &facts.inline_mods);
        if let Some(import) = imports.first() {
            return (import.type_name.clone(), import.modules.clone());
        }
        (name, vec![from_file.to_string()])
    }

    fn methods_named(
        &self,
        files: &[String],
        type_name: &str,
        method: &str,
        symbols: &SymbolTable,
    ) -> Vec<Arc<SymbolDefinition>> {
        self.methods_of_names(files, &[type_name.to_string()], method, symbols)
    }

    /// A `use` may name a module that only `pub use`s the type. Follow those
    /// re-exports (bounded) before deciding the type has no method.
    fn expand_type_files(&self, files: &[String], type_names: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut queue: Vec<String> = files.to_vec();
        let mut depth = 0usize;
        while !queue.is_empty() && depth <= PUB_USE_MAX_DEPTH {
            let batch = std::mem::take(&mut queue);
            for file in batch {
                if !seen.insert(file.clone()) {
                    continue;
                }
                let follow = self.facts_of(&file).map(|facts| {
                    let mut next = Vec::new();
                    for re in &facts.pub_uses {
                        // Explicit `pub use path::Type` only. A glob would pull every
                        // type of the target file and relink `Vec::new`-shaped calls.
                        let exported = type_names.iter().any(|n| re.exported == *n);
                        if !exported {
                            continue;
                        }
                        let (locs, ambiguous) = self.resolve_locations(
                            &file,
                            &re.module_path,
                            Some(&facts.aliases),
                            None,
                            Some(&facts.inline_mods),
                            true,
                            CallMode::Production,
                        );
                        if !ambiguous {
                            next.extend(loc_files(&locs));
                        }
                    }
                    next
                });
                out.push(file);
                if let Some(next) = follow {
                    queue.extend(next);
                }
            }
            depth += 1;
        }
        out
    }

    fn methods_of_names(
        &self,
        files: &[String],
        type_names: &[String],
        method: &str,
        symbols: &SymbolTable,
    ) -> Vec<Arc<SymbolDefinition>> {
        let files = self.expand_type_files(files, type_names);
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for file in &files {
            let Some(facts) = self.facts_of(file) else {
                continue;
            };
            if !type_names
                .iter()
                .any(|n| n != "Self" && file_has_method(&facts.impls, n, method))
            {
                continue;
            }
            push_callables(symbols, file, method, &mut out, &mut seen);
        }
        out
    }

    fn facts_for(&self, file: &str, content: &str) -> Arc<FileFacts> {
        if let Some(hit) = self.facts.borrow().get(file) {
            return Arc::clone(hit);
        }
        let built = Arc::new(self.build_facts(file, content));
        self.facts.borrow_mut().insert(file.to_string(), Arc::clone(&built));
        built
    }

    fn facts_of(&self, file: &str) -> Option<Arc<FileFacts>> {
        if let Some(hit) = self.facts.borrow().get(file) {
            return Some(Arc::clone(hit));
        }
        let content = self.content_of(file)?;
        Some(self.facts_for(file, &content))
    }

    fn content_of(&self, file: &str) -> Option<String> {
        if let Some(content) = self.contents.borrow().get(file) {
            return Some(content.clone());
        }
        let repo = self.repo.as_ref()?;
        let path = if file.is_empty() {
            repo.join("Cargo.toml")
        } else {
            repo.join(file)
        };
        fs::read_to_string(path).ok()
    }

    fn build_facts(&self, from_file: &str, content: &str) -> FileFacts {
        self.facts_builds.fetch_add(1, Ordering::SeqCst);
        let (mut inline_mods, impls, blocks) = scan::scan_items(content);
        let test_ranges = scan::cfg_test_ranges(content);
        let mut aliases = Vec::new();
        let mut type_imports = Vec::new();
        let mut pub_uses = Vec::new();
        for stmt in use_statements_at(content) {
            let block = innermost_block(&blocks, stmt.byte);
            let module = scan::enclosing_inline(&inline_mods, stmt.byte);
            // `use super::*` au niveau du module seulement. Dans une fonction,
            // le glob ne rouvre pas les `use` du parent pour tout le module.
            if block.is_none() && imports_parent_glob(&stmt.text) {
                if let Some(idx) = module {
                    if let Some(m) = inline_mods.get_mut(idx) {
                        m.imports_super_glob = true;
                    }
                }
            }
            let mode = call_mode_at(&test_ranges, Some(stmt.byte));
            for (local, path) in binding_paths(&stmt.text) {
                if local.is_empty() || path.is_empty() {
                    continue;
                }
                let item = last_segment(&path).to_string();
                if stmt.is_pub {
                    if item == "*" {
                        let module_path = path.trim_end_matches("::*").trim_end_matches("::");
                        if !module_path.is_empty() {
                            pub_uses.push(PubUse {
                                exported: "*".to_string(),
                                module_path: module_path.to_string(),
                                item: "*".to_string(),
                            });
                        }
                    } else if item != "self" {
                        let module_path = parent_path(&path);
                        if !module_path.is_empty() {
                            pub_uses.push(PubUse {
                                exported: local.clone(),
                                module_path,
                                item: item.clone(),
                            });
                        }
                    }
                }
                if local == "*" || item == "*" {
                    continue;
                }
                if is_type_like(&item) && item != "Self" && item != "self" {
                    let module_path = parent_path(&path);
                    if module_path.is_empty() {
                        continue;
                    }
                    let (locs, ambiguous) = self.resolve_locations(
                        from_file,
                        &module_path,
                        None,
                        Some(stmt.byte),
                        Some(&inline_mods),
                        false,
                        mode,
                    );
                    if ambiguous {
                        continue;
                    }
                    let modules = loc_files(&locs);
                    if !modules.is_empty() {
                        type_imports.push(TypeImport {
                            local,
                            type_name: type_base(&item).to_string(),
                            modules,
                            block,
                            module,
                        });
                    }
                    continue;
                }
                let (locs, ambiguous) = self.resolve_locations(
                    from_file,
                    &path,
                    None,
                    Some(stmt.byte),
                    Some(&inline_mods),
                    false,
                    mode,
                );
                if ambiguous {
                    continue;
                }
                let files = loc_files(&locs);
                if !files.is_empty() {
                    aliases.push(Alias {
                        local,
                        files,
                        block,
                        module,
                    });
                }
            }
        }
        FileFacts {
            aliases,
            type_imports,
            inline_mods,
            impls,
            pub_uses,
            test_ranges,
        }
    }

    fn resolve_locations(
        &self,
        from_file: &str,
        path: &str,
        aliases: Option<&[Alias]>,
        call_byte: Option<usize>,
        own_mods: Option<&[InlineMod]>,
        // False while facts are being built: looking up another file's facts
        // would build them, and that build would look this file up again.
        allow_other_facts: bool,
        mode: CallMode,
    ) -> (Vec<Loc>, bool) {
        let segments: Vec<&str> = path.split("::").filter(|s| !s.is_empty()).collect();
        if segments.is_empty() {
            return (Vec::new(), false);
        }
        let mods = |file: &str| -> Vec<InlineMod> {
            if file == from_file {
                if let Some(own) = own_mods {
                    return own.to_vec();
                }
            }
            if !allow_other_facts {
                return Vec::new();
            }
            self.facts_of(file)
                .map(|f| f.inline_mods.clone())
                .unwrap_or_default()
        };
        let mut index = 0usize;
        let mut current: Vec<Loc> = Vec::new();
        let mut ambiguous = false;
        match segments[0] {
            "crate" => {
                current = self
                    .crate_keyword_roots(from_file)
                    .into_iter()
                    .map(Loc::File)
                    .collect();
                index = 1;
            }
            "self" => {
                current = vec![self_loc(from_file, &mods(from_file), call_byte)];
                index = 1;
            }
            "super" => {
                let file_mods = mods(from_file);
                let mut scope = super_scope(&file_mods, call_byte);
                while index < segments.len() && segments[index] == "super" {
                    scope = self.raise_super(from_file, scope, &file_mods);
                    index += 1;
                }
                current = scope_to_locs(from_file, scope);
            }
            other => {
                if let Some(aliases) = aliases {
                    if let Some(files) =
                        pick_alias_files(aliases, other, call_byte, &mods(from_file))
                    {
                        current = files.into_iter().map(Loc::File).collect();
                        index = 1;
                    }
                }
                if current.is_empty() {
                    let file_mods = mods(from_file);
                    let scope = call_byte.and_then(|b| scan::enclosing_inline(&file_mods, b));
                    if let Some(child) = inline_child_visible(&file_mods, scope, other) {
                        current = vec![Loc::Inline {
                            file: from_file.to_string(),
                            index: child,
                        }];
                        index = 1;
                    } else if scope.is_none() {
                        let (child, amb) = self.child_modules(from_file, other, mode);
                        ambiguous |= amb;
                        if !child.is_empty() {
                            current = child.into_iter().map(Loc::File).collect();
                            index = 1;
                        } else if amb {
                            return (Vec::new(), true);
                        } else if let Some(root) = self.extern_or_self_crate(from_file, other) {
                            current = vec![Loc::File(root)];
                            index = 1;
                        } else {
                            return (Vec::new(), false);
                        }
                    } else if let Some(root) = self.extern_or_self_crate(from_file, other) {
                        // Rust 2018 : un nom de crate externe est absolu, même dans un module enfant.
                        // Sans cela, `use code_explorer_core::graph::KnowledgeGraph` écrit dans
                        // `mod tests` ne se lie pas, et l'héritage du parent est refusé.
                        current = vec![Loc::File(root)];
                        index = 1;
                    } else {
                        return (Vec::new(), ambiguous);
                    }
                }
            }
        }
        if current.is_empty() {
            return (Vec::new(), ambiguous);
        }
        while index < segments.len() {
            let seg = segments[index];
            if seg.trim_start().starts_with('<') {
                break;
            }
            let (next, amb) = self.step_locs(&current, seg, &mods, mode);
            ambiguous |= amb;
            if next.is_empty() {
                return (Vec::new(), ambiguous);
            }
            current = next;
            index += 1;
        }
        (current, ambiguous)
    }

    fn raise_super(&self, from_file: &str, scope: SuperScope, mods: &[InlineMod]) -> SuperScope {
        match scope {
            SuperScope::Inline(idx) => match mods.get(idx).and_then(|m| m.parent) {
                Some(p) => SuperScope::Inline(p),
                None => SuperScope::File,
            },
            SuperScope::File => SuperScope::Files(self.parents(from_file)),
            SuperScope::Files(files) => {
                SuperScope::Files(unique_extend(&files, |file| self.parents(file)))
            }
        }
    }

    fn step_locs(
        &self,
        current: &[Loc],
        seg: &str,
        mods_of: &impl Fn(&str) -> Vec<InlineMod>,
        mode: CallMode,
    ) -> (Vec<Loc>, bool) {
        let mut out = Vec::new();
        let mut ambiguous = false;
        for loc in current {
            match loc {
                Loc::File(file) => {
                    let file_mods = mods_of(file);
                    if let Some(child) = inline_child(&file_mods, None, seg) {
                        out.push(Loc::Inline {
                            file: file.clone(),
                            index: child,
                        });
                    }
                    let (kids, amb) = self.child_modules(file, seg, mode);
                    ambiguous |= amb;
                    for child in kids {
                        out.push(Loc::File(child));
                    }
                }
                Loc::Inline { file, index } => {
                    let file_mods = mods_of(file);
                    if let Some(child) = inline_child(&file_mods, Some(*index), seg) {
                        out.push(Loc::Inline {
                            file: file.clone(),
                            index: child,
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| loc_key(a).cmp(&loc_key(b)));
        out.dedup_by(|a, b| loc_key(a) == loc_key(b));
        (out, ambiguous)
    }

    fn lookup_locs(
        &self,
        locs: &[Loc],
        called_name: &str,
        symbols: &SymbolTable,
        depth: usize,
        seen: &mut HashSet<(String, String)>,
    ) -> Vec<Arc<SymbolDefinition>> {
        let mut out = Vec::new();
        let mut ids = HashSet::new();
        for loc in locs {
            match loc {
                Loc::Inline { file, index } => {
                    let listed = self.facts.borrow().get(file).is_some_and(|f| {
                        f.inline_mods
                            .get(*index)
                            .is_some_and(|m| m.functions.iter().any(|n| n == called_name))
                    });
                    if listed {
                        push_callables(symbols, file, called_name, &mut out, &mut ids);
                    }
                }
                Loc::File(file) => {
                    let before = out.len();
                    push_callables(symbols, file, called_name, &mut out, &mut ids);
                    if out.len() == before {
                        out.extend(self.follow_pub_use(file, called_name, symbols, depth, seen, &mut ids));
                    }
                }
            }
        }
        out
    }

    fn follow_pub_use(
        &self,
        file: &str,
        called_name: &str,
        symbols: &SymbolTable,
        depth: usize,
        seen: &mut HashSet<(String, String)>,
        ids: &mut HashSet<String>,
    ) -> Vec<Arc<SymbolDefinition>> {
        if depth >= PUB_USE_MAX_DEPTH || !seen.insert((file.to_string(), called_name.to_string())) {
            return Vec::new();
        }
        let Some(facts) = self.facts_of(file) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for re in &facts.pub_uses {
            let want = re.exported == called_name || re.exported == "*";
            if !want {
                continue;
            }
            let item = if re.item == "*" {
                called_name
            } else {
                re.item.as_str()
            };
            let (locs, ambiguous) = self.resolve_locations(
                file,
                &re.module_path,
                Some(&facts.aliases),
                None,
                Some(&facts.inline_mods),
                true,
                CallMode::Production,
            );
            if ambiguous {
                continue;
            }
            for loc in loc_files(&locs) {
                let before = out.len();
                push_callables(symbols, &loc, item, &mut out, ids);
                if out.len() == before && item == called_name {
                    out.extend(self.follow_pub_use(&loc, called_name, symbols, depth + 1, seen, ids));
                }
            }
        }
        out
    }

    fn crate_keyword_roots(&self, from_file: &str) -> Vec<String> {
        let Some(pkg) = self.owner(from_file) else {
            return Vec::new();
        };
        let rel = from_file
            .strip_prefix(&format!("{}/", pkg.src))
            .unwrap_or("");
        if rel == "main.rs" {
            return self.existing(&[format!("{}/main.rs", pkg.src)]);
        }
        if let Some(rest) = rel.strip_prefix("bin/") {
            let name = rest.split('/').next().unwrap_or("");
            if name.is_empty() {
                return Vec::new();
            }
            return self.existing(&[
                format!("{}/bin/{name}.rs", pkg.src),
                format!("{}/bin/{name}/main.rs", pkg.src),
            ]);
        }
        let lib = format!("{}/lib.rs", pkg.src);
        let main = format!("{}/main.rs", pkg.src);
        if self.files.contains(&lib) {
            vec![lib]
        } else if self.files.contains(&main) {
            vec![main]
        } else {
            Vec::new()
        }
    }

    fn extern_or_self_crate(&self, from_file: &str, name: &str) -> Option<String> {
        let pkg = self.owner(from_file)?;
        if let Some((_, dep_dir)) = pkg.deps.iter().find(|(dep, _)| dep == name) {
            let dep_pkg = self.packages.iter().find(|p| &p.dir == dep_dir)?;
            return self.lib_root(dep_pkg);
        }
        if pkg.name == name {
            return self.lib_root(pkg);
        }
        None
    }

    fn lib_root(&self, pkg: &Package) -> Option<String> {
        let lib = format!("{}/lib.rs", pkg.src);
        let main = format!("{}/main.rs", pkg.src);
        if self.files.contains(&lib) {
            Some(lib)
        } else if self.files.contains(&main) {
            Some(main)
        } else {
            None
        }
    }

    fn owner(&self, file: &str) -> Option<&Package> {
        self.packages
            .iter()
            .filter(|pkg| pkg.dir.is_empty() || file.starts_with(&format!("{}/", pkg.dir)))
            .max_by_key(|pkg| pkg.dir.len())
    }

    fn parents(&self, file: &str) -> Vec<String> {
        if is_crate_root_file(file) {
            return Vec::new();
        }
        if file.ends_with("/mod.rs") || file == "mod.rs" {
            let dir = parent_dir(file);
            return self.dir_as_module(&parent_dir(&dir));
        }
        self.dir_as_module(&parent_dir(file))
    }

    fn dir_as_module(&self, dir: &str) -> Vec<String> {
        if dir.is_empty() {
            return self.existing(&["lib.rs".to_string(), "main.rs".to_string()]);
        }
        if dir == "src" || dir.ends_with("/src") {
            return self.existing(&[format!("{dir}/lib.rs"), format!("{dir}/main.rs")]);
        }
        let name = dir.rsplit('/').next().unwrap_or(dir);
        let parent = parent_dir(dir);
        let rs = if parent.is_empty() {
            format!("{name}.rs")
        } else {
            format!("{parent}/{name}.rs")
        };
        self.existing(&[rs, format!("{dir}/mod.rs")])
    }

    fn mod_decls_of(&self, module_file: &str) -> Option<Vec<ModDecl>> {
        if let Some(hit) = self.mod_decl_cache.borrow().get(module_file) {
            return Some(hit.clone());
        }
        let content = self.content_of(module_file)?;
        let decls = mod_decls(&content);
        self.mod_decl_cache
            .borrow_mut()
            .insert(module_file.to_string(), decls.clone());
        Some(decls)
    }

    /// Fichiers du module `name` déclaré dans `module_file`.
    /// Le booléen est vrai quand plusieurs déclarations restent possibles :
    /// l'appelant ne doit pas émettre de lien à 0,95.
    fn child_modules(&self, module_file: &str, name: &str, mode: CallMode) -> (Vec<String>, bool) {
        if name.is_empty() || name == "crate" || name == "self" || name == "super" {
            return (Vec::new(), false);
        }
        let Some(decls) = self.mod_decls_of(module_file) else {
            return (self.default_children(module_file, name), false);
        };
        let mine: Vec<&ModDecl> = decls
            .iter()
            .filter(|decl| !decl.nested && decl.name == name)
            .collect();
        if mine.is_empty() {
            return (self.default_children(module_file, name), false);
        }
        let mut active: Vec<&ModDecl> = Vec::new();
        let mut uncertain: Vec<&ModDecl> = Vec::new();
        for decl in mine {
            match (decl.cfg, mode) {
                (CfgKind::Test, CallMode::Production) => {}
                (CfgKind::NotTest, CallMode::Test) => {}
                (CfgKind::Other, _) => uncertain.push(decl),
                _ => active.push(decl),
            }
        }
        if !uncertain.is_empty() && active.len() + uncertain.len() != 1 {
            let mut both = active;
            both.extend(uncertain);
            return (self.files_of_decls(module_file, name, &both), true);
        }
        let chosen: Vec<&ModDecl> = if uncertain.is_empty() {
            active
        } else if active.len() == 1 {
            active
        } else {
            uncertain
        };
        match chosen.len() {
            0 => (Vec::new(), false),
            1 => (self.files_of_decls(module_file, name, &chosen), false),
            _ => (self.files_of_decls(module_file, name, &chosen), true),
        }
    }

    fn files_of_decls(&self, module_file: &str, name: &str, decls: &[&ModDecl]) -> Vec<String> {
        let mut out = Vec::new();
        for decl in decls {
            if let Some(path) = &decl.path {
                let dir = parent_dir(module_file);
                if let Some(norm) = normalize_rel(&join_rel(&dir, path)) {
                    out.extend(self.existing(&[norm]));
                }
            } else {
                out.extend(self.default_children(module_file, name));
            }
        }
        out.sort();
        out.dedup();
        out
    }

    fn default_children(&self, module_file: &str, name: &str) -> Vec<String> {
        let base = if is_dir_module(module_file) {
            parent_dir(module_file)
        } else {
            module_file.trim_end_matches(".rs").to_string()
        };
        let rs = if base.is_empty() {
            format!("{name}.rs")
        } else {
            format!("{base}/{name}.rs")
        };
        let mod_rs = if base.is_empty() {
            format!("{name}/mod.rs")
        } else {
            format!("{base}/{name}/mod.rs")
        };
        self.existing(&[rs, mod_rs])
    }

    fn existing(&self, candidates: &[String]) -> Vec<String> {
        let mut out: Vec<String> = candidates
            .iter()
            .filter(|c| self.files.contains(c.as_str()))
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

enum SuperScope {
    Inline(usize),
    File,
    Files(Vec<String>),
}

struct UseAt {
    byte: usize,
    is_pub: bool,
    text: String,
}

fn super_scope(mods: &[InlineMod], byte: Option<usize>) -> SuperScope {
    match byte.and_then(|b| scan::enclosing_inline(mods, b)) {
        Some(idx) => SuperScope::Inline(idx),
        None => SuperScope::File,
    }
}

fn scope_to_locs(from_file: &str, scope: SuperScope) -> Vec<Loc> {
    match scope {
        SuperScope::Inline(index) => vec![Loc::Inline {
            file: from_file.to_string(),
            index,
        }],
        SuperScope::File => vec![Loc::File(from_file.to_string())],
        SuperScope::Files(files) => files.into_iter().map(Loc::File).collect(),
    }
}

fn self_loc(from_file: &str, mods: &[InlineMod], byte: Option<usize>) -> Loc {
    match byte.and_then(|b| scan::enclosing_inline(mods, b)) {
        Some(index) => Loc::Inline {
            file: from_file.to_string(),
            index,
        },
        None => Loc::File(from_file.to_string()),
    }
}

fn inline_child(mods: &[InlineMod], parent: Option<usize>, name: &str) -> Option<usize> {
    mods.iter()
        .position(|m| m.parent == parent && m.name == name)
}

fn loc_files(locs: &[Loc]) -> Vec<String> {
    let mut out: Vec<String> = locs
        .iter()
        .map(|loc| match loc {
            Loc::File(file) | Loc::Inline { file, .. } => file.clone(),
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn loc_key(loc: &Loc) -> String {
    match loc {
        Loc::File(file) => format!("f:{file}"),
        Loc::Inline { file, index } => format!("i:{file}:{index}"),
    }
}

fn self_encloses(impls: &[ImplBlock], byte: Option<usize>, method: &str) -> bool {
    let Some(byte) = byte else {
        return false;
    };
    let Some(enclosing) = impls.iter().find(|b| b.start <= byte && byte < b.end) else {
        return false;
    };
    if enclosing.methods.iter().any(|m| m == method) {
        return true;
    }
    // `impl Default for Store { fn default() { Self::new() } }` calls the
    // inherent `new` of Store, which lives in another impl of the same file.
    !enclosing.owner_type.is_empty() && file_has_method(impls, &enclosing.owner_type, method)
}

fn parent_path(path: &str) -> String {
    match path.rfind("::") {
        Some(idx) => path[..idx].to_string(),
        None => String::new(),
    }
}

fn push_callables(
    symbols: &SymbolTable,
    file: &str,
    name: &str,
    out: &mut Vec<Arc<SymbolDefinition>>,
    seen: &mut HashSet<String>,
) {
    if let Some(defs) = symbols.lookup_in_file(file, name) {
        for def in defs {
            if def.symbol_type.is_callable() && seen.insert(def.node_id.clone()) {
                out.push(Arc::clone(def));
            }
        }
    }
}

fn decision_of(
    targets: &[Arc<SymbolDefinition>],
    qualifier: &str,
    called_name: &str,
    kind: &str,
) -> RustCallDecision {
    match targets.len() {
        0 => RustCallDecision::Unresolved,
        1 => RustCallDecision::Link {
            node_id: targets[0].node_id.clone(),
            reason: format!("{kind}:{qualifier}::{called_name}"),
        },
        n => RustCallDecision::Ambiguous {
            note: format!(
                "{qualifier}::{called_name} ({n} candidates: {})",
                targets
                    .iter()
                    .map(|d| d.file_path.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    }
}

fn link_same_file(
    symbols: &SymbolTable,
    file: &str,
    name: &str,
) -> Option<RustCallDecision> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    push_callables(symbols, file, name, &mut targets, &mut seen);
    match targets.len() {
        0 => None,
        1 => Some(RustCallDecision::Link {
            node_id: targets[0].node_id.clone(),
            reason: "same-file".to_string(),
        }),
        n => Some(RustCallDecision::Ambiguous {
            note: format!("same-file {name} ({n} candidates)"),
        }),
    }
}

fn use_keyword_byte(raw_line: &str, line_start: usize) -> usize {
    let b = raw_line.as_bytes();
    let mut i = 0usize;
    while i + 3 <= b.len() {
        let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        if raw_line[i..].starts_with("use")
            && (i + 3 == b.len() || !ident(b[i + 3]))
            && (i == 0 || !ident(b[i - 1]))
        {
            return line_start + i;
        }
        i += 1;
    }
    line_start
}

/// Higher score wins. A block binding outranks a module binding.
/// `None` means the binding is not visible at this call.
fn scope_score(
    block: Option<(usize, usize)>,
    module: Option<usize>,
    call_byte: Option<usize>,
    mods: &[InlineMod],
) -> Option<i64> {
    match call_byte {
        None => {
            if block.is_some() {
                return None;
            }
            Some(module_depth(mods, module) as i64)
        }
        Some(byte) => {
            if let Some((start, end)) = block {
                if byte >= start && byte < end {
                    let span = end.saturating_sub(start) as i64;
                    return Some(1_000_000_000 - span);
                }
                return None;
            }
            if !module_sees(mods, scan::enclosing_inline(mods, byte), module) {
                return None;
            }
            Some(module_depth(mods, module) as i64)
        }
    }
}

fn module_depth(mods: &[InlineMod], module: Option<usize>) -> usize {
    let mut depth = 0usize;
    let mut cur = module;
    while let Some(idx) = cur {
        depth += 1;
        if depth > 64 {
            break;
        }
        cur = mods.get(idx).and_then(|m| m.parent);
    }
    depth
}

/// Un `use` est visible dans le module où il est écrit. Un enfant ne le voit
/// que si chaque module entre l'appel et ce `use` a `use super::*`.
/// Un `use` de l'enfant n'est pas visible dans le parent ni dans un frère.
fn module_sees(mods: &[InlineMod], call_mod: Option<usize>, use_mod: Option<usize>) -> bool {
    if call_mod == use_mod {
        return true;
    }
    let mut cur = call_mod;
    while let Some(idx) = cur {
        if !mods.get(idx).is_some_and(|m| m.imports_super_glob) {
            return false;
        }
        let parent = mods.get(idx).and_then(|m| m.parent);
        if parent == use_mod {
            return true;
        }
        cur = parent;
    }
    false
}

/// L'item local (espace des types) masque un `use` hérité, y compris via `use super::*`.
fn name_shadowed(
    mods: &[InlineMod],
    call_byte: Option<usize>,
    use_mod: Option<usize>,
    name: &str,
) -> bool {
    let Some(byte) = call_byte else {
        return false;
    };
    let Some(mut cur) = scan::enclosing_inline(mods, byte) else {
        return false;
    };
    loop {
        if mods
            .get(cur)
            .is_some_and(|m| m.items.iter().any(|item| item == name))
        {
            return true;
        }
        if Some(cur) == use_mod {
            return false;
        }
        let Some(parent) = mods.get(cur).and_then(|m| m.parent) else {
            return false;
        };
        if Some(parent) == use_mod {
            return false;
        }
        cur = parent;
    }
}

fn inline_child_visible(mods: &[InlineMod], scope: Option<usize>, name: &str) -> Option<usize> {
    let mut look = scope;
    loop {
        if let Some(child) = inline_child(mods, look, name) {
            return Some(child);
        }
        match look {
            Some(idx) if mods.get(idx).is_some_and(|m| m.imports_super_glob) => {
                look = mods[idx].parent;
            }
            _ => return None,
        }
    }
}

fn call_mode_at(ranges: &[(usize, usize)], byte: Option<usize>) -> CallMode {
    let Some(byte) = byte else {
        return CallMode::Production;
    };
    if ranges
        .iter()
        .any(|(start, end)| *start <= byte && byte < *end)
    {
        CallMode::Test
    } else {
        CallMode::Production
    }
}

fn imports_parent_glob(stmt: &str) -> bool {
    binding_paths(stmt)
        .iter()
        .any(|(local, path)| local == "*" && path == "super::*")
}

fn pick_alias_files(
    aliases: &[Alias],
    name: &str,
    call_byte: Option<usize>,
    mods: &[InlineMod],
) -> Option<Vec<String>> {
    let mut best: Option<i64> = None;
    let mut files: Vec<&[String]> = Vec::new();
    for alias in aliases {
        if alias.local != name {
            continue;
        }
        let Some(score) = scope_score(alias.block, alias.module, call_byte, mods) else {
            continue;
        };
        if name_shadowed(mods, call_byte, alias.module, name) {
            continue;
        }
        match best {
            Some(prev) if score < prev => {}
            Some(prev) if score == prev => files.push(alias.files.as_slice()),
            _ => {
                best = Some(score);
                files.clear();
                files.push(alias.files.as_slice());
            }
        }
    }
    let first = *files.first()?;
    if files.iter().any(|found| *found != first) {
        return None;
    }
    Some(first.to_vec())
}

fn visible_type_imports<'a>(
    imports: &'a [TypeImport],
    local: &str,
    call_byte: Option<usize>,
    mods: &[InlineMod],
) -> Vec<&'a TypeImport> {
    let mut best: Option<i64> = None;
    let mut chosen: Vec<&TypeImport> = Vec::new();
    for import in imports {
        if import.local != local {
            continue;
        }
        let Some(score) = scope_score(import.block, import.module, call_byte, mods) else {
            continue;
        };
        if name_shadowed(mods, call_byte, import.module, local) {
            continue;
        }
        match best {
            Some(prev) if score < prev => {}
            Some(prev) if score == prev => chosen.push(import),
            _ => {
                best = Some(score);
                chosen.clear();
                chosen.push(import);
            }
        }
    }
    if chosen.len() > 1 {
        let modules = &chosen[0].modules;
        let type_name = &chosen[0].type_name;
        if chosen
            .iter()
            .any(|import| &import.modules != modules || &import.type_name != type_name)
        {
            return Vec::new();
        }
    }
    chosen
}

fn use_statements_at(content: &str) -> Vec<UseAt> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_use = false;
    let mut byte = 0usize;
    let mut is_pub = false;
    let mut offset = 0usize;
    for raw in content.split_inclusive('\n') {
        let line_start = offset;
        offset += raw.len();
        let raw_line = raw.trim_end_matches(['\n', '\r']);
        let line = strip_line_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if !in_use {
            let vis = strip_visibility(line);
            let public = line.trim_start().starts_with("pub");
            if let Some(rest) = vis.strip_prefix("use ") {
                in_use = true;
                is_pub = public;
                byte = use_keyword_byte(raw_line, line_start);
                buf = rest.trim().to_string();
            } else if vis == "use" {
                in_use = true;
                is_pub = public;
                byte = use_keyword_byte(raw_line, line_start);
                buf.clear();
            }
        } else {
            if !buf.is_empty() {
                buf.push(' ');
            }
            buf.push_str(line);
        }
        if in_use && buf.contains(';') {
            let stmt = buf.split(';').next().unwrap_or("").trim().to_string();
            if !stmt.is_empty() {
                out.push(UseAt {
                    byte,
                    is_pub,
                    text: stmt,
                });
            }
            in_use = false;
            buf.clear();
        }
    }
    out
}

fn unique_extend(current: &[String], mut next_of: impl FnMut(&str) -> Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for file in current {
        out.extend(next_of(file));
    }
    out.sort();
    out.dedup();
    out
}

fn is_dir_module(file: &str) -> bool {
    file.ends_with("/mod.rs")
        || file.ends_with("/lib.rs")
        || file.ends_with("/main.rs")
        || file == "mod.rs"
        || file == "lib.rs"
        || file == "main.rs"
}

fn is_crate_root_file(file: &str) -> bool {
    file.ends_with("/lib.rs") || file.ends_with("/main.rs") || file == "lib.rs" || file == "main.rs"
}

fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(idx) => path[..idx].to_string(),
        None => String::new(),
    }
}

fn rust_extern_name(name: &str) -> String {
    name.trim().replace('-', "_")
}

/// `crate::…`, `self::…`, `super::…`, or a multi-segment module path.
/// A bare type name (`S`, `Self`, `Vec`) is not path-shaped. `Self` is not `self`.
/// Turbofish (`Vec::<u8>`) is a type, not a module path.
fn qualifier_is_path_shaped(qualifier: &str) -> bool {
    let first = qualifier
        .split("::")
        .find(|seg| !seg.is_empty())
        .unwrap_or("");
    let has_module_sep = qualifier.split("::").skip(1).any(|seg| {
        let seg = seg.trim();
        !seg.is_empty() && !seg.starts_with('<')
    });
    has_module_sep || matches!(first, "crate" | "self" | "super")
}

fn ingest_manifest(
    repo: &Path,
    dir: &str,
    files: &HashSet<String>,
    packages: &mut Vec<Package>,
    seen: &mut HashSet<String>,
    inherit: &WsInherit,
) {
    if !seen.insert(dir.to_string()) {
        return;
    }
    let manifest = if dir.is_empty() {
        repo.join("Cargo.toml")
    } else {
        repo.join(dir).join("Cargo.toml")
    };
    let Ok(text) = fs::read_to_string(&manifest) else {
        return;
    };
    let info = parse_cargo_toml(&text);
    let table = if info.is_workspace {
        WsInherit {
            root: dir.to_string(),
            deps: info.workspace_deps.clone(),
        }
    } else {
        inherit.clone()
    };
    if let Some(name) = info.package_name {
        let src_prefix = if dir.is_empty() {
            "src/".to_string()
        } else {
            format!("{dir}/src/")
        };
        let src = if files.iter().any(|f| f.starts_with(&src_prefix)) {
            if dir.is_empty() {
                "src".to_string()
            } else {
                format!("{dir}/src")
            }
        } else {
            dir.to_string()
        };
        let mut deps = Vec::new();
        for (dep_name, dep_path) in &info.path_deps {
            if let Some(dep_dir) = normalize_rel(&join_rel(dir, dep_path)) {
                deps.push((rust_extern_name(dep_name), dep_dir));
            }
        }
        for dep_name in &info.workspace_inherited {
            let Some((_, rel)) = table
                .deps
                .iter()
                .find(|(n, _)| rust_extern_name(n) == rust_extern_name(dep_name))
            else {
                continue;
            };
            if let Some(dep_dir) = normalize_rel(&join_rel(&table.root, rel)) {
                deps.push((rust_extern_name(dep_name), dep_dir));
            }
        }
        let dep_dirs: Vec<String> = deps.iter().map(|(_, d)| d.clone()).collect();
        packages.push(Package {
            name: rust_extern_name(&name),
            dir: dir.to_string(),
            src,
            deps,
        });
        for dep_dir in dep_dirs {
            ingest_manifest(repo, &dep_dir, files, packages, seen, &table);
        }
    }
    let members = expand_member_patterns(repo, dir, &info.members);
    let mut excluded = expand_member_patterns(repo, dir, &info.exclude);
    for pat in &info.exclude {
        if !is_glob(pat) {
            if let Some(norm) = normalize_rel(&join_rel(dir, pat)) {
                if !excluded.contains(&norm) {
                    excluded.push(norm);
                }
            }
        }
    }
    for member_dir in members {
        if excluded.iter().any(|e| e == &member_dir) {
            continue;
        }
        ingest_manifest(repo, &member_dir, files, packages, seen, &table);
    }
}

fn expand_member_patterns(repo: &Path, base: &str, patterns: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for pat in patterns {
        let full = join_rel(base, pat);
        let Some(norm) = normalize_rel(&full) else {
            continue;
        };
        if !is_glob(&norm) {
            let manifest = if norm.is_empty() {
                repo.join("Cargo.toml")
            } else {
                repo.join(&norm).join("Cargo.toml")
            };
            if manifest.is_file() {
                out.push(norm);
            }
            continue;
        }
        walk_glob(repo, "", &norm, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn walk_glob(repo: &Path, rel: &str, pattern: &str, out: &mut Vec<String>) {
    let dir = if rel.is_empty() {
        repo.to_path_buf()
    } else {
        repo.join(rel)
    };
    let Ok(rd) = fs::read_dir(&dir) else {
        return;
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if !ent.path().is_dir() {
            continue;
        }
        let child = if rel.is_empty() {
            name.to_string()
        } else {
            format!("{rel}/{name}")
        };
        if glob_match(pattern, &child) && ent.path().join("Cargo.toml").is_file() {
            out.push(child.clone());
        }
        if glob_may_descend(pattern, &child) {
            walk_glob(repo, &child, pattern, out);
        }
    }
}

struct CargoInfo {
    package_name: Option<String>,
    members: Vec<String>,
    exclude: Vec<String>,
    path_deps: Vec<(String, String)>,
    workspace_deps: Vec<(String, String)>,
    workspace_inherited: Vec<String>,
    is_workspace: bool,
}

enum ArrKind {
    Members,
    Exclude,
}

enum DepKind {
    Package,
    Workspace,
}

fn parse_cargo_toml(text: &str) -> CargoInfo {
    let mut section = String::new();
    let mut package_name = None;
    let mut members = Vec::new();
    let mut exclude = Vec::new();
    let mut path_deps = Vec::new();
    let mut workspace_deps = Vec::new();
    let mut workspace_inherited = Vec::new();
    let mut is_workspace = false;
    let mut arr_buf: Option<(ArrKind, String)> = None;
    let mut dep_buf: Option<(DepKind, String, String)> = None;

    for raw in text.lines() {
        let line = strip_hash_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            flush_arr(&mut arr_buf, &mut members, &mut exclude);
            flush_dep_kind(
                &mut dep_buf,
                &mut path_deps,
                &mut workspace_deps,
                &mut workspace_inherited,
            );
            if let Some(end) = line.find(']') {
                section = line[1..end].trim().to_string();
            }
            if section == "workspace" || section.starts_with("workspace.") {
                is_workspace = true;
            }
            continue;
        }
        if let Some((_, buf)) = arr_buf.as_mut() {
            buf.push(' ');
            buf.push_str(line);
            if buf.contains(']') {
                flush_arr(&mut arr_buf, &mut members, &mut exclude);
            }
            continue;
        }
        if let Some((_, _, buf)) = dep_buf.as_mut() {
            buf.push(' ');
            buf.push_str(line);
            if buf.contains('}') {
                flush_dep_kind(
                    &mut dep_buf,
                    &mut path_deps,
                    &mut workspace_deps,
                    &mut workspace_inherited,
                );
            }
            continue;
        }
        if section == "package" {
            if let Some(name) = assign_string(line, "name") {
                package_name = Some(name);
            }
            continue;
        }
        if section == "workspace" && (line.starts_with("members") || line.starts_with("exclude")) {
            let kind = if line.starts_with("exclude") {
                ArrKind::Exclude
            } else {
                ArrKind::Members
            };
            if line.contains(']') {
                let dest = match kind {
                    ArrKind::Members => &mut members,
                    ArrKind::Exclude => &mut exclude,
                };
                dest.extend(quoted_strings(line));
            } else if line.contains('[') {
                arr_buf = Some((kind, line.to_string()));
            }
            continue;
        }
        if section == "workspace.dependencies" || is_dep_section(&section) {
            let kind = if section == "workspace.dependencies" {
                DepKind::Workspace
            } else {
                DepKind::Package
            };
            if let Some((key, value)) = split_assign(line) {
                if value.contains('}') || !value.contains('{') {
                    record_dep(
                        kind,
                        &key,
                        &value,
                        &mut path_deps,
                        &mut workspace_deps,
                        &mut workspace_inherited,
                    );
                } else {
                    dep_buf = Some((kind, key, value));
                }
            }
        }
    }
    flush_arr(&mut arr_buf, &mut members, &mut exclude);
    flush_dep_kind(
        &mut dep_buf,
        &mut path_deps,
        &mut workspace_deps,
        &mut workspace_inherited,
    );
    CargoInfo {
        package_name,
        members,
        exclude,
        path_deps,
        workspace_deps,
        workspace_inherited,
        is_workspace,
    }
}

fn is_dep_section(section: &str) -> bool {
    if section.starts_with("workspace.") {
        return false;
    }
    section == "dependencies"
        || section == "dev-dependencies"
        || section == "build-dependencies"
        || section.ends_with(".dependencies")
}

fn flush_arr(
    buf: &mut Option<(ArrKind, String)>,
    members: &mut Vec<String>,
    exclude: &mut Vec<String>,
) {
    if let Some((kind, text)) = buf.take() {
        let dest = match kind {
            ArrKind::Members => members,
            ArrKind::Exclude => exclude,
        };
        dest.extend(quoted_strings(&text));
    }
}

fn flush_dep_kind(
    buf: &mut Option<(DepKind, String, String)>,
    path_deps: &mut Vec<(String, String)>,
    workspace_deps: &mut Vec<(String, String)>,
    workspace_inherited: &mut Vec<String>,
) {
    if let Some((kind, key, value)) = buf.take() {
        record_dep(
            kind,
            &key,
            &value,
            path_deps,
            workspace_deps,
            workspace_inherited,
        );
    }
}

fn record_dep(
    kind: DepKind,
    key: &str,
    value: &str,
    path_deps: &mut Vec<(String, String)>,
    workspace_deps: &mut Vec<(String, String)>,
    workspace_inherited: &mut Vec<String>,
) {
    if let Some(name) = key.strip_suffix(".workspace") {
        if value.trim() == "true" && matches!(kind, DepKind::Package) {
            workspace_inherited.push(name.to_string());
        }
        return;
    }
    if let Some(path) = path_of(value) {
        match kind {
            DepKind::Package => path_deps.push((key.to_string(), path)),
            DepKind::Workspace => workspace_deps.push((key.to_string(), path)),
        }
        return;
    }
    if matches!(kind, DepKind::Package) && value.replace(' ', "").contains("workspace=true") {
        workspace_inherited.push(key.to_string());
    }
}

fn strip_hash_comment(line: &str) -> &str {
    let mut in_str = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_str = !in_str,
            b'#' if !in_str => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

fn assign_string(line: &str, key: &str) -> Option<String> {
    let (k, v) = split_assign(line)?;
    if k != key {
        return None;
    }
    quoted_strings(&v).into_iter().next()
}

fn split_assign(line: &str) -> Option<(String, String)> {
    let eq = line.find('=')?;
    let key = line[..eq].trim().trim_matches('"').to_string();
    if key.is_empty() || key.contains(' ') {
        return None;
    }
    Some((key, line[eq + 1..].trim().to_string()))
}

fn path_of(value: &str) -> Option<String> {
    let marker = "path";
    let idx = value.find(marker)?;
    let after = value[idx + marker.len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    quoted_strings(after).into_iter().next()
}

fn quoted_strings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('"') {
            out.push(after[..end].to_string());
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    out
}

fn join_rel(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

fn normalize_rel(path: &str) -> Option<String> {
    let mut stack = Vec::new();
    for seg in path.split(['/', '\\']) {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            if stack.pop().is_none() {
                return None;
            }
            continue;
        }
        stack.push(seg);
    }
    Some(stack.join("/"))
}

fn strip_line_comment(line: &str) -> &str {
    match line.find("//") {
        Some(idx) => &line[..idx],
        None => line,
    }
}

fn strip_visibility(line: &str) -> &str {
    let line = line.trim();
    let Some(rest) = line.strip_prefix("pub") else {
        return line;
    };
    let rest = rest.trim_start();
    if let Some(rest) = rest.strip_prefix('(') {
        if let Some(end) = rest.find(')') {
            return rest[end + 1..].trim_start();
        }
    }
    rest
}

fn binding_paths(stmt: &str) -> Vec<(String, String)> {
    let stmt = stmt.trim().trim_end_matches(';').trim();
    if let Some(open) = stmt.find('{') {
        let Some(close) = stmt.rfind('}') else {
            return Vec::new();
        };
        if close < open {
            return Vec::new();
        }
        let prefix = stmt[..open].trim().trim_end_matches("::").trim();
        let inner = &stmt[open + 1..close];
        let mut out = Vec::new();
        for part in split_top_commas(inner) {
            let part = part.trim();
            if part.is_empty() || part.contains('{') {
                continue;
            }
            let (tail, alias) = split_as(part);
            if tail == "self" {
                let local = alias.unwrap_or_else(|| last_segment(prefix).to_string());
                if !prefix.is_empty() {
                    out.push((local, prefix.to_string()));
                }
                continue;
            }
            let full = if prefix.is_empty() {
                tail.to_string()
            } else if tail.is_empty() {
                prefix.to_string()
            } else {
                format!("{prefix}::{tail}")
            };
            let local = alias.unwrap_or_else(|| last_segment(&full).to_string());
            if !local.is_empty() && !full.is_empty() {
                out.push((local, full));
            }
        }
        return out;
    }
    let (path, alias) = split_as(stmt);
    let local = alias.unwrap_or_else(|| last_segment(path).to_string());
    if local.is_empty() || path.is_empty() {
        return Vec::new();
    }
    vec![(local, path.to_string())]
}

fn split_as(part: &str) -> (&str, Option<String>) {
    if let Some(idx) = part.rfind(" as ") {
        let path = part[..idx].trim();
        let alias = part[idx + 4..].trim();
        if !alias.is_empty() {
            return (path, Some(alias.to_string()));
        }
    }
    (part.trim(), None)
}

fn split_top_commas(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in input.char_indices() {
        match ch {
            '{' | '<' | '(' => depth += 1,
            '}' | '>' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&input[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start <= input.len() {
        parts.push(&input[start..]);
    }
    parts
}

fn last_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path).trim()
}

/// Explicitly typed parameters of parenthesized C# lambdas.
/// `(AnswerCache cache) =>` yields `("cache", "AnswerCache")`.
pub fn csharp_lambda_parameters(content: &str) -> Vec<(String, String)> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'=' && bytes[i + 1] == b'>' {
            if let Some((start, end)) = matching_paren_before(content, i) {
                out.extend(params_in(&content[start..end]));
            }
            i += 2;
            continue;
        }
        i += 1;
    }
    out
}

fn matching_paren_before(content: &str, arrow: usize) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut j = arrow;
    while j > 0 && bytes[j - 1].is_ascii_whitespace() {
        j -= 1;
    }
    if j == 0 || bytes[j - 1] != b')' {
        return None;
    }
    let mut depth = 0i32;
    let mut k = j;
    while k > 0 {
        k -= 1;
        match bytes[k] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some((k + 1, j - 1));
                }
            }
            _ => {}
        }
    }
    None
}

fn params_in(list: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in split_top_commas(list) {
        let part = strip_attributes(raw.trim());
        if part.is_empty() {
            continue;
        }
        let mut tokens: Vec<&str> = part.split_whitespace().collect();
        while tokens.first().is_some_and(|t| {
            matches!(
                *t,
                "ref" | "out" | "in" | "params" | "this" | "scoped" | "readonly"
            )
        }) {
            tokens.remove(0);
        }
        if tokens.len() < 2 {
            continue;
        }
        let name = tokens
            .pop()
            .unwrap_or("")
            .trim_end_matches(|c| c == '?' || c == '!');
        let type_token = tokens.pop().unwrap_or("");
        let type_name = type_leaf(type_token);
        if is_ident(name) && is_type_name(type_name) {
            out.push((name.to_string(), type_name.to_string()));
        }
    }
    out
}

fn strip_attributes(mut text: &str) -> &str {
    text = text.trim();
    while text.starts_with('[') {
        let Some(end) = text.find(']') else {
            break;
        };
        text = text[end + 1..].trim();
    }
    text
}

fn type_leaf(token: &str) -> &str {
    let token = token.trim_end_matches(|c| matches!(c, '?' | '!' | ']' | '['));
    token
        .rsplit(['.', ':'])
        .next()
        .unwrap_or(token)
        .trim_end_matches(|c| matches!(c, '?' | '!' | ']' | '['))
}

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn is_type_name(name: &str) -> bool {
    if !is_ident(name) {
        return false;
    }
    match name {
        "var" | "void" | "int" | "string" | "bool" | "long" | "short" | "byte" | "char"
        | "float" | "double" | "decimal" | "object" | "dynamic" | "nint" | "nuint" => false,
        _ => name.chars().next().is_some_and(|c| c.is_ascii_uppercase()),
    }
}

/// C# methods of `type_name` (exact class/struct/interface name, else one
/// leading `I` stripped) named `method_name`. Several distinct nodes stay
/// ambiguous.
pub fn csharp_methods_of_type<'a>(
    symbols: &'a SymbolTable,
    type_name: &str,
    method_name: &str,
) -> Vec<Arc<SymbolDefinition>> {
    let files = class_files(symbols, type_name);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for file in files {
        if let Some(defs) = symbols.lookup_in_file(&file, method_name) {
            for def in defs {
                if def.symbol_type.is_callable() && seen.insert(def.node_id.clone()) {
                    out.push(Arc::clone(def));
                }
            }
        }
    }
    out
}

fn class_files(symbols: &SymbolTable, type_name: &str) -> Vec<String> {
    let exact = files_named(symbols, type_name);
    if !exact.is_empty() {
        return exact;
    }
    if let Some(stripped) = type_name.strip_prefix('I') {
        if stripped
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
        {
            return files_named(symbols, stripped);
        }
    }
    Vec::new()
}

fn files_named(symbols: &SymbolTable, type_name: &str) -> Vec<String> {
    use code_explorer_core::graph::types::NodeLabel;
    let mut files = Vec::new();
    if let Some(defs) = symbols.lookup_global(type_name) {
        for def in defs {
            if matches!(
                def.symbol_type,
                NodeLabel::Class | NodeLabel::Struct | NodeLabel::Interface
            ) {
                files.push(def.file_path.clone());
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_explorer_core::graph::types::NodeLabel;
    use code_explorer_core::symbol::SymbolDefinition;

    fn files(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    fn pkg(name: &str, dir: &str, src: &str, deps: &[(&str, &str)]) -> Package {
        Package {
            name: name.to_string(),
            dir: dir.to_string(),
            src: src.to_string(),
            deps: deps
                .iter()
                .map(|(n, d)| (n.to_string(), d.to_string()))
                .collect(),
        }
    }

    fn add_fn(symbols: &mut SymbolTable, file: &str, name: &str) {
        symbols.add(
            name.to_string(),
            SymbolDefinition {
                node_id: format!("Function:{file}:{name}"),
                file_path: file.to_string(),
                symbol_type: NodeLabel::Function,
                parameter_count: Some(0),
                required_parameter_count: Some(0),
                parameter_types: None,
                return_type: None,
                declared_type: None,
                owner_id: None,
                is_exported: true,
            },
        );
    }

    fn ws_lib() -> RustWorkspace {
        RustWorkspace::new(
            files(&[
                "src/lib.rs",
                "src/transforms/mod.rs",
                "src/transforms/gate.rs",
                "src/transforms/live.rs",
                "src/other.rs",
                "src/dup.rs",
                "src/dup/mod.rs",
                "src/a/child.rs",
            ]),
            vec![pkg("demo", "", "src", &[])],
        )
    }

    #[test]
    fn crate_path_lands_on_gate_not_homonym() {
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/transforms/gate.rs", "reinject");
        add_fn(&mut symbols, "src/other.rs", "reinject");
        let decision = ws.decide(
            "src/transforms/live.rs",
            "",
            "crate::transforms::gate",
            "reinject",
            &symbols,
        );
        assert_eq!(
            decision,
            RustCallDecision::Link {
                node_id: "Function:src/transforms/gate.rs:reinject".into(),
                reason: "rust-path:crate::transforms::gate::reinject".into(),
            }
        );
    }

    #[test]
    fn super_and_self_paths() {
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/transforms/gate.rs", "reinject");
        add_fn(&mut symbols, "src/a/child.rs", "go");
        assert!(matches!(
            ws.decide(
                "src/transforms/live.rs",
                "",
                "super::gate",
                "reinject",
                &symbols
            ),
            RustCallDecision::Link { .. }
        ));
        assert!(matches!(
            ws.decide("src/a.rs", "", "self::child", "go", &symbols),
            RustCallDecision::Link { .. }
        ));
    }

    #[test]
    fn use_alias_resolves_module() {
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/transforms/gate.rs", "reinject");
        let decision = ws.decide(
            "src/lib.rs",
            "use crate::transforms::gate as g;\nfn via() { g::reinject(); }\n",
            "g",
            "reinject",
            &symbols,
        );
        assert_eq!(
            decision,
            RustCallDecision::Link {
                node_id: "Function:src/transforms/gate.rs:reinject".into(),
                reason: "rust-path:g::reinject".into(),
            }
        );
    }

    #[test]
    fn duplicate_module_files_are_ambiguous() {
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/dup.rs", "run");
        add_fn(&mut symbols, "src/dup/mod.rs", "run");
        match ws.decide("src/lib.rs", "", "crate::dup", "run", &symbols) {
            RustCallDecision::Ambiguous { note } => {
                assert!(note.contains("2 candidates"), "{note}");
                assert!(note.contains("src/dup.rs"), "{note}");
                assert!(note.contains("src/dup/mod.rs"), "{note}");
            }
            other => panic!("expected ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn unknown_qualifier_does_not_pick_a_homonym() {
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/other.rs", "reinject");
        assert_eq!(
            ws.decide("src/lib.rs", "", "missing::place", "reinject", &symbols),
            RustCallDecision::Unresolved
        );
    }

    #[test]
    fn type_and_self_are_not_modules() {
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/lib.rs", "build");
        add_fn(&mut symbols, "src/other.rs", "build");
        assert_eq!(
            ws.decide("src/lib.rs", "", "S", "build", &symbols),
            RustCallDecision::NotAModule
        );
        assert_eq!(
            ws.decide("src/lib.rs", "", "Self", "build", &symbols),
            RustCallDecision::NotAModule
        );
    }

    #[test]
    fn malformed_path_stays_unresolved() {
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/lib.rs", "build");
        add_fn(&mut symbols, "src/other.rs", "build");
        assert_eq!(
            ws.decide("src/lib.rs", "", "crate::missing", "build", &symbols),
            RustCallDecision::Unresolved
        );
        assert_eq!(
            ws.decide(
                "src/transforms/live.rs",
                "",
                "super::nope",
                "build",
                &symbols
            ),
            RustCallDecision::Unresolved
        );
    }

    #[test]
    fn resolved_module_without_the_function_does_not_fall_back() {
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/lib.rs", "build");
        add_fn(&mut symbols, "src/transforms/gate.rs", "reinject");
        assert_eq!(
            ws.decide(
                "src/lib.rs",
                "",
                "crate::transforms::gate",
                "build",
                &symbols
            ),
            RustCallDecision::Unresolved
        );
    }

    #[test]
    fn path_dependency_crate_name() {
        let ws = RustWorkspace::new(
            files(&[
                "src/main.rs",
                "crates/core/src/lib.rs",
                "crates/core/src/transforms/mod.rs",
                "crates/core/src/transforms/gate.rs",
                "src/gate.rs",
            ]),
            vec![
                pkg("app", "", "src", &[("core", "crates/core")]),
                pkg("core", "crates/core", "crates/core/src", &[]),
            ],
        );
        let mut symbols = SymbolTable::new();
        add_fn(
            &mut symbols,
            "crates/core/src/transforms/gate.rs",
            "reinject",
        );
        add_fn(&mut symbols, "src/gate.rs", "reinject");
        let decision = ws.decide(
            "src/main.rs",
            "",
            "core::transforms::gate",
            "reinject",
            &symbols,
        );
        assert_eq!(
            decision,
            RustCallDecision::Link {
                node_id: "Function:crates/core/src/transforms/gate.rs:reinject".into(),
                reason: "rust-path:core::transforms::gate::reinject".into(),
            }
        );
    }

    #[test]
    fn parses_package_name_and_path_dep() {
        let info = parse_cargo_toml(
            r#"
            [package]
            name = "lm-resizer"
            [dependencies]
            lm-resizer-core = { path = "crates/lm-resizer-core" }
            serde = "1"
            "#,
        );
        assert_eq!(info.package_name.as_deref(), Some("lm-resizer"));
        assert_eq!(
            info.path_deps,
            vec![("lm-resizer-core".into(), "crates/lm-resizer-core".into())]
        );
    }

    #[test]
    fn lambda_parameters_keep_explicit_types() {
        let params = csharp_lambda_parameters(
            r#"
            app.MapPost("/feedback", async (AnswerCache answerCache, int skip) => {
                answerCache.Clear();
            });
            app.MapGet("/admin", (AnswerCache answers, Other other) => other.Clear());
            "#,
        );
        assert!(params.contains(&("answerCache".into(), "AnswerCache".into())));
        assert!(params.contains(&("answers".into(), "AnswerCache".into())));
        assert!(params.contains(&("other".into(), "Other".into())));
        assert!(!params.iter().any(|(n, _)| n == "skip"));
    }

    fn assert_links(decision: RustCallDecision, file: &str, name: &str) {
        match decision {
            RustCallDecision::Link { node_id, .. } => {
                assert_eq!(node_id, format!("Function:{file}:{name}"), "{node_id}");
            }
            other => panic!("expected link to {file}:{name}, got {other:?}"),
        }
    }

    #[test]
    fn super_inside_inline_module_stays_in_the_file() {
        let src = "pub fn helper() {}\nmod inner {\n    pub fn deep() {}\n}\nmod tests {\n    fn t() { super::helper(); }\n    fn s() { self::only_inside(); }\n    fn only_inside() {}\n}\nfn outer() { inner::deep(); }\n";
        let ws = RustWorkspace::new(
            files(&["src/gate.rs", "src/lib.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "helper");
        add_fn(&mut symbols, "src/lib.rs", "helper");
        add_fn(&mut symbols, "src/gate.rs", "deep");
        add_fn(&mut symbols, "src/gate.rs", "only_inside");
        let super_at = src.find("super::helper").unwrap();
        assert_links(
            ws.decide_at("src/gate.rs", src, "super", "helper", &symbols, Some(super_at)),
            "src/gate.rs",
            "helper",
        );
        let self_at = src.find("self::only_inside").unwrap();
        assert_links(
            ws.decide_at(
                "src/gate.rs",
                src,
                "self",
                "only_inside",
                &symbols,
                Some(self_at),
            ),
            "src/gate.rs",
            "only_inside",
        );
        let inner_at = src.find("inner::deep").unwrap();
        assert_links(
            ws.decide_at("src/gate.rs", src, "inner", "deep", &symbols, Some(inner_at)),
            "src/gate.rs",
            "deep",
        );
    }

    #[test]
    fn imported_type_method_and_path_type_do_not_link_vec() {
        let gate = "pub struct Gate;\nimpl Gate { pub fn open() -> Gate { Gate } }\n";
        let user = "use crate::transforms::gate::Gate;\nfn run_import() { let _ = Gate::open(); }\nfn run_path() { let _ = crate::transforms::gate::Gate::open(); }\nfn run_vec() { let _ = Vec::new(); }\n";
        let ws = ws_lib();
        ws.preload_contents([("src/transforms/gate.rs".to_string(), gate.to_string())]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/transforms/gate.rs", "open");
        add_fn(&mut symbols, "src/other.rs", "new");
        assert_links(
            ws.decide("src/other.rs", user, "Gate", "open", &symbols),
            "src/transforms/gate.rs",
            "open",
        );
        assert_links(
            ws.decide(
                "src/other.rs",
                user,
                "crate::transforms::gate::Gate",
                "open",
                &symbols,
            ),
            "src/transforms/gate.rs",
            "open",
        );
        assert_eq!(
            ws.decide("src/other.rs", user, "Vec", "new", &symbols),
            RustCallDecision::NotAModule
        );
        assert!(ws
            .same_file_target("src/other.rs", user, "Vec", "new", None, &symbols)
            .is_none());
    }

    #[test]
    fn trait_method_ufcs_and_default() {
        let shapes = "pub trait Render { fn render(&self) -> u8; }\npub struct Square;\nimpl Render for Square { fn render(&self) -> u8 { 1 } }\nimpl Default for Square { fn default() -> Self { Square } }\n";
        let user = "use crate::shapes::{Render, Square};\nfn by_trait() { Render::render(); }\nfn by_ufcs() { <Square as Render>::render(); }\nfn by_default() { Square::default(); }\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/shapes.rs", "src/user.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([("src/shapes.rs".to_string(), shapes.to_string())]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/shapes.rs", "render");
        add_fn(&mut symbols, "src/shapes.rs", "default");
        add_fn(&mut symbols, "src/user.rs", "render");
        assert_links(
            ws.decide("src/user.rs", user, "Render", "render", &symbols),
            "src/shapes.rs",
            "render",
        );
        assert_links(
            ws.decide("src/user.rs", user, "<Square as Render>", "render", &symbols),
            "src/shapes.rs",
            "render",
        );
        assert_links(
            ws.decide("src/user.rs", user, "Square", "default", &symbols),
            "src/shapes.rs",
            "default",
        );
    }

    #[test]
    fn same_file_keeps_local_type_and_drops_homonyms() {
        let src = "struct S;\nimpl S { fn build() -> S { S } }\nfn make() { let _ = S::build(); }\nfn write(_: &str) {}\nfn fs_alias() { let _ = fs::write(\"x\", \"y\"); }\nfn from_str(_: &str) {}\nfn ext() { let _ = serde_json::from_str(\"1\"); }\nfn kind_call() { let _ = Kind::from_str(\"k\"); }\nstruct Kind;\nimpl FromStr for Kind { fn from_str(_: &str) -> Result<Kind, ()> { Ok(Kind) } }\n";
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/lib.rs", "build");
        add_fn(&mut symbols, "src/lib.rs", "write");
        add_fn(&mut symbols, "src/lib.rs", "from_str");
        let make_at = src.find("S::build").unwrap();
        assert_links(
            ws.same_file_target("src/lib.rs", src, "S", "build", Some(make_at), &symbols)
                .expect("S::build"),
            "src/lib.rs",
            "build",
        );
        assert!(ws
            .same_file_target("src/lib.rs", src, "fs", "write", None, &symbols)
            .is_none());
        assert_eq!(
            ws.decide("src/lib.rs", src, "serde_json", "from_str", &symbols),
            RustCallDecision::NotAModule
        );
        assert!(ws
            .same_file_target("src/lib.rs", src, "serde_json", "from_str", None, &symbols)
            .is_none());
        let kind_at = src.find("Kind::from_str").unwrap();
        assert!(ws
            .same_file_target(
                "src/lib.rs",
                src,
                "Kind",
                "from_str",
                Some(kind_at),
                &symbols
            )
            .is_some());
    }

    #[test]
    fn pub_use_follows_the_reexport_not_the_homonym() {
        let lib = "pub mod other;\npub mod transforms;\npub use transforms::gate::reinject;\nfn root_call() { crate::reinject(); }\n";
        let transforms = "pub mod gate;\npub use gate::reinject;\n";
        let gate = "pub fn reinject() {}\n";
        let ws = ws_lib();
        ws.preload_contents([
            ("src/lib.rs".to_string(), lib.to_string()),
            ("src/transforms/mod.rs".to_string(), transforms.to_string()),
            ("src/transforms/gate.rs".to_string(), gate.to_string()),
            ("src/other.rs".to_string(), "pub fn reinject() {}\n".to_string()),
        ]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/transforms/gate.rs", "reinject");
        add_fn(&mut symbols, "src/other.rs", "reinject");
        assert_links(
            ws.decide("src/lib.rs", lib, "crate", "reinject", &symbols),
            "src/transforms/gate.rs",
            "reinject",
        );
        assert_links(
            ws.decide(
                "src/main.rs",
                "fn via_mod() { demo::transforms::reinject(); }\n",
                "demo::transforms",
                "reinject",
                &symbols,
            ),
            "src/transforms/gate.rs",
            "reinject",
        );
    }

    #[test]
    fn pub_use_cycle_does_not_loop() {
        let a = "pub use b::reinject;\n";
        let b = "pub use a::reinject;\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/a.rs", "src/b.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([
            ("src/a.rs".to_string(), a.to_string()),
            ("src/b.rs".to_string(), b.to_string()),
        ]);
        let symbols = SymbolTable::new();
        assert_eq!(
            ws.decide("src/lib.rs", "fn t() { crate::a::reinject(); }\n", "crate::a", "reinject", &symbols),
            RustCallDecision::Unresolved
        );
    }

    #[test]
    fn file_facts_are_built_once_per_file() {
        let ws = ws_lib();
        let content = "use crate::transforms::gate as g;\n".repeat(30);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/transforms/gate.rs", "reinject");
        for _ in 0..20 {
            assert!(matches!(
                ws.decide("src/lib.rs", &content, "g", "reinject", &symbols),
                RustCallDecision::Link { .. }
            ));
        }
        assert_eq!(
            ws.facts_builds.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "module aliases rebuilt"
        );
    }

    #[test]
    fn workspace_glob_exclude_and_inherited_dependency() {
        let dir = std::env::temp_dir().join(format!("ce-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for rel in [
            "crates/a/src",
            "crates/b/src",
            "crates/secret/src",
        ] {
            std::fs::create_dir_all(dir.join(rel)).unwrap();
        }
        std::fs::write(
            dir.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/secret\"]\n[workspace.dependencies]\na = { path = \"crates/a\" }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("crates/a/Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("crates/a/src/lib.rs"), "pub mod gate;\n").unwrap();
        std::fs::write(dir.join("crates/a/src/gate.rs"), "pub fn reinject() {}\n").unwrap();
        std::fs::write(
            dir.join("crates/b/Cargo.toml"),
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\n[dependencies]\na.workspace = true\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("crates/b/src/lib.rs"),
            "pub fn in_b() { a::gate::reinject(); }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("crates/secret/Cargo.toml"),
            "[package]\nname = \"secret\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("crates/secret/src/lib.rs"), "pub fn hidden() {}\n").unwrap();
        let files = files(&[
            "crates/a/src/lib.rs",
            "crates/a/src/gate.rs",
            "crates/b/src/lib.rs",
            "crates/secret/src/lib.rs",
        ]);
        let ws = RustWorkspace::load(&dir, &files);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "crates/a/src/gate.rs", "reinject");
        add_fn(&mut symbols, "crates/secret/src/lib.rs", "hidden");
        assert_links(
            ws.decide(
                "crates/a/src/lib.rs",
                "pub fn in_a() { crate::gate::reinject(); }\n",
                "crate::gate",
                "reinject",
                &symbols,
            ),
            "crates/a/src/gate.rs",
            "reinject",
        );
        assert_links(
            ws.decide(
                "crates/b/src/lib.rs",
                "pub fn in_b() { a::gate::reinject(); }\n",
                "a::gate",
                "reinject",
                &symbols,
            ),
            "crates/a/src/gate.rs",
            "reinject",
        );
        assert_eq!(
            ws.decide(
                "crates/secret/src/lib.rs",
                "fn t() { crate::hidden(); }\n",
                "crate",
                "hidden",
                &symbols,
            ),
            RustCallDecision::Unresolved,
            "exclude must drop crates/secret"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn self_new_inside_default_impl_links_the_inherent_new() {
        let src = "struct Store;\nimpl Store { fn new() -> Store { Store } }\nimpl Default for Store {\n    fn default() -> Store { Self::new() }\n}\n";
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/lib.rs", "new");
        let at = src.find("Self::new").unwrap();
        assert_links(
            ws.same_file_target("src/lib.rs", src, "Self", "new", Some(at), &symbols)
                .expect("Self::new"),
            "src/lib.rs",
            "new",
        );
    }

    #[test]
    fn reexported_type_follows_pub_use_not_a_homonym() {
        let lib = "pub mod gate;\npub use gate::Gate;\n";
        let gate = "pub struct Gate;\nimpl Gate { pub fn open() {} }\n";
        let user = "use crate::Gate;\nfn run() { let _ = Gate::open(); }\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/gate.rs", "src/user.rs", "src/other.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([
            ("src/lib.rs".to_string(), lib.to_string()),
            ("src/gate.rs".to_string(), gate.to_string()),
        ]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "open");
        add_fn(&mut symbols, "src/other.rs", "open");
        assert_links(
            ws.decide("src/user.rs", user, "Gate", "open", &symbols),
            "src/gate.rs",
            "open",
        );
    }

    #[test]
    fn parses_workspace_inherited_dependency() {
        let info = parse_cargo_toml(
            "[workspace.dependencies]\ncore = { path = \"crates/core\" }\n[dependencies]\ncore.workspace = true\nserde = \"1\"\n",
        );
        assert!(info.is_workspace);
        assert_eq!(
            info.workspace_deps,
            vec![("core".into(), "crates/core".into())]
        );
        assert_eq!(info.workspace_inherited, vec!["core".to_string()]);
        assert!(info.path_deps.is_empty());
    }

    #[test]
    fn file_level_use_alias_reaches_the_call() {
        let src = "use crate::transforms::gate as g;\nfn via() { g::reinject(); }\n";
        let ws = ws_lib();
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/transforms/gate.rs", "reinject");
        let at = src.find("g::reinject").unwrap();
        assert_links(
            ws.decide_at("src/lib.rs", src, "g", "reinject", &symbols, Some(at)),
            "src/transforms/gate.rs",
            "reinject",
        );
    }

    /// Deux `use … as g` dans deux fonctions. La portée de chacun s'arrête
    /// à sa fonction : `via_other` ne doit jamais aller vers `gate.rs`.
    #[test]
    fn scoped_use_alias_stays_inside_its_function() {
        let src = "pub fn via_gate() -> u8 {\n    use crate::gate as g;\n    g::reinject()\n}\npub fn via_other() -> u8 {\n    use crate::other as g;\n    g::reinject()\n}\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/gate.rs", "src/other.rs", "src/user.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "reinject");
        add_fn(&mut symbols, "src/other.rs", "reinject");
        let gate_at = src.find("g::reinject").unwrap();
        let other_at = src.rfind("g::reinject").unwrap();
        assert_links(
            ws.decide_at("src/user.rs", src, "g", "reinject", &symbols, Some(gate_at)),
            "src/gate.rs",
            "reinject",
        );
        assert_links(
            ws.decide_at("src/user.rs", src, "g", "reinject", &symbols, Some(other_at)),
            "src/other.rs",
            "reinject",
        );
    }

    /// `#[path = "gate_v2.rs"] mod gate` ne doit jamais retomber sur `gate.rs`.
    #[test]
    fn scoped_path_attribute_does_not_link_the_default_file() {
        let lib = "#[path = \"gate_v2.rs\"]\npub mod gate;\npub mod user;\n";
        let user = "pub fn run() -> u8 { crate::gate::reinject() }\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/gate.rs", "src/gate_v2.rs", "src/user.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([
            ("src/lib.rs".to_string(), lib.to_string()),
            ("src/gate.rs".to_string(), "pub fn reinject() -> u8 { 1 }\n".to_string()),
            (
                "src/gate_v2.rs".to_string(),
                "pub fn reinject() -> u8 { 2 }\n".to_string(),
            ),
        ]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "reinject");
        add_fn(&mut symbols, "src/gate_v2.rs", "reinject");
        assert_links(
            ws.decide("src/user.rs", user, "crate::gate", "reinject", &symbols),
            "src/gate_v2.rs",
            "reinject",
        );
    }

    /// `#[cfg(test)] #[path] mod clock` puis `#[cfg(not(test))] mod clock`.
    /// Un appel de production vise `clock.rs`, un appel dans `cfg(test)` le mock.
    #[test]
    fn ambiguity_production_cfg_not_test() {
        let lib = "#[cfg(test)]\n#[path = \"clock_mock.rs\"]\npub mod clock;\n#[cfg(not(test))]\npub mod clock;\n#[cfg(test)]\nmod tests {\n    fn t() { crate::clock::now(); }\n}\nfn prod() { crate::clock::now(); }\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/clock.rs", "src/clock_mock.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([
            ("src/lib.rs".to_string(), lib.to_string()),
            ("src/clock.rs".to_string(), "pub fn now() -> u64 { 1 }\n".to_string()),
            (
                "src/clock_mock.rs".to_string(),
                "pub fn now() -> u64 { 2 }\n".to_string(),
            ),
        ]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/clock.rs", "now");
        add_fn(&mut symbols, "src/clock_mock.rs", "now");
        let prod_at = lib.find("fn prod").unwrap();
        assert_links(
            ws.decide_at("src/lib.rs", lib, "crate::clock", "now", &symbols, Some(prod_at)),
            "src/clock.rs",
            "now",
        );
        let test_at = lib.find("fn t()").unwrap();
        assert_links(
            ws.decide_at("src/lib.rs", lib, "crate::clock", "now", &symbols, Some(test_at)),
            "src/clock_mock.rs",
            "now",
        );
    }

    /// Le `#[path]` d'un module inline ne s'applique pas au `mod` homonyme du fichier.
    #[test]
    fn ambiguity_path_nested_does_not_steal() {
        let lib = "pub mod gate;\npub mod legacy {\n    #[path = \"gate_v1.rs\"]\n    pub mod gate;\n}\n";
        let user = "pub fn current() -> u8 { crate::gate::reinject() }\n";
        let ws = RustWorkspace::new(
            files(&[
                "src/lib.rs",
                "src/gate.rs",
                "src/legacy/gate_v1.rs",
                "src/user.rs",
            ]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([
            ("src/lib.rs".to_string(), lib.to_string()),
            ("src/gate.rs".to_string(), "pub fn reinject() -> u8 { 1 }\n".to_string()),
            (
                "src/legacy/gate_v1.rs".to_string(),
                "pub fn reinject() -> u8 { 0 }\n".to_string(),
            ),
        ]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "reinject");
        add_fn(&mut symbols, "src/legacy/gate_v1.rs", "reinject");
        assert_links(
            ws.decide("src/user.rs", user, "crate::gate", "reinject", &symbols),
            "src/gate.rs",
            "reinject",
        );
    }

    /// Sans `use super::*`, l'enfant ne voit pas les `use` du parent.
    /// L'item local (`mod g`, `struct Gate`) est la cible.
    #[test]
    fn ambiguity_parent_use_hidden_without_super_glob() {
        let user = "use crate::gate as g;\nuse crate::gate::Gate;\npub mod inner {\n    pub mod g { pub fn reinject() -> u8 { 2 } }\n    pub struct Gate;\n    impl Gate { pub fn open() -> u8 { 2 } }\n    pub fn call_mod() -> u8 { g::reinject() }\n    pub fn call_type() -> u8 { Gate::open() }\n}\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/gate.rs", "src/user.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([
            ("src/lib.rs".to_string(), "pub mod gate;\npub mod user;\n".to_string()),
            (
                "src/gate.rs".to_string(),
                "pub struct Gate;\nimpl Gate { pub fn open() -> u8 { 1 } }\npub fn reinject() -> u8 { 1 }\n".to_string(),
            ),
        ]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "reinject");
        add_fn(&mut symbols, "src/gate.rs", "open");
        add_fn(&mut symbols, "src/user.rs", "reinject");
        add_fn(&mut symbols, "src/user.rs", "open");
        let mod_at = user.find("g::reinject").unwrap();
        let type_at = user.find("Gate::open").unwrap();
        assert_links(
            ws.decide_at("src/user.rs", user, "g", "reinject", &symbols, Some(mod_at)),
            "src/user.rs",
            "reinject",
        );
        // `Gate` n'est pas un module. Le pipeline retombe alors sur le palier
        // même fichier, qui voit l'`impl Gate` local — pas l'import du parent.
        let type_decision = ws.decide_at("src/user.rs", user, "Gate", "open", &symbols, Some(type_at));
        assert!(
            !matches!(type_decision, RustCallDecision::Link { ref node_id, .. } if node_id.contains("gate.rs")),
            "parent import must not win: {type_decision:?}"
        );
        let type_decision = match type_decision {
            RustCallDecision::NotAModule => ws
                .same_file_target("src/user.rs", user, "Gate", "open", Some(type_at), &symbols)
                .expect("local Gate::open"),
            other => other,
        };
        assert_links(type_decision, "src/user.rs", "open");
    }

    /// Sans item local et sans `use super::*`, le `use` du parent n'est pas une cible.
    #[test]
    fn ambiguity_child_without_super_glob_does_not_inherit() {
        let user = "use crate::gate as g;\nmod inner {\n    fn call() { g::reinject() }\n}\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/gate.rs", "src/user.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([(
            "src/gate.rs".to_string(),
            "pub fn reinject() -> u8 { 1 }\n".to_string(),
        )]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "reinject");
        let at = user.find("g::reinject").unwrap();
        let decision = ws.decide_at("src/user.rs", user, "g", "reinject", &symbols, Some(at));
        assert!(
            !matches!(decision, RustCallDecision::Link { ref node_id, .. } if node_id.contains("gate.rs")),
            "inherited use must stay unresolved, got {decision:?}"
        );
    }

    /// `use super::*` et un `mod g` local : l'item local gagne, pas le `use` du parent.
    #[test]
    fn ambiguity_local_item_hides_super_glob() {
        let user = "use crate::gate as g;\nmod inner {\n    use super::*;\n    mod g { pub fn reinject() -> u8 { 2 } }\n    fn call() { g::reinject() }\n}\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/gate.rs", "src/user.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([(
            "src/gate.rs".to_string(),
            "pub fn reinject() -> u8 { 1 }\n".to_string(),
        )]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "reinject");
        add_fn(&mut symbols, "src/user.rs", "reinject");
        let at = user.rfind("g::reinject").unwrap();
        assert_links(
            ws.decide_at("src/user.rs", user, "g", "reinject", &symbols, Some(at)),
            "src/user.rs",
            "reinject",
        );
    }

    /// `use crate_externe::…` dans un module enfant est absolu (Rust 2018).
    #[test]
    fn ambiguity_extern_use_inside_child_module_resolves() {
        let lib = "pub mod gate;\n";
        let gate = "pub struct Gate;\nimpl Gate { pub fn open() -> u8 { 1 } }\n";
        let user = "mod tests {\n    use demo::gate::Gate;\n    fn t() { let _ = Gate::open(); }\n}\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/gate.rs", "src/user.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([
            ("src/lib.rs".to_string(), lib.to_string()),
            ("src/gate.rs".to_string(), gate.to_string()),
        ]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "open");
        let at = user.find("Gate::open").unwrap();
        assert_links(
            ws.decide_at("src/user.rs", user, "Gate", "open", &symbols, Some(at)),
            "src/gate.rs",
            "open",
        );
    }

    /// `use super::*` dans l'enfant rend les `use` du parent visibles. Exact.
    #[test]
    fn ambiguity_super_glob_keeps_parent_use() {
        let user = "use crate::gate::Gate;\nuse crate::gate as g2;\npub fn run() -> u8 { Gate::open() }\nmod tests {\n    use super::*;\n    fn t_type() { let _ = Gate::open(); }\n    fn t_alias() { let _ = g2::Gate::open(); }\n}\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/gate.rs", "src/user.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([(
            "src/gate.rs".to_string(),
            "pub struct Gate;\nimpl Gate { pub fn open() -> u8 { 1 } }\n".to_string(),
        )]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/gate.rs", "open");
        let first = user.find("Gate::open").unwrap();
        let type_at = user[first + 1..].find("Gate::open").unwrap() + first + 1;
        let alias_at = user.find("g2::Gate").unwrap();
        assert_links(
            ws.decide_at("src/user.rs", user, "Gate", "open", &symbols, Some(type_at)),
            "src/gate.rs",
            "open",
        );
        assert_links(
            ws.decide_at("src/user.rs", user, "g2::Gate", "open", &symbols, Some(alias_at)),
            "src/gate.rs",
            "open",
        );
    }

    /// Deux `#[cfg]` que l'on ne sait pas départager : aucun lien, même si une seule
    /// des cibles possède la fonction.
    #[test]
    fn ambiguity_two_cfg_families_not_confident() {
        let lib = "#[cfg(unix)]\n#[path = \"clock_unix.rs\"]\npub mod clock;\n#[cfg(windows)]\n#[path = \"clock_win.rs\"]\npub mod clock;\nfn prod() { crate::clock::now(); }\n";
        let ws = RustWorkspace::new(
            files(&["src/lib.rs", "src/clock_unix.rs", "src/clock_win.rs"]),
            vec![pkg("demo", "", "src", &[])],
        );
        ws.preload_contents([
            ("src/lib.rs".to_string(), lib.to_string()),
            ("src/clock_unix.rs".to_string(), "pub fn now() -> u64 { 1 }\n".to_string()),
            ("src/clock_win.rs".to_string(), "pub fn tick() -> u64 { 2 }\n".to_string()),
        ]);
        let mut symbols = SymbolTable::new();
        add_fn(&mut symbols, "src/clock_unix.rs", "now");
        let at = lib.find("crate::clock").unwrap();
        match ws.decide_at("src/lib.rs", lib, "crate::clock", "now", &symbols, Some(at)) {
            RustCallDecision::Ambiguous { note } => {
                assert!(note.contains("plusieurs"), "{note}");
            }
            other => panic!("expected ambiguous, got {other:?}"),
        }
    }
}
