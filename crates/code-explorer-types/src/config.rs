use serde::{Deserialize, Serialize};

/// Supported programming languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SupportedLanguage {
    JavaScript, TypeScript, Python, Java, C,
    #[serde(rename = "cpp")] CPlusPlus,
    #[serde(rename = "csharp")] CSharp,
    Go, Ruby, Rust,
    #[serde(rename = "php")] Php,
    Kotlin, Swift, Razor,
}

impl SupportedLanguage {
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

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            ".js" | ".jsx" | ".mjs" | ".cjs" => Some(Self::JavaScript),
            ".ts" | ".tsx" | ".mts" | ".cts" => Some(Self::TypeScript),
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
            _ => None,
        }
    }

    pub fn from_filename(filename: &str) -> Option<Self> {
        std::path::Path::new(filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| Self::from_extension(&format!(".{ext}")))
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::JavaScript => "javascript", Self::TypeScript => "typescript",
            Self::Python => "python", Self::Java => "java",
            Self::C => "c", Self::CPlusPlus => "cpp",
            Self::CSharp => "csharp", Self::Go => "go",
            Self::Ruby => "ruby", Self::Rust => "rust",
            Self::Php => "php", Self::Kotlin => "kotlin",
            Self::Swift => "swift",
            Self::Razor => "razor",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::JavaScript, Self::TypeScript, Self::Python, Self::Java,
          Self::C, Self::CPlusPlus, Self::CSharp, Self::Go, Self::Ruby,
          Self::Rust, Self::Php, Self::Kotlin, Self::Swift, Self::Razor]
    }
}

impl std::fmt::Display for SupportedLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
