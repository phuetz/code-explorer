use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_core::resolution::context::ResolutionContext;
use code_explorer_core::resolution::types::*;
use code_explorer_core::symbol::SymbolTable;

use code_explorer_lang::registry::get_provider;

use crate::phases::parsing::ExtractedData;
use crate::IngestError;

/// Process heritage (inheritance/implementation) relationships.
///
/// For each extracted heritage clause (extends/implements):
/// 1. Resolve the class and parent names using the symbol table
/// 2. Create EXTENDS or IMPLEMENTS edges based on heritage kind and language
pub fn process_heritage(
    graph: &mut KnowledgeGraph,
    extracted: &ExtractedData,
    symbol_table: &SymbolTable,
    import_map: &ImportMap,
    named_import_map: &NamedImportMap,
    re_export_map: &ReExportMap,
) -> Result<(), IngestError> {
    let package_map = PackageMap::new();
    let module_alias_map = ModuleAliasMap::new();
    let mut ctx = ResolutionContext::new(
        symbol_table,
        import_map,
        &package_map,
        named_import_map,
        re_export_map,
        &module_alias_map,
    );

    let mut edge_count = 0;

    for heritage in &extracted.heritage {
        ctx.enable_cache(&heritage.file_path);

        // Resolve the class
        let class_id =
            if let Some(resolved) = ctx.resolve(&heritage.class_name, &heritage.file_path) {
                resolved.candidates.first().map(|c| c.node_id.clone())
            } else {
                None
            };

        // Resolve the parent
        let parent_id =
            if let Some(resolved) = ctx.resolve(&heritage.parent_name, &heritage.file_path) {
                resolved.candidates.first().map(|c| c.node_id.clone())
            } else {
                None
            };

        if let (Some(class_id), Some(parent_id)) = (class_id, parent_id) {
            // Determine edge type based on heritage kind and language
            let rel_type = match heritage.kind.as_str() {
                "implements" | "heritage.implements" => RelationshipType::Implements,
                "trait" | "heritage.trait" | "uses" => RelationshipType::Implements,
                _ => {
                    // Check language-specific interface patterns
                    let lang = code_explorer_core::config::languages::SupportedLanguage::from_filename(
                        &heritage.file_path,
                    );
                    if let Some(lang) = lang {
                        let provider = get_provider(lang);
                        if let Some(pattern) = provider.interface_name_pattern() {
                            if pattern.is_match(&heritage.parent_name) {
                                RelationshipType::Implements
                            } else {
                                provider.heritage_default_edge()
                            }
                        } else {
                            provider.heritage_default_edge()
                        }
                    } else {
                        RelationshipType::Extends
                    }
                }
            };

            let edge_id = format!("heritage_{}_{}", class_id, parent_id);
            if graph.get_relationship(&edge_id).is_none() {
                graph.add_relationship(GraphRelationship {
                    id: edge_id,
                    source_id: class_id,
                    target_id: parent_id,
                    rel_type,
                    confidence: 0.9,
                    reason: heritage.kind.clone(),
                    step: None,
                });
                edge_count += 1;
            }
        }
    }

    tracing::info!("Created {} heritage edges", edge_count);
    Ok(())
}
