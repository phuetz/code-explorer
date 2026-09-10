//! The `list` command: display all indexed repositories.

use code_explorer_core::storage::repo_manager;

pub fn run() -> anyhow::Result<()> {
    let entries = repo_manager::read_registry()?;

    if entries.is_empty() {
        println!("No repositories indexed yet.");
        println!("Run `code-explorer analyze /path/to/repo` to get started.");
        return Ok(());
    }

    println!("Indexed repositories ({}):", entries.len());
    println!();

    for entry in &entries {
        println!("  {} ({})", entry.name, entry.path);
        println!("    Indexed at: {}", entry.indexed_at);
        println!("    Commit: {}", entry.last_commit);
        if let Some(stats) = &entry.stats {
            let parts: Vec<String> = [
                stats.files.map(|n| format!("{n} files")),
                stats.nodes.map(|n| format!("{n} nodes")),
                stats.edges.map(|n| format!("{n} edges")),
                stats.communities.map(|n| format!("{n} communities")),
                stats.processes.map(|n| format!("{n} processes")),
            ]
            .into_iter()
            .flatten()
            .collect();
            if !parts.is_empty() {
                println!("    Stats: {}", parts.join(", "));
            }
        }
        println!();
    }

    Ok(())
}
