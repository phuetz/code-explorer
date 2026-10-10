use std::process::{Command, Stdio};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

fn run_test(cmd: &str, args: &[&str]) {
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn");

    if let Some(mut stdout) = child.stdout.take() {
        let mut buf = [0; 100];
        // Read just a little bit
        let _ = stdout.read(&mut buf);
    }
    // child.stdout is dropped here, closing the pipe

    let status = child.wait().expect("Failed to wait");

    // Read stderr to check for panic
    let mut stderr_str = String::new();
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut stderr_str);
    }

    assert!(!stderr_str.contains("panicked"), "Found 'panicked' in stderr:\n{}", stderr_str);

    #[cfg(unix)]
    {
        // Assert that the process was terminated by SIGPIPE (13)
        // or exited without panic (0, 141, or another code depending on context,
        // but no panic signal/101 code).
        if let Some(signal) = status.signal() {
            assert_eq!(signal, 13, "Expected SIGPIPE (13), got {}", signal);
        } else {
            let code = status.code().unwrap_or(-1);
            assert!(code != 101, "Expected not to exit with 101 (panic), got {}", code);
        }
    }
}

#[test]
fn test_cli_broken_pipe_status() {
    let bin = env!("CARGO_BIN_EXE_code-explorer");
    run_test(bin, &["status"]);
}

#[test]
fn test_cli_broken_pipe_query() {
    let bin = env!("CARGO_BIN_EXE_code-explorer");
    run_test(bin, &["query", "main"]);
}

#[test]
fn test_cli_broken_pipe_analyze_help() {
    let bin = env!("CARGO_BIN_EXE_code-explorer");
    run_test(bin, &["analyze", "--help"]);
}
