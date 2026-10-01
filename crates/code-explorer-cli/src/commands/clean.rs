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
    let mut failures = 0;
    let mut remaining_entries = Vec::new();
    let global_dir = repo_manager::get_global_dir().canonicalize()?;

    for entry in entries.into_iter() {
        let storage = std::path::Path::new(&entry.storage_path);

        if storage.exists() {
            // Safety checks before deletion
            let is_safe = storage.file_name().and_then(|n| n.to_str()) == Some(".codeexplorer")
                && !storage.is_symlink()
                && storage
                    .canonicalize()
                    .is_ok_and(|resolved| resolved != global_dir);

            if !is_safe {
                eprintln!(
                    "Refusing to delete {}: not a Code Explorer index directory",
                    entry.storage_path
                );
                failures += 1;
                remaining_entries.push(entry);
                continue;
            }

            match std::fs::remove_dir_all(storage) {
                Ok(_) => {
                    println!("Deleted: {} ({})", entry.name, entry.storage_path);
                    cleaned += 1;
                }
                Err(e) => {
                    eprintln!("Failed to delete {}: {e}", entry.storage_path);
                    failures += 1;
                    remaining_entries.push(entry);
                }
            }
        }
    }

    // Rewrite the registry, keeping only the entries that failed or were refused
    repo_manager::write_registry(&remaining_entries)?;

    println!();
    let total = cleaned + failures;
    println!("Cleaned {cleaned}/{total} repositories.");
    if failures == 0 {
        println!("Registry cleared.");
    } else {
        println!("Kept {} entries in the registry.", remaining_entries.len());
    }

    if failures > 0 {
        anyhow::bail!("Some indexes could not be cleaned");
    }
    Ok(())
}
