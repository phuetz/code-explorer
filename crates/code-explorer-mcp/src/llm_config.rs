//! LLM config loader mirrored from `code-explorer-cli/src/commands/generate/enrichment.rs`.
//!
//! Kept minimal (file read + deserialize) to avoid pulling CLI-specific code
//! into the MCP crate. If a third caller needs the same format, promote this
//! to `code-explorer-core::llm::config` and deduplicate.

use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmConfig {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
    #[serde(default, alias = "apiKey")]
    pub api_key: String,
    #[serde(alias = "baseUrl")]
    pub base_url: String,
    pub model: String,
    #[serde(
        default,
        alias = "reasoningEffort",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_effort: Option<String>,
    #[serde(default, alias = "maxTokens", skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    // Optional big-context fallback fields. Not consumed by the MCP reranker
    // today (rerank prompts are always small) but accepted here so a single
    // chat-config.json edited for the CLI enrichment pipeline doesn't fail
    // to deserialize when MCP loads the same file.
    #[serde(
        default,
        alias = "bigContextModel",
        skip_serializing_if = "Option::is_none"
    )]
    pub big_context_model: Option<String>,
    #[serde(
        default,
        alias = "bigContextThresholdBytes",
        skip_serializing_if = "Option::is_none"
    )]
    pub big_context_threshold_bytes: Option<usize>,
    #[serde(
        default,
        alias = "bigContextMaxTokens",
        skip_serializing_if = "Option::is_none"
    )]
    pub big_context_max_tokens: Option<u32>,
}

/// Resolve `~/.codeexplorer/chat-config.json` across OS home-dir env variations.
pub fn load_llm_config() -> Option<LlmConfig> {
    load_llm_config_raw().map(hydrate_api_key_from_env)
}

/// Load the persisted config without hydrating an API key from environment.
///
/// This is used when editing the local config so a blank API key field in the
/// UI does not accidentally persist an environment variable value back to disk.
pub fn load_llm_config_raw() -> Option<LlmConfig> {
    for p in config_path_candidates() {
        if p.exists() {
            if let Ok(raw) = std::fs::read_to_string(&p) {
                let raw = raw.trim_start_matches('\u{feff}');
                if let Ok(cfg) = serde_json::from_str::<LlmConfig>(raw) {
                    return Some(cfg);
                }
            }
        }
    }
    None
}

pub fn save_llm_config(config: &LlmConfig) -> io::Result<PathBuf> {
    let path = default_config_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Impossible de trouver le dossier utilisateur pour ~/.codeexplorer/chat-config.json",
        )
    })?;
    save_llm_config_at(&path, config)?;
    Ok(path)
}

pub fn save_llm_config_at(path: &Path, config: &LlmConfig) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(config)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    std::fs::write(path, format!("{raw}\n"))
}

fn config_path_candidates() -> Vec<PathBuf> {
    home_dir_candidates()
        .into_iter()
        .map(|home| home.join(".codeexplorer").join("chat-config.json"))
        .collect()
}

fn default_config_path() -> Option<PathBuf> {
    config_path_candidates().into_iter().next()
}

fn home_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for home in [
        std::env::var("USERPROFILE").ok(),
        std::env::var("HOME").ok(),
        std::env::var("HOMEDRIVE").ok().and_then(|d| {
            std::env::var("HOMEPATH")
                .ok()
                .map(|p| format!("{}{}", d, p))
        }),
    ]
    .into_iter()
    .flatten()
    {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            let path = PathBuf::from(trimmed);
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
    }
    candidates
}

pub fn display_provider(config: &LlmConfig) -> String {
    let provider = config.provider.trim();
    if provider.is_empty() {
        infer_provider_from_base_url(&config.base_url)
    } else {
        provider.to_string()
    }
}

fn env_api_key_candidates(config: &LlmConfig) -> &'static [&'static str] {
    let provider = if config.provider.trim().is_empty() {
        infer_provider_from_base_url(&config.base_url)
    } else {
        config.provider.trim().to_ascii_lowercase()
    };
    match provider.as_str() {
        "openai" => &["OPENAI_API_KEY", "CODE_EXPLORER_API_KEY"],
        "anthropic" => &["ANTHROPIC_API_KEY", "CODE_EXPLORER_API_KEY"],
        "openrouter" => &["OPENROUTER_API_KEY", "CODE_EXPLORER_API_KEY"],
        "gemini" | "google" => &["GEMINI_API_KEY", "GOOGLE_API_KEY", "CODE_EXPLORER_API_KEY"],
        provider if provider.contains("ollama") || provider.contains("local") => &[],
        _ => &["CODE_EXPLORER_API_KEY"],
    }
}

fn infer_provider_from_base_url(base_url: &str) -> String {
    let lower = base_url.to_ascii_lowercase();
    if lower.contains("generativelanguage.googleapis.com") || lower.contains("googleapis.com") {
        "gemini".to_string()
    } else if lower.contains("openrouter.ai") {
        "openrouter".to_string()
    } else if lower.contains("anthropic.com") {
        "anthropic".to_string()
    } else if lower.contains("localhost") || lower.contains("127.0.0.1") {
        "local".to_string()
    } else {
        "openai".to_string()
    }
}

fn hydrate_api_key_from_env(config: LlmConfig) -> LlmConfig {
    hydrate_api_key_from_sources(config, |name| std::env::var(name).ok())
}

fn hydrate_api_key_from_sources(
    mut config: LlmConfig,
    get_var: impl Fn(&str) -> Option<String>,
) -> LlmConfig {
    if !config.api_key.is_empty() {
        return config;
    }
    for key in env_api_key_candidates(&config) {
        if let Some(value) = get_var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                config.api_key = trimmed.to_string();
                break;
            }
        }
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(json: &str) -> LlmConfig {
        serde_json::from_str(json).expect("LlmConfig parse failed")
    }

    #[test]
    fn config_parses_without_persisted_api_key() {
        let cfg = parse_config(
            r#"{
                "provider": "ollama",
                "baseUrl": "http://localhost:11434/v1",
                "model": "llama3.2"
            }"#,
        );
        assert_eq!(cfg.api_key, "");
        assert_eq!(cfg.provider, "ollama");
    }

    #[test]
    fn config_hydrates_provider_specific_api_key() {
        let cfg = parse_config(
            r#"{
                "provider": "gemini",
                "baseUrl": "https://generativelanguage.googleapis.com/v1beta/openai",
                "model": "gemini-2.5-flash"
            }"#,
        );
        let hydrated = hydrate_api_key_from_sources(cfg, |name| match name {
            "GEMINI_API_KEY" => Some(" gem-key ".to_string()),
            _ => None,
        });
        assert_eq!(hydrated.api_key, "gem-key");
    }

    #[test]
    fn config_infers_provider_when_missing() {
        let cfg = parse_config(
            r#"{
                "baseUrl": "https://openrouter.ai/api/v1",
                "model": "openai/gpt-4o-mini",
                "reasoning_effort": "high",
                "max_tokens": 4096
            }"#,
        );
        let hydrated = hydrate_api_key_from_sources(cfg, |name| match name {
            "OPENROUTER_API_KEY" => Some("router-key".to_string()),
            _ => None,
        });
        assert_eq!(hydrated.api_key, "router-key");
        assert_eq!(display_provider(&hydrated), "openrouter");
        assert_eq!(hydrated.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(hydrated.max_tokens, Some(4096));
    }

    #[test]
    fn save_config_writes_pretty_json_without_hydrating_env_key() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "code-explorer-llm-config-test-{}-{}.json",
            std::process::id(),
            nanos
        ));
        let _ = std::fs::remove_file(&path);
        let cfg = LlmConfig {
            provider: "ollama".to_string(),
            api_key: String::new(),
            base_url: "http://localhost:11434/v1".to_string(),
            model: "llama3.2".to_string(),
            reasoning_effort: Some("medium".to_string()),
            max_tokens: Some(4096),
            big_context_model: None,
            big_context_threshold_bytes: None,
            big_context_max_tokens: None,
        };

        save_llm_config_at(&path, &cfg).expect("save config");
        let raw = std::fs::read_to_string(&path).expect("read config");
        let _ = std::fs::remove_file(&path);

        assert!(raw.contains("\"provider\": \"ollama\""));
        assert!(raw.contains("\"api_key\": \"\""));
        assert!(raw.contains("\"base_url\": \"http://localhost:11434/v1\""));
    }
}
