//! Scan léger du source Rust : modules inline, blocs `impl`/`trait`,
//! et correspondance de jokers de membres Cargo. Pas un analyseur complet :
//! les chaînes, les caractères et les commentaires sont sautés pour ne pas
//! prendre un mot dans un littéral pour un item.

#[derive(Debug, Clone)]
pub(super) struct InlineMod {
    pub name: String,
    pub body_start: usize,
    pub body_end: usize,
    pub parent: Option<usize>,
    pub functions: Vec<String>,
    /// Noms de l'espace des types déclarés dans ce module (`mod`, `struct`,
    /// `enum`, `trait`, `type`, `union`). Pas les fonctions : `fn git` ne
    /// masque pas le module `git`.
    pub items: Vec<String>,
    /// `use super::*` au niveau du module (pas dans une fonction).
    pub imports_super_glob: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ImplBlock {
    pub start: usize,
    pub end: usize,
    pub owner_type: String,
    pub trait_name: Option<String>,
    pub methods: Vec<String>,
}

#[derive(Debug)]
struct Tok {
    start: usize,
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    Ident(String),
    LBrace,
    RBrace,
    Semi,
    Lt,
    Gt,
}

enum Frame {
    Mod { index: Option<usize> },
    Impl { index: usize },
    Fn { span: usize },
    Other { span: usize },
}

/// Accolade d'une fonction ou d'un bloc (pas d'un `mod` ni d'un `impl`).
/// Un `use` situé dedans ne vaut que pour `[start, end)`.
#[derive(Debug, Clone)]
pub(super) struct CodeSpan {
    pub start: usize,
    pub end: usize,
}

pub(super) fn scan_items(content: &str) -> (Vec<InlineMod>, Vec<ImplBlock>, Vec<CodeSpan>) {
    let tokens = tokenize(content);
    let mut mods: Vec<InlineMod> = Vec::new();
    let mut impls: Vec<ImplBlock> = Vec::new();
    let mut blocks: Vec<CodeSpan> = Vec::new();
    let mut stack = vec![Frame::Mod { index: None }];
    let mut i = 0usize;
    while i < tokens.len() {
        if matches!(tokens[i].kind, Kind::RBrace) {
            if let Some(frame) = stack.pop() {
                match frame {
                    Frame::Mod { index: Some(idx) } => {
                        if let Some(m) = mods.get_mut(idx) {
                            m.body_end = tokens[i].start;
                        }
                    }
                    Frame::Impl { index } => {
                        if let Some(b) = impls.get_mut(index) {
                            b.end = tokens[i].start.saturating_add(1);
                        }
                    }
                    Frame::Fn { span } | Frame::Other { span } => {
                        if let Some(s) = blocks.get_mut(span) {
                            s.end = tokens[i].start;
                        }
                    }
                    Frame::Mod { index: None } => {}
                }
            }
            if stack.is_empty() {
                stack.push(Frame::Mod { index: None });
            }
            i += 1;
            continue;
        }
        if matches!(tokens[i].kind, Kind::LBrace) {
            let span = blocks.len();
            blocks.push(CodeSpan {
                start: tokens[i].start,
                end: content.len(),
            });
            stack.push(Frame::Other { span });
            i += 1;
            continue;
        }
        let Some(name) = ident_at(&tokens, i) else {
            i += 1;
            continue;
        };
        let in_mod = matches!(stack.last(), Some(Frame::Mod { .. }));
        let in_impl = matches!(stack.last(), Some(Frame::Impl { .. }));
        if name == "mod" && in_mod {
            if let Some(mod_name) = next_ident(&tokens, i + 1) {
                if let Some(Frame::Mod { index: Some(pidx) }) = stack.last() {
                    if let Some(m) = mods.get_mut(*pidx) {
                        m.items.push(mod_name.1.clone());
                    }
                }
                if let Some(brace_at) = sig_brace(&tokens, mod_name.0 + 1) {
                    let parent = match stack.last() {
                        Some(Frame::Mod { index }) => *index,
                        _ => None,
                    };
                    let idx = mods.len();
                    mods.push(InlineMod {
                        name: mod_name.1,
                        body_start: tokens[brace_at].start.saturating_add(1),
                        body_end: content.len(),
                        parent,
                        functions: Vec::new(),
                        items: Vec::new(),
                        imports_super_glob: false,
                    });
                    stack.push(Frame::Mod { index: Some(idx) });
                    i = brace_at + 1;
                    continue;
                }
            }
        } else if (name == "impl" || name == "trait") && in_mod {
            if let Some(brace_at) = sig_brace(&tokens, i + 1) {
                let (owner, trait_name) = if name == "trait" {
                    (next_ident(&tokens, i + 1).map(|(_, n)| n).unwrap_or_default(), None)
                } else {
                    parse_impl_head(&tokens[i + 1..brace_at])
                };
                let idx = impls.len();
                impls.push(ImplBlock {
                    start: tokens[i].start,
                    end: content.len(),
                    owner_type: owner,
                    trait_name,
                    methods: Vec::new(),
                });
                stack.push(Frame::Impl { index: idx });
                i = brace_at + 1;
                continue;
            }
        } else if name == "fn" && (in_mod || in_impl) {
            if let Some((name_at, fname)) = next_ident(&tokens, i + 1) {
                match stack.last_mut() {
                    Some(Frame::Mod { index: Some(idx) }) => {
                        if let Some(m) = mods.get_mut(*idx) {
                            m.functions.push(fname);
                        }
                    }
                    Some(Frame::Impl { index }) => {
                        if let Some(b) = impls.get_mut(*index) {
                            b.methods.push(fname);
                        }
                    }
                    _ => {}
                }
                if let Some(brace_at) = sig_brace(&tokens, name_at + 1) {
                    let span = blocks.len();
                    blocks.push(CodeSpan {
                        start: tokens[brace_at].start,
                        end: content.len(),
                    });
                    stack.push(Frame::Fn { span });
                    i = brace_at + 1;
                    continue;
                }
            }
        } else if matches!(name, "struct" | "enum" | "trait" | "type" | "union") && in_mod {
            if let Some((_, item)) = next_ident(&tokens, i + 1) {
                if let Some(Frame::Mod { index: Some(idx) }) = stack.last() {
                    if let Some(m) = mods.get_mut(*idx) {
                        m.items.push(item);
                    }
                }
            }
        }
        i += 1;
    }
    (mods, impls, blocks)
}

/// Plus petit bloc qui contient `byte`, s'il y en a un.
pub(super) fn innermost_block(spans: &[CodeSpan], byte: usize) -> Option<(usize, usize)> {
    let mut best: Option<&CodeSpan> = None;
    for span in spans {
        if span.start <= byte && byte < span.end {
            match best {
                None => best = Some(span),
                Some(prev) if span.end - span.start < prev.end - prev.start => best = Some(span),
                _ => {}
            }
        }
    }
    best.map(|span| (span.start, span.end))
}

/// Configuration que l'on sait départager. Tout le reste est `Other` :
/// plusieurs déclarations dont une est `Other` ne donnent pas un lien à 0,95.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CfgKind {
    Always,
    Test,
    NotTest,
    Other,
}

/// Appel hors de tout `#[cfg(test)]`, ou appel qui s'y trouve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CallMode {
    Production,
    Test,
}

/// `mod name;` (point-virgule, pas un corps inline), avec son `#[path]` et son `#[cfg]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModDecl {
    pub name: String,
    /// Vrai si la déclaration est dans un `mod parent { … }`.
    pub nested: bool,
    pub path: Option<String>,
    pub cfg: CfgKind,
}

pub(super) fn merge_cfg(a: CfgKind, b: CfgKind) -> CfgKind {
    match (a, b) {
        (CfgKind::Always, other) | (other, CfgKind::Always) => other,
        (CfgKind::Test, CfgKind::Test) => CfgKind::Test,
        (CfgKind::NotTest, CfgKind::NotTest) => CfgKind::NotTest,
        _ => CfgKind::Other,
    }
}

/// Déclarations `mod name;` du fichier. Un `#[path]` ne vaut que pour la
/// déclaration qu'il précède, pas pour un homonyme plus loin ou dans un autre module.
pub(super) fn mod_decls(content: &str) -> Vec<ModDecl> {
    let b = content.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    let mut pending_path: Option<String> = None;
    let mut pending_cfg = CfgKind::Always;
    let mut brace = 0i32;
    // Profondeur d'accolade de chaque `mod inline {` encore ouvert.
    let mut frames: Vec<i32> = Vec::new();
    while i < b.len() {
        if let Some(n) = skip_trivia_or_literal(content, i) {
            i = n;
            continue;
        }
        if b[i] == b'#' && i + 1 < b.len() && b[i + 1] == b'[' {
            if let Some(path) = attr_path_value(content, i + 2) {
                pending_path = Some(path);
            }
            if let Some(cfg) = classify_cfg_attr(content, i) {
                pending_cfg = merge_cfg(pending_cfg, cfg);
            }
            i = skip_group(content, i + 1, b'[', b']');
            continue;
        }
        if b[i] == b'#' && i + 1 < b.len() && b[i + 1] == b'!' {
            i = skip_group(content, i + 2, b'[', b']');
            continue;
        }
        if b[i] == b'{' {
            brace += 1;
            pending_path = None;
            pending_cfg = CfgKind::Always;
            i += 1;
            continue;
        }
        if b[i] == b'}' {
            brace -= 1;
            while frames.last().copied() == Some(brace + 1) {
                frames.pop();
            }
            i += 1;
            continue;
        }
        if b[i] == b';' {
            pending_path = None;
            pending_cfg = CfgKind::Always;
            i += 1;
            continue;
        }
        if !is_ident_start(b[i]) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < b.len() && is_ident_cont(b[i]) {
            i += 1;
        }
        let word = &content[start..i];
        if word == "pub" {
            i = skip_trivia_span(content, i);
            if i < b.len() && b[i] == b'(' {
                i = skip_group(content, i, b'(', b')');
            }
            continue;
        }
        if word == "mod" {
            i = skip_trivia_span(content, i);
            if i < b.len() && is_ident_start(b[i]) {
                let ns = i;
                i += 1;
                while i < b.len() && is_ident_cont(b[i]) {
                    i += 1;
                }
                let name = content[ns..i].to_string();
                i = skip_trivia_span(content, i);
                if i < b.len() && b[i] == b';' {
                    out.push(ModDecl {
                        name,
                        nested: !frames.is_empty(),
                        path: pending_path.take(),
                        cfg: pending_cfg,
                    });
                    pending_cfg = CfgKind::Always;
                    i += 1;
                    continue;
                }
                if i < b.len() && b[i] == b'{' {
                    brace += 1;
                    frames.push(brace);
                    pending_path = None;
                    pending_cfg = CfgKind::Always;
                    i += 1;
                    continue;
                }
            }
            pending_path = None;
            pending_cfg = CfgKind::Always;
            continue;
        }
        if matches!(
            word,
            "fn" | "struct"
                | "enum"
                | "impl"
                | "trait"
                | "use"
                | "type"
                | "const"
                | "static"
                | "union"
                | "extern"
                | "macro_rules"
        ) {
            pending_path = None;
            pending_cfg = CfgKind::Always;
        }
    }
    out
}

/// Intervalles `[start, end)` des accolades compilées seulement en `cfg(test)`.
pub(super) fn cfg_test_ranges(content: &str) -> Vec<(usize, usize)> {
    let b = content.as_bytes();
    let mut i = 0usize;
    let mut depth = 0i32;
    let mut stack: Vec<bool> = Vec::new();
    let mut open: Vec<(i32, usize)> = Vec::new();
    let mut ranges = Vec::new();
    let mut pending = CfgKind::Always;
    let mut have_cfg = false;
    while i < b.len() {
        if let Some(n) = skip_trivia_or_literal(content, i) {
            i = n;
            continue;
        }
        if b[i] == b'#' && i + 1 < b.len() && b[i + 1] == b'[' {
            if let Some(cfg) = classify_cfg_attr(content, i) {
                pending = merge_cfg(pending, cfg);
                have_cfg = true;
            }
            i = skip_group(content, i + 1, b'[', b']');
            continue;
        }
        if b[i] == b'#' && i + 1 < b.len() && b[i + 1] == b'!' {
            i = skip_group(content, i + 2, b'[', b']');
            continue;
        }
        if b[i] == b'{' {
            depth += 1;
            let parent = stack.last().copied().unwrap_or(false);
            let is_test = if have_cfg {
                match pending {
                    CfgKind::Test => true,
                    CfgKind::NotTest => false,
                    _ => parent,
                }
            } else {
                parent
            };
            stack.push(is_test);
            if is_test {
                open.push((depth, i));
            }
            pending = CfgKind::Always;
            have_cfg = false;
            i += 1;
            continue;
        }
        if b[i] == b'}' {
            let was_test = stack.pop().unwrap_or(false);
            if was_test {
                if let Some((d, start)) = open.last().copied() {
                    if d == depth {
                        open.pop();
                        ranges.push((start, i));
                    }
                }
            }
            if depth > 0 {
                depth -= 1;
            }
            pending = CfgKind::Always;
            have_cfg = false;
            i += 1;
            continue;
        }
        if b[i] == b';' {
            pending = CfgKind::Always;
            have_cfg = false;
        }
        i += 1;
    }
    ranges
}

fn classify_cfg_attr(content: &str, hash_at: usize) -> Option<CfgKind> {
    let b = content.as_bytes();
    let mut j = hash_at + 2;
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if j >= b.len() || !content[j..].starts_with("cfg") {
        return None;
    }
    j += 3;
    if j < b.len() && is_ident_cont(b[j]) {
        return None;
    }
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if j >= b.len() || b[j] != b'(' {
        return None;
    }
    let end = skip_group(content, j, b'(', b')');
    if end <= j + 1 {
        return None;
    }
    Some(classify_pred(content[j + 1..end - 1].trim()))
}

fn classify_pred(raw: &str) -> CfgKind {
    let s = raw.trim();
    if s == "test" {
        return CfgKind::Test;
    }
    if let Some(inner) = stripped_call(s, "not") {
        return match classify_pred(inner) {
            CfgKind::Test => CfgKind::NotTest,
            CfgKind::NotTest => CfgKind::Test,
            _ => CfgKind::Other,
        };
    }
    if let Some(inner) = stripped_call(s, "all") {
        let mut acc = CfgKind::Always;
        for part in split_cfg_args(inner) {
            acc = merge_cfg(acc, classify_pred(part));
        }
        return acc;
    }
    if let Some(inner) = stripped_call(s, "any") {
        let parts: Vec<CfgKind> = split_cfg_args(inner).into_iter().map(classify_pred).collect();
        if !parts.is_empty() && parts.iter().all(|p| *p == CfgKind::Test) {
            return CfgKind::Test;
        }
        if !parts.is_empty() && parts.iter().all(|p| *p == CfgKind::NotTest) {
            return CfgKind::NotTest;
        }
        return CfgKind::Other;
    }
    CfgKind::Other
}

fn stripped_call<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(name)?.trim_start();
    if !rest.starts_with('(') || !rest.ends_with(')') {
        return None;
    }
    let mut depth = 0i32;
    let bytes = rest.as_bytes();
    for (idx, ch) in bytes.iter().enumerate() {
        match ch {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    if idx + 1 != bytes.len() {
                        return None;
                    }
                    return Some(rest[1..idx].trim());
                }
            }
            _ => {}
        }
    }
    None
}

fn split_cfg_args(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (idx, ch) in input.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                let part = input[start..idx].trim();
                if !part.is_empty() {
                    parts.push(part);
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts
}

fn attr_path_value(content: &str, mut j: usize) -> Option<String> {
    let b = content.as_bytes();
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if !content[j..].starts_with("path") {
        return None;
    }
    j += 4;
    if j < b.len() && is_ident_cont(b[j]) {
        return None;
    }
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    if j >= b.len() || b[j] != b'=' {
        return None;
    }
    j += 1;
    while j < b.len() && b[j].is_ascii_whitespace() {
        j += 1;
    }
    rust_string_at(content, j)
}

fn rust_string_at(content: &str, j: usize) -> Option<String> {
    let b = content.as_bytes();
    if j >= b.len() || b[j] != b'"' {
        return None;
    }
    let end = cooked_string_end(content, j)?;
    let inner = &content[j + 1..end - 1];
    if inner.contains('\\') {
        return None;
    }
    Some(inner.to_string())
}

fn skip_trivia_span(content: &str, mut i: usize) -> usize {
    while let Some(n) = skip_trivia_or_literal(content, i) {
        if n == i {
            break;
        }
        i = n;
    }
    i
}

fn skip_group(content: &str, open: usize, left: u8, right: u8) -> usize {
    let b = content.as_bytes();
    if open >= b.len() || b[open] != left {
        return open;
    }
    let mut i = open + 1;
    let mut depth = 1i32;
    while i < b.len() && depth > 0 {
        if let Some(n) = skip_trivia_or_literal(content, i) {
            i = n;
            continue;
        }
        if b[i] == left {
            depth += 1;
        } else if b[i] == right {
            depth -= 1;
        }
        i += 1;
    }
    i
}

fn ident_at(tokens: &[Tok], i: usize) -> Option<&str> {
    match tokens.get(i).map(|t| &t.kind) {
        Some(Kind::Ident(name)) => Some(name.as_str()),
        _ => None,
    }
}

fn next_ident(tokens: &[Tok], from: usize) -> Option<(usize, String)> {
    let mut angle = 0i32;
    for (idx, tok) in tokens.iter().enumerate().skip(from) {
        match &tok.kind {
            Kind::Lt => angle += 1,
            Kind::Gt => angle -= 1,
            Kind::Ident(name) if angle == 0 => return Some((idx, name.clone())),
            Kind::LBrace | Kind::Semi if angle == 0 => return None,
            _ => {}
        }
    }
    None
}

/// Index of the `{` that opens the body, skipping generics and parentheses
/// that the tokenizer does not emit. `;` means there is no body.
fn sig_brace(tokens: &[Tok], from: usize) -> Option<usize> {
    let mut angle = 0i32;
    for (idx, tok) in tokens.iter().enumerate().skip(from) {
        match tok.kind {
            Kind::Lt => angle += 1,
            Kind::Gt => angle -= 1,
            Kind::LBrace if angle <= 0 => return Some(idx),
            Kind::Semi if angle <= 0 => return None,
            _ => {}
        }
    }
    None
}

fn parse_impl_head(tokens: &[Tok]) -> (String, Option<String>) {
    let mut parts: Vec<String> = Vec::new();
    let mut angle = 0i32;
    for tok in tokens {
        match &tok.kind {
            Kind::Lt => angle += 1,
            Kind::Gt => angle -= 1,
            Kind::Ident(name) if angle == 0 => {
                if name == "where" {
                    break;
                }
                if name == "unsafe" || name == "const" {
                    continue;
                }
                parts.push(name.clone());
            }
            _ => {}
        }
    }
    if let Some(idx) = parts.iter().position(|p| p == "for") {
        let trait_name = parts[..idx].last().cloned();
        let owner = parts[idx + 1..].last().cloned().unwrap_or_default();
        (owner, trait_name)
    } else {
        (parts.last().cloned().unwrap_or_default(), None)
    }
}

pub(super) fn type_base(name: &str) -> &str {
    let name = name.trim();
    let name = name.rsplit("::").next().unwrap_or(name).trim();
    match name.find('<') {
        Some(idx) => name[..idx].trim(),
        None => name,
    }
}

pub(super) fn is_type_like(name: &str) -> bool {
    let name = type_base(name);
    name == "Self" || name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

pub(super) fn parse_ufcs(qualifier: &str) -> Option<(&str, &str)> {
    let q = qualifier.trim();
    let inner = q.strip_prefix('<')?.strip_suffix('>')?.trim();
    let mut angle = 0i32;
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        match bytes[i] {
            b'<' => angle += 1,
            b'>' => angle -= 1,
            b' ' if angle == 0 && inner[i..].starts_with(" as ") => {
                let ty = inner[..i].trim();
                let tr = inner[i + 4..].trim();
                if !ty.is_empty() && !tr.is_empty() {
                    return Some((ty, tr));
                }
                return None;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `(module path, type name)` when the qualifier ends with a type segment.
/// `crate::gate::Gate` → `("crate::gate", "Gate")`. A trailing turbofish
/// (`Vec::<u8>`) is dropped. A single identifier is not a path.
pub(super) fn split_type_tail(qualifier: &str) -> Option<(String, String)> {
    let mut segs: Vec<&str> = qualifier.split("::").filter(|s| !s.is_empty()).collect();
    while segs.last().is_some_and(|s| s.trim_start().starts_with('<')) {
        segs.pop();
    }
    if segs.len() < 2 {
        return None;
    }
    let last = type_base(segs.pop()?);
    if !is_type_like(last) || last == "Self" {
        return None;
    }
    Some((segs.join("::"), last.to_string()))
}

pub(super) fn is_glob(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

pub(super) fn glob_match(pattern: &str, path: &str) -> bool {
    fn rec(p: &[&str], t: &[&str]) -> bool {
        if p.is_empty() {
            return t.is_empty();
        }
        if p[0] == "**" {
            if rec(&p[1..], t) {
                return true;
            }
            return !t.is_empty() && rec(p, &t[1..]);
        }
        if t.is_empty() {
            return false;
        }
        seg_match(p[0], t[0]) && rec(&p[1..], &t[1..])
    }
    let p: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let t: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    rec(&p, &t)
}

fn seg_match(pat: &str, seg: &str) -> bool {
    fn rec(p: &[u8], s: &[u8]) -> bool {
        if p.is_empty() {
            return s.is_empty();
        }
        match p[0] {
            b'*' => rec(&p[1..], s) || (!s.is_empty() && rec(p, &s[1..])),
            b'?' => !s.is_empty() && rec(&p[1..], &s[1..]),
            c => !s.is_empty() && s[0] == c && rec(&p[1..], &s[1..]),
        }
    }
    rec(pat.as_bytes(), seg.as_bytes())
}

pub(super) fn glob_may_descend(pattern: &str, rel: &str) -> bool {
    if pattern.split('/').any(|s| s == "**") {
        return true;
    }
    let p = pattern.split('/').filter(|s| !s.is_empty()).count();
    let r = rel.split('/').filter(|s| !s.is_empty()).count();
    r < p
}

fn tokenize(content: &str) -> Vec<Tok> {
    let b = content.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < b.len() {
        if let Some(n) = skip_trivia_or_literal(content, i) {
            i = n;
            continue;
        }
        if is_ident_start(b[i]) {
            let start = i;
            i += 1;
            while i < b.len() && is_ident_cont(b[i]) {
                i += 1;
            }
            out.push(Tok {
                start,
                kind: Kind::Ident(content[start..i].to_string()),
            });
            continue;
        }
        let kind = match b[i] {
            b'{' => Some(Kind::LBrace),
            b'}' => Some(Kind::RBrace),
            b';' => Some(Kind::Semi),
            b'<' => Some(Kind::Lt),
            b'>' => Some(Kind::Gt),
            _ => None,
        };
        if let Some(kind) = kind {
            out.push(Tok { start: i, kind });
        }
        i += 1;
    }
    out
}

fn skip_trivia_or_literal(content: &str, i: usize) -> Option<usize> {
    let b = content.as_bytes();
    if i >= b.len() {
        return None;
    }
    if b[i].is_ascii_whitespace() {
        let mut j = i + 1;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        return Some(j);
    }
    if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
        return Some(content[i..].find('\n').map(|n| i + n + 1).unwrap_or(content.len()));
    }
    if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
        let mut j = i + 2;
        let mut depth = 1i32;
        while j + 1 < b.len() {
            if b[j] == b'/' && b[j + 1] == b'*' {
                depth += 1;
                j += 2;
                continue;
            }
            if b[j] == b'*' && b[j + 1] == b'/' {
                depth -= 1;
                j += 2;
                if depth == 0 {
                    return Some(j);
                }
                continue;
            }
            j += 1;
        }
        return Some(content.len());
    }
    if b[i] == b'\'' {
        return Some(skip_char_or_lifetime(content, i));
    }
    if let Some(n) = skip_stringish(content, i) {
        return Some(n);
    }
    None
}

fn skip_char_or_lifetime(content: &str, i: usize) -> usize {
    let b = content.as_bytes();
    if i + 2 < b.len() && b[i + 1] != b'\\' && b[i + 2] == b'\'' {
        return i + 3;
    }
    if i + 1 < b.len() && b[i + 1] == b'\\' {
        let mut j = i + 2;
        if j < b.len() && b[j] == b'u' {
            j += 1;
            if j < b.len() && b[j] == b'{' {
                j += 1;
                while j < b.len() && b[j] != b'}' {
                    j += 1;
                }
                if j < b.len() {
                    j += 1;
                }
            }
        } else if j < b.len() {
            j += 1;
        }
        if j < b.len() && b[j] == b'\'' {
            return j + 1;
        }
        return j;
    }
    let mut j = i + 1;
    if j < b.len() && is_ident_start(b[j]) {
        j += 1;
        while j < b.len() && is_ident_cont(b[j]) {
            j += 1;
        }
    } else {
        j = (i + 1).min(b.len());
    }
    j
}

fn skip_stringish(content: &str, i: usize) -> Option<usize> {
    let b = content.as_bytes();
    let mut j = i;
    let mut prefixed = false;
    if matches!(b.get(j), Some(b'b' | b'c')) {
        let n = b.get(j + 1).copied();
        if matches!(n, Some(b'"' | b'\'' | b'r' | b'#')) {
            prefixed = true;
            j += 1;
        }
    }
    let raw = b.get(j) == Some(&b'r') && matches!(b.get(j + 1), Some(b'"' | b'#'));
    if raw {
        j += 1;
        return raw_string_end(content, j);
    }
    if b.get(j) == Some(&b'\'') && prefixed {
        return Some(skip_char_or_lifetime(content, j));
    }
    if b.get(j) == Some(&b'"') {
        return cooked_string_end(content, j);
    }
    None
}

fn raw_string_end(content: &str, hash_at: usize) -> Option<usize> {
    let b = content.as_bytes();
    let mut j = hash_at;
    let mut hashes = 0usize;
    while j < b.len() && b[j] == b'#' {
        hashes += 1;
        j += 1;
    }
    if j >= b.len() || b[j] != b'"' {
        return None;
    }
    j += 1;
    let tail = &content[j..];
    let marker = format!("\"{}", "#".repeat(hashes));
    tail.find(&marker).map(|k| j + k + marker.len())
}

fn cooked_string_end(content: &str, quote_at: usize) -> Option<usize> {
    let b = content.as_bytes();
    let mut j = quote_at + 1;
    while j < b.len() {
        if b[j] == b'\\' {
            j = (j + 2).min(b.len());
            continue;
        }
        if b[j] == b'"' {
            return Some(j + 1);
        }
        j += 1;
    }
    Some(content.len())
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub(super) fn enclosing_inline(mods: &[InlineMod], byte: usize) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (idx, m) in mods.iter().enumerate() {
        if m.body_start <= byte && byte < m.body_end {
            match best {
                None => best = Some(idx),
                Some(prev) => {
                    let prev_span = mods[prev].body_end - mods[prev].body_start;
                    let span = m.body_end - m.body_start;
                    if span < prev_span {
                        best = Some(idx);
                    }
                }
            }
        }
    }
    best
}

pub(super) fn file_has_method(impls: &[ImplBlock], type_name: &str, method: &str) -> bool {
    let type_name = type_base(type_name);
    impls.iter().any(|b| {
        b.methods.iter().any(|m| m == method)
            && (b.owner_type == type_name || b.trait_name.as_deref() == Some(type_name))
    })
}
