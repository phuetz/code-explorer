//! Real indexing oracle; no synthetic delays or benchmark harness.
use code_explorer_db::snapshot::load_snapshot;
use serde_json::Value;
use std::{fs, path::Path, process::Command, time::Instant};

fn analyze(root: &Path, mode: &str) -> std::time::Duration {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_code-explorer"))
        .args(["analyze", root.to_str().unwrap(), mode, "--skip-git"])
        .env("CODE_EXPLORER_HOME", root.join(".codeexplorer/home"))
        .env("RAYON_NUM_THREADS", "2")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    start.elapsed()
}

fn snapshot(root: &Path) -> Value {
    use code_explorer_db::inmemory::{cypher::GraphIndexes, fts::FtsIndex};
    let graph = load_snapshot(&root.join(".codeexplorer/graph.bin")).unwrap();
    let mut nodes: Vec<_> = graph
        .iter_nodes()
        .map(|n| serde_json::to_value(n).unwrap().to_string())
        .collect();
    let mut edges: Vec<_> = graph
        .iter_relationships()
        .map(|r| serde_json::to_value(r).unwrap().to_string())
        .collect();
    nodes.sort();
    edges.sort();
    let indexes = GraphIndexes::build(&graph);
    let mut incoming = indexes.incoming;
    let mut outgoing = indexes.outgoing;
    for values in incoming.values_mut().chain(outgoing.values_mut()) {
        values.sort_by_key(|(id, kind)| (id.clone(), kind.as_str()));
    }
    let fts = FtsIndex::build(&graph);
    let mut searches = std::collections::BTreeMap::new();
    for node in graph.iter_nodes() {
        let name = &node.properties.name;
        let mut hits: Vec<_> = fts
            .search(&graph, name, None, usize::MAX)
            .into_iter()
            .map(|r| (r.node_id, r.score.to_bits()))
            .collect();
        hits.sort();
        searches.insert(name.clone(), hits);
    }
    serde_json::json!({"nodes": nodes, "edges": edges,
        "incoming": incoming, "outgoing": outgoing, "text": searches})
}

#[test]
fn incremental_matches_full_and_parses_only_changes() {
    let root = std::env::temp_dir().join(format!("incremental-oracle-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/incremental");
    for entry in fs::read_dir(fixture).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), root.join(entry.file_name())).unwrap();
    }
    analyze(&root, "--force");
    let a = snapshot(&root);
    fs::write(
        root.join("value.ts"),
        "export function value() { return 2; }\n",
    )
    .unwrap();
    fs::write(
        root.join("new.ts"),
        "import { value } from \"./value\";\nexport function added() { return value(); }\n",
    )
    .unwrap();
    let incremental = analyze(&root, "--incremental");
    let b = snapshot(&root);
    let incoming = b["incoming"]["Function:value.ts:value"].as_array().unwrap();
    assert!(
        incoming
            .iter()
            .any(|edge| edge[0] == "Function:client.ts:run" && edge[1] == "CALLS"),
        "the unchanged caller must still call the modified callee"
    );
    assert!(
        incoming
            .iter()
            .any(|edge| edge[0] == "Function:new.ts:added" && edge[1] == "CALLS"),
        "the new caller must resolve to the modified callee"
    );
    let report: Value = serde_json::from_slice(
        &fs::read(root.join(".codeexplorer/analyze.json"))
            .expect("analyze must expose parsed_files"),
    )
    .unwrap();
    assert_eq!(
        report["parsed_files"], 2,
        "only changed/new files may be parsed"
    );
    let full = analyze(&root, "--force");
    let c = snapshot(&root);
    assert_ne!(a, b);
    assert_eq!(
        b, c,
        "all node properties and edges must match full rebuild"
    );
    // Renamed/removed callees must not retain stale incoming edges or text hits.
    fs::write(
        root.join("value.ts"),
        "export function renamed() { return 3; }\n",
    )
    .unwrap();
    fs::remove_file(root.join("new.ts")).unwrap();
    analyze(&root, "--incremental");
    let renamed = snapshot(&root);
    analyze(&root, "--force");
    assert_eq!(renamed, snapshot(&root));
    // Broken cache must trigger an explicit full parsing fallback.
    fs::write(root.join(".codeexplorer/parse-cache.bin"), "broken").unwrap();
    analyze(&root, "--incremental");
    let fallback: Value =
        serde_json::from_slice(&fs::read(root.join(".codeexplorer/analyze.json")).unwrap())
            .unwrap();
    assert!(fallback["fallback_reason"]
        .as_str()
        .unwrap()
        .contains("full refresh"));
    assert_eq!(fallback["parsed_files"], 20);
    assert_eq!(renamed, snapshot(&root));
    eprintln!("incremental={incremental:?}, full={full:?}");

    fs::remove_dir_all(root).unwrap();
}

/// Opt-in wall-clock check using real parsing of 320 generated source files.
/// Run `cargo test --release -p code-explorer-cli --test incremental_oracle
/// large_repository_timing -- --ignored --nocapture` on a quiet machine.
#[test]
#[ignore = "wall-clock requirement on 320 generated files; run on a quiet machine"]
fn large_repository_timing() {
    let root = std::env::temp_dir().join(format!("large-incremental-oracle-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for i in 0..320 {
        fs::write(root.join(format!("module{i}.rs")),
            format!("pub fn value_{i}() -> usize {{ {i} }}\n")).unwrap();
    }
    analyze(&root, "--force");
    let a = fs::read(root.join(".codeexplorer/graph.bin")).unwrap();
    for i in 0..3 {
        fs::write(root.join(format!("module{i}.rs")),
            format!("pub fn updated_{i}() -> usize {{ {} }}\n", i + 1)).unwrap();
    }
    let incremental = analyze(&root, "--incremental");
    let report: Value = serde_json::from_slice(
        &fs::read(root.join(".codeexplorer/analyze.json")).unwrap(),
    ).unwrap();
    assert_eq!(report["total_files"], 320);
    assert_eq!(report["parsed_files"], 3);
    let b = fs::read(root.join(".codeexplorer/graph.bin")).unwrap();
    assert!(a != b, "renamed functions must change the graph");
    let full = analyze(&root, "--force");
    assert!(b == fs::read(root.join(".codeexplorer/graph.bin")).unwrap());
    eprintln!("three changed files: incremental={incremental:?}, full={full:?}");
    assert!(incremental.as_secs_f64() <= full.as_secs_f64() * 0.3,
        "incremental {incremental:?} exceeds 30% of full {full:?}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn consecutive_full_snapshots_are_identical() {
    let root = std::env::temp_dir().join(format!("deterministic-oracle-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/incremental");
    for entry in fs::read_dir(fixture).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), root.join(entry.file_name())).unwrap();
    }
    analyze(&root, "--force");
    let a = fs::read(root.join(".codeexplorer/graph.bin")).unwrap();
    analyze(&root, "--force");
    let b = fs::read(root.join(".codeexplorer/graph.bin")).unwrap();
    assert!(a == b, "serialized full snapshots differ");
    fs::remove_dir_all(root).unwrap();
}
