//! The `clean` command: delete Code Explorer index data.

use code_explorer_core::storage::repo_manager;

pub fn run(force: bool, all: bool) -> anyhow::Result<()> {
    if all {
        clean_all(force)
    } else {
        clean_current(force)
    }
}

fn clean_current(force: bool) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let storage_paths = repo_manager::get_storage_paths(&cwd);

    if !storage_paths.storage_path.exists() {
        println!("No Code Explorer index found in {}", cwd.display());
        return Ok(());
    }

    if !force {
        println!(
            "This will delete the Code Explorer index at {}",
            storage_paths.storage_path.display()
        );
        println!("Use --force to skip this confirmation.");
        // In a real CLI we'd prompt for confirmation. Since we can't
        // do interactive input easily, we require --force.
        return Ok(());
    }

    // Delete the .codeexplorer directory
    std::fs::remove_dir_all(&storage_paths.storage_path)?;
    println!(
        "Deleted Code Explorer index at {}",
        storage_paths.storage_path.display()
    );

    // Unregister from global registry
    if let Err(e) = repo_manager::unregister_repo(&cwd) {
        eprintln!("Warning: failed to update registry: {e}");
    }

    println!("Repository unregistered from global registry.");
    Ok(())
}

fn clean_all(force: bool) -> anyhow::Result<()> {
    let entries = repo_manager::read_registry()?;

    if entries.is_empty() {
        println!("No repositories indexed.");
        return Ok(());
    }

    if !force {
        println!(
            "This will delete Code Explorer indexes for {} repositories:",
            entries.len()
        );
        for entry in &entries {
            println!("  {} ({})", entry.name, entry.path);
        }
        println!();
        println!("Use --force to proceed.");
        return Ok(());
    }

    let mut cleaned = 0;
    for entry in &entries {
        let storage = std::path::Path::new(&entry.storage_path);
        if storage.exists() {
            match std::fs::remove_dir_all(storage) {
                Ok(_) => {
                    println!("Deleted: {} ({})", entry.name, entry.storage_path);
                    cleaned += 1;
                }
                Err(e) => {
                    eprintln!("Failed to delete {}: {e}", entry.storage_path);
                }
            }
        }
    }

    // Clear the registry
    repo_manager::write_registry(&[])?;

    println!();
    println!("Cleaned {cleaned}/{} repositories.", entries.len());
    println!("Registry cleared.");
    Ok(())
}
