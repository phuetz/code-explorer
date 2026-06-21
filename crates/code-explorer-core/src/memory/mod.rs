use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MemoryStore {
    pub facts: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum MemoryScope {
    Global,
    Project,
}

impl MemoryStore {
    fn global_path() -> PathBuf {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir());
        home.join(".codeexplorer").join("global-memory.json")
    }

    fn project_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".codeexplorer").join("project-memory.json")
    }

    pub fn load(scope: MemoryScope, repo_root: Option<&Path>) -> Self {
        let path = match scope {
            MemoryScope::Global => Self::global_path(),
            MemoryScope::Project => {
                if let Some(root) = repo_root {
                    Self::project_path(root)
                } else {
                    return Self::default();
                }
            }
        };

        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(store) = serde_json::from_str::<MemoryStore>(&content) {
                    return store;
                }
            }
        }
        Self::default()
    }

    pub fn save(&mut self, scope: MemoryScope, repo_root: Option<&Path>) -> Result<(), String> {
        let path = match scope {
            MemoryScope::Global => Self::global_path(),
            MemoryScope::Project => {
                if let Some(root) = repo_root {
                    Self::project_path(root)
                } else {
                    return Err("Project root is required for project scope memory".into());
                }
            }
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;

        // Atomic write: write to .tmp then rename to avoid corruption on crash
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &content).map_err(|e| format!("write tmp: {}", e))?;
        fs::rename(&tmp_path, &path).map_err(|e| format!("rename: {}", e))?;
        Ok(())
    }

    pub fn add_fact(&mut self, fact: String) {
        if !self.facts.contains(&fact) {
            self.facts.push(fact);
        }
    }
}

pub fn build_memory_context(repo_root: Option<&Path>) -> String {
    let global_memory = MemoryStore::load(MemoryScope::Global, None);
    let project_memory = MemoryStore::load(MemoryScope::Project, repo_root);

    let mut context = String::new();

    if !global_memory.facts.is_empty() {
        context.push_str("## Global User Preferences\n");
        for fact in &global_memory.facts {
            context.push_str(&format!("- {}\n", fact));
        }
        context.push('\n');
    }

    if !project_memory.facts.is_empty() {
        context.push_str("## Project-Specific Facts & Context\n");
        for fact in &project_memory.facts {
            context.push_str(&format!("- {}\n", fact));
        }
        context.push('\n');
    }

    context
}
