//! Real CLI updates: locality, route ownership changes, deletion and cache fallback.
use std::{fs, path::Path, process::Command};
use serde_json::Value;

fn analyze(root: &Path, mode: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_code-explorer"))
        .args(["analyze", root.to_str().unwrap(), mode, "--skip-git"])
        .env("CODE_EXPLORER_HOME", root.join(".codeexplorer/home"))
        .output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&fs::read(root.join(".codeexplorer/analyze.json")).unwrap()).unwrap()
}

fn assert_full_equivalent(root: &Path) {
    let b = fs::read(root.join(".codeexplorer/graph.bin")).unwrap();
    analyze(root, "--force");
    let c = fs::read(root.join(".codeexplorer/graph.bin")).unwrap();
    assert!(b == c, "incremental and full serialized graphs differ");
}

#[test]
fn local_enrichments_invalidate_changed_files_and_direct_neighbors() {
    let root = std::env::temp_dir().join(format!("local-enrichment-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("value.ts"), "// TODO old\nexport function value() { return 1; }\n").unwrap();
    fs::write(root.join("caller.ts"), "import { value } from './value';\nexport function run() { return value(); }\n").unwrap();
    fs::write(root.join("a.ts"), "export function first() {}\napp.get('/same', first);\n").unwrap();
    fs::write(root.join("z.ts"), "export function last() {}\napp.get('/same', last);\n").unwrap();
    analyze(&root, "--force");
    fs::write(root.join("value.ts"), "// TODO replaced\nexport function value() { return 2; }\n").unwrap();
    let report = analyze(&root, "--incremental");
    assert_eq!(report["parsed_files"], 1);
    for phase in ["todos", "api_surface"] {
        assert_eq!(report["local_enrichments"][phase]["scanned_files"], 2, "{phase}: changed file plus unchanged caller");
        assert_eq!(report["local_enrichments"][phase]["reused_files"], 2);
    }
    assert_full_equivalent(&root);
    // A new dependency has no edge in the previous snapshot: include both
    // the old callee and the newly resolved callee in the invalidation set.
    fs::write(root.join("caller.ts"), "import { first } from './a';\nexport function run() { return first(); }\n").unwrap();
    let report = analyze(&root, "--incremental");
    for phase in ["todos", "api_surface"] {
        assert_eq!(report["local_enrichments"][phase]["scanned_files"], 3, "{phase}: old and new neighbors");
    }
    assert_full_equivalent(&root);
    // Losing duplicate declarations must remain available when ownership changes.
    fs::write(root.join("a.ts"), "export function first() {}\napp.get('/other', first);\n").unwrap();
    analyze(&root, "--incremental");
    assert_full_equivalent(&root);
    fs::remove_file(root.join("z.ts")).unwrap();
    analyze(&root, "--incremental");
    assert_full_equivalent(&root);
    fs::write(root.join(".codeexplorer/local-enrichment.bin"), "broken").unwrap();
    let fallback = analyze(&root, "--incremental");
    assert_eq!(fallback["local_enrichments"]["todos"]["scanned_files"], 3);
    assert_full_equivalent(&root);
    fs::remove_dir_all(root).unwrap();
}
