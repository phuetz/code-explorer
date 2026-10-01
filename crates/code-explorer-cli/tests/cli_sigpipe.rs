#![cfg(unix)]

use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

#[test]
fn test_sigpipe_no_panic() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bin_path = env!("CARGO_BIN_EXE_code-explorer");
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);

    let child = Command::new(bin_path)
        .arg("list")
        .env("CODE_EXPLORER_HOME", temp_dir.path())
        .env("HOME", temp_dir.path())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start code-explorer");

    let output = child.wait_with_output().expect("Failed to wait for child");
    assert_eq!(
        output.status.signal(),
        Some(libc::SIGPIPE),
        "expected SIGPIPE after closing stdout; status: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
