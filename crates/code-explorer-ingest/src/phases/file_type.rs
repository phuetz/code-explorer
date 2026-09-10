use std::path::Path;
#[cfg(feature = "magika-detect")]
use std::sync::{Arc, Mutex, OnceLock};

use code_explorer_core::config::languages::SupportedLanguage;

#[derive(Debug, Clone)]
pub struct FileTypeDetection {
    pub language: SupportedLanguage,
    pub detector: &'static str,
    pub label: String,
    pub mime: String,
    pub confidence: f64,
}

pub fn detect_language_for_path(_abs_path: &Path, rel_path: &str) -> Option<SupportedLanguage> {
    if let Some(language) = SupportedLanguage::from_filename(rel_path) {
        return Some(language);
    }

    if !should_try_magika(rel_path) {
        return None;
    }

    #[cfg(feature = "magika-detect")]
    {
        if let Some(detection) = detect_with_magika(_abs_path) {
            return Some(detection.language);
        }
    }

    None
}

fn should_try_magika(rel_path: &str) -> bool {
    Path::new(rel_path).extension().is_none()
}

#[cfg(any(feature = "magika-detect", test))]
fn language_from_magika_label(label: &str) -> Option<SupportedLanguage> {
    match label {
        "javascript" => Some(SupportedLanguage::JavaScript),
        "typescript" => Some(SupportedLanguage::TypeScript),
        "python" => Some(SupportedLanguage::Python),
        "java" => Some(SupportedLanguage::Java),
        "c" => Some(SupportedLanguage::C),
        "cpp" => Some(SupportedLanguage::CPlusPlus),
        "cs" => Some(SupportedLanguage::CSharp),
        "go" => Some(SupportedLanguage::Go),
        "ruby" => Some(SupportedLanguage::Ruby),
        "rust" => Some(SupportedLanguage::Rust),
        "php" => Some(SupportedLanguage::Php),
        "kotlin" => Some(SupportedLanguage::Kotlin),
        "swift" => Some(SupportedLanguage::Swift),
        _ => None,
    }
}

#[cfg(feature = "magika-detect")]
fn detect_with_magika(abs_path: &Path) -> Option<FileTypeDetection> {
    static SESSION: OnceLock<Mutex<Option<magika::Session>>> = OnceLock::new();

    let session = SESSION.get_or_init(|| {
        configure_ort_logging();
        Mutex::new(magika::Session::new().ok())
    });
    let mut guard = session.lock().ok()?;
    let session = guard.as_mut()?;
    let file_type = match session.identify_file_sync(abs_path) {
        Ok(file_type) => file_type,
        Err(err) => {
            tracing::debug!(
                "Magika detection failed for {}: {}",
                abs_path.display(),
                err
            );
            return None;
        }
    };
    let info = file_type.info();
    let language = language_from_magika_label(info.label)?;
    Some(FileTypeDetection {
        language,
        detector: "magika",
        label: info.label.to_string(),
        mime: info.mime_type.to_string(),
        confidence: file_type.score() as f64,
    })
}

#[cfg(feature = "magika-detect")]
fn configure_ort_logging() {
    static ORT_LOGGING: OnceLock<()> = OnceLock::new();

    ORT_LOGGING.get_or_init(|| {
        let _ = ort::init()
            .with_name("code-explorer-magika")
            .with_logger(Arc::new(|level, category, id, _location, message| {
                let target = "ort::logging";
                match level {
                    ort::logging::LogLevel::Warning => {
                        tracing::warn!(target, category, id, "{message}");
                    }
                    ort::logging::LogLevel::Error | ort::logging::LogLevel::Fatal => {
                        tracing::error!(target, category, id, "{message}");
                    }
                    ort::logging::LogLevel::Verbose | ort::logging::LogLevel::Info => {}
                }
            }))
            .commit();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_supported_magika_labels() {
        assert_eq!(
            language_from_magika_label("typescript"),
            Some(SupportedLanguage::TypeScript)
        );
        assert_eq!(
            language_from_magika_label("cs"),
            Some(SupportedLanguage::CSharp)
        );
        assert_eq!(
            language_from_magika_label("rust"),
            Some(SupportedLanguage::Rust)
        );
    }

    #[test]
    fn ignores_unsupported_magika_labels() {
        assert_eq!(language_from_magika_label("markdown"), None);
        assert_eq!(language_from_magika_label("unknown"), None);
    }

    #[test]
    fn tries_magika_only_for_extensionless_files() {
        assert!(should_try_magika("script"));
        assert!(should_try_magika("bin/tool"));
        assert!(!should_try_magika("src/main.rs"));
        assert!(!should_try_magika("README.md"));
    }
}
