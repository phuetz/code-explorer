use code_explorer_core::config::languages::SupportedLanguage;

use crate::languages::*;
use crate::provider::LanguageProvider;

/// Get the language provider for a given language.
///
/// Uses static dispatch via match on the 13 language variants.
/// Returns a reference to a lazily-initialized static provider instance.
pub fn get_provider(lang: SupportedLanguage) -> &'static dyn LanguageProvider {
    match lang {
        SupportedLanguage::TypeScript => &TypeScriptProvider,
        SupportedLanguage::JavaScript => &JavaScriptProvider,
        SupportedLanguage::Python => &PythonProvider,
        SupportedLanguage::Java => &JavaProvider,
        SupportedLanguage::Go => &GoProvider,
        SupportedLanguage::Rust => &RustProvider,
        SupportedLanguage::C => &CProvider,
        SupportedLanguage::CPlusPlus => &CppProvider,
        SupportedLanguage::CSharp => &CSharpProvider,
        SupportedLanguage::Php => &PhpProvider,
        SupportedLanguage::Ruby => &RubyProvider,
        SupportedLanguage::Kotlin => &KotlinProvider,
        SupportedLanguage::Swift => &SwiftProvider,
        SupportedLanguage::Razor => &RazorProvider,
    }
}

/// Get the language provider for a file based on its extension.
pub fn get_provider_for_file(filename: &str) -> Option<&'static dyn LanguageProvider> {
    SupportedLanguage::from_filename(filename).map(get_provider)
}

/// All language providers.
pub fn all_providers() -> Vec<&'static dyn LanguageProvider> {
    SupportedLanguage::all()
        .iter()
        .map(|lang| get_provider(*lang))
        .collect()
}
