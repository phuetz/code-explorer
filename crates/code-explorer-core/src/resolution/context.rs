use std::collections::{HashMap, HashSet};

use crate::symbol::SymbolTable;

use super::types::*;

/// Stateful context for tiered name resolution.
///
/// Wraps the symbol table and import/package maps to provide
/// a unified resolution API with caching.
pub struct ResolutionContext<'a> {
    pub symbols: &'a SymbolTable,
    pub import_map: &'a ImportMap,
    pub package_map: &'a PackageMap,
    pub named_import_map: &'a NamedImportMap,
    pub re_export_map: &'a ReExportMap,
    pub module_alias_map: &'a ModuleAliasMap,

    /// Per-file resolution cache: (file_path, name) → result
    cache: HashMap<(String, String), Option<TieredCandidates>>,
    /// Currently active file path for caching
    active_file: Option<String>,

    // Stats
    cache_hits: u64,
    cache_misses: u64,
}

impl<'a> ResolutionContext<'a> {
    pub fn new(
        symbols: &'a SymbolTable,
        import_map: &'a ImportMap,
        package_map: &'a PackageMap,
        named_import_map: &'a NamedImportMap,
        re_export_map: &'a ReExportMap,
        module_alias_map: &'a ModuleAliasMap,
    ) -> Self {
        Self {
            symbols,
            import_map,
            package_map,
            named_import_map,
            re_export_map,
            module_alias_map,
            cache: HashMap::new(),
            active_file: None,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// Enable per-file caching. Call before processing each file.
    pub fn enable_cache(&mut self, file_path: &str) {
        if self.active_file.as_deref() != Some(file_path) {
            self.cache.clear();
            self.active_file = Some(file_path.to_string());
        }
    }

    /// Clear the per-file cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.active_file = None;
    }

    /// Resolve a name from a given file using tiered resolution.
    ///
    /// Resolution order:
    /// 1. Same-file exact match
    ///    2a. Named import binding chain walk
    ///    2a. Import-scoped fuzzy match
    ///    2b. Package-scoped fuzzy match
    /// 3. Global fuzzy match
    pub fn resolve(&mut self, name: &str, from_file: &str) -> Option<TieredCandidates> {
        // Check cache
        let cache_key = (from_file.to_string(), name.to_string());
        if let Some(cached) = self.cache.get(&cache_key) {
            self.cache_hits += 1;
            return cached.clone();
        }
        self.cache_misses += 1;

        let result = self.resolve_uncached(name, from_file);

        // Store in cache
        self.cache.insert(cache_key, result.clone());
        result
    }

    fn resolve_uncached(&self, name: &str, from_file: &str) -> Option<TieredCandidates> {
        // Tier 1: Same-file exact match
        if let Some(defs) = self.symbols.lookup_in_file(from_file, name) {
            if !defs.is_empty() {
                return Some(TieredCandidates {
                    tier: ResolutionTier::SameFile,
                    candidates: defs.to_vec(),
                });
            }
        }

        // Tier 2a: Named import binding chain
        if let Some(bindings) = self.named_import_map.get(from_file) {
            if let Some(binding) = bindings.get(name) {
                let mut visited = HashSet::new();
                let candidates = self.resolve_module_export(
                    &binding.source_path,
                    &binding.exported_name,
                    &mut visited,
                );
                if !candidates.is_empty() {
                    return Some(TieredCandidates {
                        tier: ResolutionTier::NamedImport,
                        candidates,
                    });
                }
            }
        }

        // Tier 2a: Import-scoped
        if let Some(imported_files) = self.import_map.get(from_file) {
            let mut candidates = Vec::new();
            for imported_file in imported_files {
                if let Some(defs) = self.symbols.lookup_in_file(imported_file, name) {
                    for def in defs {
                        if def.is_exported {
                            candidates.push(def.clone());
                        }
                    }
                }
                let mut visited = HashSet::new();
                candidates.extend(self.resolve_module_export(imported_file, name, &mut visited));
            }
            dedupe_candidates(&mut candidates);
            if !candidates.is_empty() {
                return Some(TieredCandidates {
                    tier: ResolutionTier::ImportScoped,
                    candidates,
                });
            }
        }

        // Tier 2b: Package-scoped
        if let Some(package_dirs) = self.package_map.get(from_file) {
            let mut candidates = Vec::new();
            if let Some(global_defs) = self.symbols.lookup_global(name) {
                for def in global_defs {
                    if def.is_exported {
                        for pkg_dir in package_dirs {
                            // Use path-segment boundary matching rather than
                            // raw substring containment: `models` must not
                            // match `legacy_models/` or `submodels/`. A
                            // def's `file_path` belongs to a package when it
                            // is exactly equal to, starts with `{pkg_dir}/`,
                            // or contains `/{pkg_dir}/` as a segment.
                            let fp = def.file_path.as_str();
                            let pd = pkg_dir.as_str();
                            let matches = fp == pd
                                || fp.starts_with(&format!("{pd}/"))
                                || fp.contains(&format!("/{pd}/"));
                            if matches {
                                candidates.push(def.clone());
                                break;
                            }
                        }
                    }
                }
            }
            if !candidates.is_empty() {
                return Some(TieredCandidates {
                    tier: ResolutionTier::PackageScoped,
                    candidates,
                });
            }
        }

        // Tier 3: Global fuzzy
        if let Some(defs) = self.symbols.lookup_global(name) {
            let exported: Vec<_> = defs.iter().filter(|d| d.is_exported).cloned().collect();
            if !exported.is_empty() {
                return Some(TieredCandidates {
                    tier: ResolutionTier::Global,
                    candidates: exported,
                });
            }
        }

        None
    }

    fn resolve_module_export(
        &self,
        module_file: &str,
        name: &str,
        visited: &mut HashSet<(String, String)>,
    ) -> Vec<std::sync::Arc<crate::symbol::SymbolDefinition>> {
        if !visited.insert((module_file.to_string(), name.to_string())) {
            return Vec::new();
        }

        let mut candidates = Vec::new();
        if let Some(defs) = self.symbols.lookup_in_file(module_file, name) {
            candidates.extend(defs.iter().cloned());
        }

        if let Some(re_exports) = self.re_export_map.get(module_file) {
            for re_export in re_exports {
                match (&re_export.local_name, &re_export.exported_name) {
                    (Some(local), Some(exported)) if local == name => {
                        if re_export.source_path == module_file {
                            if let Some(binding) = self
                                .named_import_map
                                .get(module_file)
                                .and_then(|bindings| bindings.get(exported))
                            {
                                candidates.extend(self.resolve_module_export(
                                    &binding.source_path,
                                    &binding.exported_name,
                                    visited,
                                ));
                                continue;
                            }
                        }
                        candidates.extend(self.resolve_module_export(
                            &re_export.source_path,
                            exported,
                            visited,
                        ));
                    }
                    (None, None) => {
                        candidates.extend(self.resolve_module_export(
                            &re_export.source_path,
                            name,
                            visited,
                        ));
                    }
                    _ => {}
                }
            }
        }

        dedupe_candidates(&mut candidates);
        candidates
    }

    /// Resolution statistics.
    pub fn stats(&self) -> ResolutionStats {
        ResolutionStats {
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
        }
    }
}

fn dedupe_candidates(candidates: &mut Vec<std::sync::Arc<crate::symbol::SymbolDefinition>>) {
    let mut seen = HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.node_id.clone()));
}

#[derive(Debug, Clone)]
pub struct ResolutionStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::types::NodeLabel;
    use crate::symbol::{SymbolDefinition, SymbolTable};

    fn make_def(
        node_id: &str,
        file_path: &str,
        symbol_type: NodeLabel,
        exported: bool,
    ) -> SymbolDefinition {
        SymbolDefinition {
            node_id: node_id.to_string(),
            file_path: file_path.to_string(),
            symbol_type,
            parameter_count: None,
            required_parameter_count: None,
            parameter_types: None,
            return_type: None,
            declared_type: None,
            owner_id: None,
            is_exported: exported,
        }
    }

    #[test]
    fn test_same_file_resolution() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "foo".to_string(),
            make_def("Function:a:foo", "a.ts", NodeLabel::Function, true),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let named_map = NamedImportMap::new();
        let re_export_map = ReExportMap::new();
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("foo", "a.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::SameFile);
        assert_eq!(result.candidates.len(), 1);
    }

    #[test]
    fn test_named_import_resolution() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "User".to_string(),
            make_def("Class:models:User", "models.ts", NodeLabel::Class, true),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let mut named_map = NamedImportMap::new();
        named_map.entry("app.ts".to_string()).or_default().insert(
            "User".to_string(),
            NamedImportBinding {
                source_path: "models.ts".to_string(),
                exported_name: "User".to_string(),
                is_type_only: false,
            },
        );
        let alias_map = ModuleAliasMap::new();
        let re_export_map = ReExportMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("User", "app.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::NamedImport);
    }

    #[test]
    fn test_named_import_resolves_through_named_re_export() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "User".to_string(),
            make_def("Class:models:User", "models.ts", NodeLabel::Class, true),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let mut named_map = NamedImportMap::new();
        named_map.entry("app.ts".to_string()).or_default().insert(
            "User".to_string(),
            NamedImportBinding {
                source_path: "index.ts".to_string(),
                exported_name: "User".to_string(),
                is_type_only: false,
            },
        );
        let mut re_export_map = ReExportMap::new();
        re_export_map
            .entry("index.ts".to_string())
            .or_default()
            .push(ReExportBinding {
                source_path: "models.ts".to_string(),
                local_name: Some("User".to_string()),
                exported_name: Some("User".to_string()),
                is_type_only: false,
            });
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("User", "app.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::NamedImport);
        assert_eq!(result.candidates[0].file_path, "models.ts");
    }

    #[test]
    fn test_named_import_resolves_through_wildcard_re_export() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "User".to_string(),
            make_def("Class:models:User", "models.ts", NodeLabel::Class, true),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let mut named_map = NamedImportMap::new();
        named_map.entry("app.ts".to_string()).or_default().insert(
            "User".to_string(),
            NamedImportBinding {
                source_path: "index.ts".to_string(),
                exported_name: "User".to_string(),
                is_type_only: false,
            },
        );
        let mut re_export_map = ReExportMap::new();
        re_export_map
            .entry("index.ts".to_string())
            .or_default()
            .push(ReExportBinding {
                source_path: "models.ts".to_string(),
                local_name: None,
                exported_name: None,
                is_type_only: false,
            });
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("User", "app.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::NamedImport);
        assert_eq!(result.candidates[0].file_path, "models.ts");
    }

    #[test]
    fn test_named_import_resolves_default_export_alias() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "runTask".to_string(),
            make_def(
                "Function:task:runTask",
                "task.ts",
                NodeLabel::Function,
                true,
            ),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let mut named_map = NamedImportMap::new();
        named_map.entry("app.ts".to_string()).or_default().insert(
            "task".to_string(),
            NamedImportBinding {
                source_path: "task.ts".to_string(),
                exported_name: "default".to_string(),
                is_type_only: false,
            },
        );
        let mut re_export_map = ReExportMap::new();
        re_export_map
            .entry("task.ts".to_string())
            .or_default()
            .push(ReExportBinding {
                source_path: "task.ts".to_string(),
                local_name: Some("default".to_string()),
                exported_name: Some("runTask".to_string()),
                is_type_only: false,
            });
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("task", "app.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::NamedImport);
        assert_eq!(result.candidates[0].file_path, "task.ts");
        assert_eq!(result.candidates[0].node_id, "Function:task:runTask");
    }

    #[test]
    fn test_named_import_resolves_default_re_export_alias() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "runTask".to_string(),
            make_def(
                "Function:task:runTask",
                "task.ts",
                NodeLabel::Function,
                true,
            ),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let mut named_map = NamedImportMap::new();
        named_map.entry("app.ts".to_string()).or_default().insert(
            "runTask".to_string(),
            NamedImportBinding {
                source_path: "index.ts".to_string(),
                exported_name: "runTask".to_string(),
                is_type_only: false,
            },
        );
        let mut re_export_map = ReExportMap::new();
        re_export_map
            .entry("index.ts".to_string())
            .or_default()
            .push(ReExportBinding {
                source_path: "task.ts".to_string(),
                local_name: Some("runTask".to_string()),
                exported_name: Some("default".to_string()),
                is_type_only: false,
            });
        re_export_map
            .entry("task.ts".to_string())
            .or_default()
            .push(ReExportBinding {
                source_path: "task.ts".to_string(),
                local_name: Some("default".to_string()),
                exported_name: Some("runTask".to_string()),
                is_type_only: false,
            });
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("runTask", "app.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::NamedImport);
        assert_eq!(result.candidates[0].file_path, "task.ts");
        assert_eq!(result.candidates[0].node_id, "Function:task:runTask");
    }

    #[test]
    fn test_named_import_resolves_local_import_re_export() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "runTask".to_string(),
            make_def(
                "Function:task:runTask",
                "task.ts",
                NodeLabel::Function,
                true,
            ),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let mut named_map = NamedImportMap::new();
        named_map.entry("app.ts".to_string()).or_default().insert(
            "runTask".to_string(),
            NamedImportBinding {
                source_path: "index.ts".to_string(),
                exported_name: "runTask".to_string(),
                is_type_only: false,
            },
        );
        named_map.entry("index.ts".to_string()).or_default().insert(
            "runTask".to_string(),
            NamedImportBinding {
                source_path: "task.ts".to_string(),
                exported_name: "runTask".to_string(),
                is_type_only: false,
            },
        );
        let mut re_export_map = ReExportMap::new();
        re_export_map
            .entry("index.ts".to_string())
            .or_default()
            .push(ReExportBinding {
                source_path: "index.ts".to_string(),
                local_name: Some("runTask".to_string()),
                exported_name: Some("runTask".to_string()),
                is_type_only: false,
            });
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("runTask", "app.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::NamedImport);
        assert_eq!(result.candidates[0].file_path, "task.ts");
        assert_eq!(result.candidates[0].node_id, "Function:task:runTask");
    }

    #[test]
    fn test_import_scoped_resolution() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "helper".to_string(),
            make_def(
                "Function:utils:helper",
                "utils.ts",
                NodeLabel::Function,
                true,
            ),
        );

        let mut import_map = ImportMap::new();
        import_map
            .entry("main.ts".to_string())
            .or_default()
            .insert("utils.ts".to_string());

        let package_map = PackageMap::new();
        let named_map = NamedImportMap::new();
        let re_export_map = ReExportMap::new();
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("helper", "main.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::ImportScoped);
    }

    #[test]
    fn test_global_resolution() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "globalFn".to_string(),
            make_def("Function:x:globalFn", "x.ts", NodeLabel::Function, true),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let named_map = NamedImportMap::new();
        let re_export_map = ReExportMap::new();
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        let result = ctx.resolve("globalFn", "other.ts").unwrap();
        assert_eq!(result.tier, ResolutionTier::Global);
    }

    #[test]
    fn test_cache_works() {
        let mut symbols = SymbolTable::new();
        symbols.add(
            "foo".to_string(),
            make_def("f1", "a.ts", NodeLabel::Function, true),
        );

        let import_map = ImportMap::new();
        let package_map = PackageMap::new();
        let named_map = NamedImportMap::new();
        let re_export_map = ReExportMap::new();
        let alias_map = ModuleAliasMap::new();

        let mut ctx = ResolutionContext::new(
            &symbols,
            &import_map,
            &package_map,
            &named_map,
            &re_export_map,
            &alias_map,
        );

        ctx.enable_cache("a.ts");
        ctx.resolve("foo", "a.ts");
        ctx.resolve("foo", "a.ts"); // Should hit cache

        let stats = ctx.stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
    }
}
