//! Project-level configuration from code-explorer.toml

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectConfig {
    #[serde(default)]
    pub project: ProjectInfo,
    #[serde(default)]
    pub ingestion: IngestionConfig,
    #[serde(default)]
    pub generate: GenerateConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IngestionConfig {
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: u64,
    #[serde(default)]
    pub ignored_dirs: Vec<String>,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            max_file_size_mb: default_max_file_size(),
            ignored_dirs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateConfig {
    #[serde(default = "default_lang")]
    pub default_lang: String,
    #[serde(default = "default_profile")]
    pub enrich_profile: String,
}

impl Default for GenerateConfig {
    fn default() -> Self {
        Self {
            default_lang: default_lang(),
            enrich_profile: default_profile(),
        }
    }
}

fn default_max_file_size() -> u64 {
    20
}
fn default_lang() -> String {
    "fr".to_string()
}
fn default_profile() -> String {
    "quality".to_string()
}

impl ProjectConfig {
    /// Load from code-explorer.toml in the given directory.
    /// Returns Default if file doesn't exist.
    pub fn load(project_dir: &Path) -> Self {
        let config_path = project_dir.join("code-explorer.toml");
        if !config_path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => {
                    tracing::info!("Loaded project config from {}", config_path.display());
                    config
                }
                Err(e) => {
                    tracing::warn!("Failed to parse {}: {}", config_path.display(), e);
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read {}: {}", config_path.display(), e);
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ProjectConfig::default();
        assert_eq!(config.ingestion.max_file_size_mb, 20);
        assert_eq!(config.generate.default_lang, "fr");
        assert_eq!(config.generate.enrich_profile, "quality");
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
[project]
name = "Test Project"
description = "A test"

[ingestion]
max_file_size_mb = 50
ignored_dirs = ["bin", "obj"]

[generate]
default_lang = "en"
enrich_profile = "strict"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project.name.as_deref(), Some("Test Project"));
        assert_eq!(config.ingestion.max_file_size_mb, 50);
        assert_eq!(config.ingestion.ignored_dirs, vec!["bin", "obj"]);
        assert_eq!(config.generate.default_lang, "en");
    }

    #[test]
    fn test_load_nonexistent() {
        let config = ProjectConfig::load(Path::new("/nonexistent/path"));
        assert_eq!(config.ingestion.max_file_size_mb, 20); // Default
    }

    #[test]
    fn test_partial_toml() {
        let toml_str = r#"
[project]
name = "Partial"
"#;
        let config: ProjectConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.project.name.as_deref(), Some("Partial"));
        assert_eq!(config.ingestion.max_file_size_mb, 20); // Default preserved
    }

    #[test]
    fn test_load_from_temp_dir() {
        let dir = std::env::temp_dir().join(format!(
            "code_explorer_config_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let config_path = dir.join("code-explorer.toml");
        std::fs::write(
            &config_path,
            r#"
[project]
name = "TempProject"
description = "loaded from disk"

[ingestion]
max_file_size_mb = 100
ignored_dirs = ["target", "node_modules"]
"#,
        )
        .unwrap();

        let config = ProjectConfig::load(&dir);
        assert_eq!(config.project.name.as_deref(), Some("TempProject"));
        assert_eq!(
            config.project.description.as_deref(),
            Some("loaded from disk")
        );
        assert_eq!(config.ingestion.max_file_size_mb, 100);
        assert_eq!(
            config.ingestion.ignored_dirs,
            vec!["target", "node_modules"]
        );
        // generate section not specified, should use defaults
        assert_eq!(config.generate.default_lang, "fr");
        assert_eq!(config.generate.enrich_profile, "quality");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_malformed_toml_returns_default() {
        let dir = std::env::temp_dir().join(format!(
            "code_explorer_config_bad_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let config_path = dir.join("code-explorer.toml");
        std::fs::write(&config_path, "this is not [[ valid toml !!!").unwrap();

        let config = ProjectConfig::load(&dir);
        // Should fall back to defaults, not panic
        assert_eq!(config.ingestion.max_file_size_mb, 20);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
