use serde::{Deserialize, Serialize};

/// Supported programming languages for code analysis.
/// Matches the TypeScript `SupportedLanguages` enum exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportedLanguage {
    JavaScript,
    TypeScript,
    Python,
    Java,
    C,
    #[serde(rename = "cpp")]
    CPlusPlus,
    #[serde(rename = "csharp")]
    CSharp,
    Go,
    Ruby,
    Rust,
    #[serde(rename = "php")]
    Php,
    Kotlin,
    Swift,
    Razor,
}

impl SupportedLanguage {
    /// File extensions associated with this language.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::JavaScript => &[".js", ".jsx", ".mjs", ".cjs"],
            Self::TypeScript => &[".ts", ".tsx", ".mts", ".cts"],
            Self::Python => &[".py", ".pyi"],
            Self::Java => &[".java"],
            Self::C => &[".c", ".h"],
            Self::CPlusPlus => &[".cpp", ".hpp", ".cc", ".hh", ".cxx", ".hxx"],
            Self::CSharp => &[".cs"],
            Self::Go => &[".go"],
            Self::Ruby => &[".rb"],
            Self::Rust => &[".rs"],
            Self::Php => &[".php"],
            Self::Kotlin => &[".kt", ".kts"],
            Self::Swift => &[".swift"],
            Self::Razor => &[".cshtml", ".razor"],
        }
    }

    /// Detect language from a file extension (including the dot).
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext_lower = ext.to_lowercase();
        match ext_lower.as_str() {
            ".js" | ".jsx" | ".mjs" | ".cjs" => Some(Self::JavaScript),
            ".ts" | ".mts" | ".cts" => Some(Self::TypeScript),
            ".tsx" => Some(Self::TypeScript),
            ".py" | ".pyi" => Some(Self::Python),
            ".java" => Some(Self::Java),
            ".c" | ".h" => Some(Self::C),
            ".cpp" | ".hpp" | ".cc" | ".hh" | ".cxx" | ".hxx" => Some(Self::CPlusPlus),
            ".cs" => Some(Self::CSharp),
            ".go" => Some(Self::Go),
            ".rb" => Some(Self::Ruby),
            ".rs" => Some(Self::Rust),
            ".php" => Some(Self::Php),
            ".kt" | ".kts" => Some(Self::Kotlin),
            ".swift" => Some(Self::Swift),
            ".cshtml" | ".razor" => Some(Self::Razor),
            // Non-code files needed for ASP.NET MVC analysis (EDMX, config)
            ".edmx" | ".config" => Some(Self::CSharp),
            _ => None,
        }
    }

    /// Detect language from a full filename.
    pub fn from_filename(filename: &str) -> Option<Self> {
        let path = std::path::Path::new(filename);
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| Self::from_extension(&format!(".{ext}")))
    }

    /// String representation matching TypeScript enum values.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Python => "python",
            Self::Java => "java",
            Self::C => "c",
            Self::CPlusPlus => "cpp",
            Self::CSharp => "csharp",
            Self::Go => "go",
            Self::Ruby => "ruby",
            Self::Rust => "rust",
            Self::Php => "php",
            Self::Kotlin => "kotlin",
            Self::Swift => "swift",
            Self::Razor => "razor",
        }
    }

    /// All supported languages.
    pub fn all() -> &'static [Self] {
        &[
            Self::JavaScript,
            Self::TypeScript,
            Self::Python,
            Self::Java,
            Self::C,
            Self::CPlusPlus,
            Self::CSharp,
            Self::Go,
            Self::Ruby,
            Self::Rust,
            Self::Php,
            Self::Kotlin,
            Self::Swift,
            Self::Razor,
        ]
    }
}

impl std::fmt::Display for SupportedLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_extension() {
        assert_eq!(
            SupportedLanguage::from_extension(".ts"),
            Some(SupportedLanguage::TypeScript)
        );
        assert_eq!(
            SupportedLanguage::from_extension(".py"),
            Some(SupportedLanguage::Python)
        );
        assert_eq!(
            SupportedLanguage::from_extension(".rs"),
            Some(SupportedLanguage::Rust)
        );
        assert_eq!(SupportedLanguage::from_extension(".unknown"), None);
    }

    #[test]
    fn test_from_filename() {
        assert_eq!(
            SupportedLanguage::from_filename("main.go"),
            Some(SupportedLanguage::Go)
        );
        assert_eq!(
            SupportedLanguage::from_filename("App.tsx"),
            Some(SupportedLanguage::TypeScript)
        );
        assert_eq!(SupportedLanguage::from_filename("README.md"), None);
    }

    #[test]
    fn test_serde_roundtrip() {
        let lang = SupportedLanguage::CPlusPlus;
        let json = serde_json::to_string(&lang).unwrap();
        assert_eq!(json, "\"cpp\"");
        let parsed: SupportedLanguage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, lang);
    }

    #[test]
    fn test_all_languages_count() {
        assert_eq!(SupportedLanguage::all().len(), 14);
    }
}
