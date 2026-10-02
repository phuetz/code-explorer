//! Requires local Playwright/Chromium; intentionally explicit on minimal CI hosts.
#[test]
#[ignore = "requires Node.js, local Playwright/Chromium and Python 3"]
fn html_pdf_docx_render_diagrams_without_external_requests() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/tests/offline-exports.cjs");
    let result = std::process::Command::new("node")
        .arg(script)
        .arg(env!("CARGO_BIN_EXE_code-explorer"))
        .output()
        .expect("Node.js must be installed for the offline export integration test");
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    println!("{}", String::from_utf8_lossy(&result.stdout));
}
