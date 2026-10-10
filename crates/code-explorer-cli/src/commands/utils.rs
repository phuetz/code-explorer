use code_explorer_core::storage::{git, repo_manager};
use std::path::Path;

pub fn check_and_warn_stale_index(repo_path: &Path) {
    let storage_paths = repo_manager::get_storage_paths(repo_path);
    if let Ok(Some(meta)) = repo_manager::load_meta(&storage_paths.storage_path) {
        if let Some(current_commit) = git::current_commit(repo_path) {
            if current_commit != meta.last_commit || git::has_uncommitted_changes(repo_path) {
                eprintln!(
                    "WARNING: index perime (commit {}, HEAD {})",
                    meta.last_commit, current_commit
                );
            }
        } else if git::has_uncommitted_changes(repo_path) {
            eprintln!(
                "WARNING: index perime (commit {}, HEAD unknown)",
                meta.last_commit
            );
        }
    }
}
