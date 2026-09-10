use std::path::Path;
use std::process::Command;

/// Check if a path is inside a git repository.
pub fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Get the current HEAD commit hash.
pub fn current_commit(repo_path: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// Get the git repository root.
pub fn git_root(path: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// Get the current branch name (`HEAD` when detached).
pub fn current_branch(repo_path: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Get the shared `.git` directory. For a linked worktree this is the `.git`
/// of the main checkout, which is how a worktree is told apart from a plain
/// clone (`--git-dir` differs from `--git-common-dir`).
pub fn git_common_dir(repo_path: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(repo_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Get the worktree-private `.git` directory.
pub fn git_dir(repo_path: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(repo_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Count commits reachable from `to` but not from `from` (`from..to`).
///
/// `None` when either revision is unknown to this checkout — after a branch
/// switch the indexed commit may not be an ancestor of HEAD at all, and the
/// caller must say "different history", not "0 commits behind".
pub fn commits_between(repo_path: &Path, from: &str, to: &str) -> Option<usize> {
    Command::new("git")
        .args(["rev-list", "--count", &format!("{from}..{to}")])
        .current_dir(repo_path)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|s| s.trim().parse::<usize>().ok())
}

/// Whether `commit` exists in this checkout.
pub fn commit_exists(repo_path: &Path, commit: &str) -> bool {
    Command::new("git")
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .current_dir(repo_path)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
