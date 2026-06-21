//! User-defined slash commands — lightweight plugin system.
//!
//! Each command pairs a `name` (the trigger after `/`) with a `template` that
//! gets sent to the chat when invoked. Templates may use `{{args}}` to
//! receive everything the user typed after the command name, e.g.
//!   /explain UserService → expands "Explain `UserService` in detail" if the
//!   template is "Explain `{{args}}` in detail".
//!
//! Commands are stored per-repo in `<.codeexplorer>/user_commands.json` so each
//! project can ship its own conventions. The chat panel matches `/<name>` in
//! the input on Enter and replaces it with the resolved template before
//! sending the message.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserCommand {
    pub id: String,
    /// Trigger name (without the leading `/`). Lowercased + alnum-only.
    pub name: String,
    /// Markdown / plain-text template; `{{args}}` is replaced at invocation.
    pub template: String,
    /// Which chat mode to switch to before sending. Default: "qa".
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserCommandsFile {
    #[serde(default)]
    pub commands: Vec<UserCommand>,
}

fn commands_path(storage: &str) -> PathBuf {
    PathBuf::from(storage).join("user_commands.json")
}

fn load(path: &std::path::Path) -> UserCommandsFile {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => UserCommandsFile::default(),
    }
}

fn save(path: &std::path::Path, file: &UserCommandsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

fn sanitize_name(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[tauri::command]
pub async fn user_commands_list(state: State<'_, AppState>) -> Result<Vec<UserCommand>, String> {
    let storage = state.active_storage_path().await?;
    Ok(load(&commands_path(&storage)).commands)
}

#[tauri::command]
pub async fn user_commands_save(
    state: State<'_, AppState>,
    command: UserCommand,
) -> Result<Vec<UserCommand>, String> {
    let storage = state.active_storage_path().await?;
    let path = commands_path(&storage);
    let mut file = load(&path);
    let mut cmd = command;
    if cmd.id.is_empty() {
        cmd.id = format!("uc_{}", Uuid::new_v4().simple());
    }
    cmd.name = sanitize_name(&cmd.name);
    if cmd.name.is_empty() {
        return Err("Command name must contain at least one alphanumeric char".into());
    }
    cmd.updated_at = chrono::Utc::now().timestamp_millis();
    // Upsert by name (case-insensitive); replaces same-name entry.
    file.commands
        .retain(|c| c.name != cmd.name && c.id != cmd.id);
    file.commands.push(cmd);
    file.commands.sort_by(|a, b| a.name.cmp(&b.name));
    save(&path, &file)?;
    Ok(file.commands)
}

#[tauri::command]
pub async fn user_commands_delete(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<UserCommand>, String> {
    let storage = state.active_storage_path().await?;
    let path = commands_path(&storage);
    let mut file = load(&path);
    file.commands.retain(|c| c.id != id);
    save(&path, &file)?;
    Ok(file.commands)
}

/// Resolve a slash command invocation into the rendered template + mode.
/// Returns None when no command matches the given name.
#[tauri::command]
pub async fn user_command_resolve(
    state: State<'_, AppState>,
    name: String,
    args: String,
) -> Result<Option<ResolvedCommand>, String> {
    let storage = state.active_storage_path().await?;
    let file = load(&commands_path(&storage));
    let lookup = sanitize_name(&name);
    Ok(file
        .commands
        .into_iter()
        .find(|c| c.name == lookup)
        .map(|c| ResolvedCommand {
            text: c.template.replace("{{args}}", args.trim()),
            mode: c.mode.unwrap_or_else(|| "qa".into()),
        }))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCommand {
    pub text: String,
    pub mode: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("Explain"), "explain");
        assert_eq!(sanitize_name("foo BAR! 42"), "foobar42");
        assert_eq!(sanitize_name("auth-flow_v2"), "auth-flow_v2");
    }

    #[test]
    fn test_args_substitution_in_template() {
        let cmd = UserCommand {
            id: "x".into(),
            name: "explain".into(),
            template: "Explain `{{args}}` step by step.".into(),
            mode: Some("qa".into()),
            description: None,
            updated_at: 0,
        };
        let rendered = cmd.template.replace("{{args}}", "UserService");
        assert_eq!(rendered, "Explain `UserService` step by step.");
    }

    #[test]
    fn test_save_load_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("code-explorer-user-cmds-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user_commands.json");
        let file = UserCommandsFile {
            commands: vec![UserCommand {
                id: "uc_1".into(),
                name: "explain".into(),
                template: "Explain {{args}}".into(),
                mode: None,
                description: None,
                updated_at: 1,
            }],
        };
        save(&path, &file).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.commands.len(), 1);
        assert_eq!(loaded.commands[0].name, "explain");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
