//! DeepWiki-style documentation generator (overview, architecture, code map, getting-started, modules, index).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use colored::Colorize;
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

use super::analytics::generate_git_analytics_pages;
use super::deployment::{describe_service_fr, generate_deployment_guide};
use super::functional::{describe_project_fr, generate_functional_guide};
use super::health::generate_project_health;
use super::utils::*;

pub(super) fn generate_docs(
    graph: &KnowledgeGraph,
    repo_path: &Path,
    docs_dir: &Path,
) -> Result<()> {
    // Clean old generated files to avoid stale duplicates. Propagate the
    // remove error so callers know the directory wasn't actually cleared
    // (otherwise stale files mix with newly generated ones, producing a
    // corrupt output state with no diagnostic).
    if docs_dir.exists() {
        std::fs::remove_dir_all(docs_dir)
            .with_context(|| format!("Failed to clean docs dir {}", docs_dir.display()))?;
    }
    std::fs::create_dir_all(docs_dir)?;
    let modules_dir = docs_dir.join("modules");
    std::fs::create_dir_all(&modules_dir)?;

    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repository");

    let communities = collect_communities(graph);
    let edge_map = build_edge_map(graph);
    let lang_stats = collect_language_stats(graph);
    let file_count = count_files(graph);

    let node_count = graph.iter_nodes().count();
    let edge_count = graph.iter_relationships().count();

    if communities.is_empty() {
        println!(
            "{} No communities found. Run `code-explorer analyze` first.",
            "!".yellow()
        );
        return Ok(());
    }

    // 1. Generate overview.md
    generate_docs_overview(
        docs_dir,
        repo_name,
        file_count,
        node_count,
        edge_count,
        &lang_stats,
        &communities,
        graph,
    )?;

    // 1b. Generate functional guide (business-oriented documentation)
    generate_functional_guide(docs_dir, repo_name, graph)?;

    // 1c. Generate project health dashboard
    generate_project_health(docs_dir, graph)?;

    // 2. Generate architecture.md
    generate_docs_architecture(
        docs_dir,
        &communities,
        graph,
        &edge_map,
        file_count,
        node_count,
        edge_count,
    )?;

    // 3. Generate code-map.md
    generate_docs_code_map(docs_dir, &communities, graph, &edge_map)?;

    // 3b. Generate optional guided wiki pages from `.devin/wiki.json`
    let guided_pages = generate_wiki_steering_pages(docs_dir, repo_path, graph)?;

    // 4. Generate getting-started.md
    generate_docs_getting_started(docs_dir, repo_name, &communities, graph)?;

    // 5. Generate per-module files
    let module_page_count =
        generate_docs_modules(&modules_dir, &communities, graph, &edge_map, repo_path)?;

    // 5b. Generate deployment guide
    generate_deployment_guide(docs_dir, repo_name, graph)?;

    // 5d. Generate git analytics pages (hotspots, coupling, ownership)
    let git_analytics_count = generate_git_analytics_pages(docs_dir, repo_path)?;

    // 5e. Business-process documentation is provided by an optional domain plugin
    // (not part of the open-source build).
    let business_page_count = 0usize;

    // 5c. Generate ASP.NET MVC specific documentation (if applicable)
    let aspnet_pages = if super::super::generate_aspnet::has_aspnet_content(graph) {
        let pages = super::super::generate_aspnet::generate_aspnet_docs(graph, docs_dir)?;
        if !pages.is_empty() {
            info!("ASP.NET docs generated: {} pages", pages.len());
            println!(
                "{} Generated {} ASP.NET documentation pages",
                "OK".green(),
                pages.len()
            );
        }
        pages
    } else {
        Vec::new()
    };

    // Total page count: static pages (overview, architecture, code-map, getting-started, deployment, functional-guide, project-health) + git analytics + module pages + ASP.NET pages + business pages
    let total_pages = 7
        + guided_pages.len()
        + git_analytics_count
        + module_page_count
        + aspnet_pages.len()
        + business_page_count;
    info!("Documentation generated: {} pages total", total_pages);

    // 6. Generate _index.json LAST so it includes ASP.NET pages
    generate_docs_index(
        docs_dir,
        repo_name,
        file_count,
        node_count,
        edge_count,
        communities.len(),
        &communities,
        &aspnet_pages,
        business_page_count > 0,
        &guided_pages,
    )?;

    println!(
        "{} Generated DeepWiki docs in {}",
        "OK".green(),
        docs_dir.display()
    );
    Ok(())
}

/// Generate the _index.json navigation file.
/// `aspnet_pages` contains (id, title, filename) tuples from ASP.NET doc generation.
#[allow(clippy::too_many_arguments)]
fn generate_docs_index(
    docs_dir: &Path,
    repo_name: &str,
    file_count: usize,
    node_count: usize,
    edge_count: usize,
    module_count: usize,
    communities: &BTreeMap<String, CommunityInfo>,
    aspnet_pages: &[(String, String, String)],
    has_business: bool,
    guided_pages: &[(String, String, String)],
) -> Result<()> {
    let now = chrono::Local::now().to_rfc3339();

    // Build module children (deduplicated by filename)
    let mut module_children = Vec::new();
    let mut seen_modules = HashSet::new();
    for info in communities.values() {
        let filename = sanitize_filename(&info.label);
        if seen_modules.insert(filename.clone()) {
            let icon = if filename.starts_with("ctrl-") {
                "server"
            } else if filename.starts_with("data-") || filename.contains("model") {
                "database"
            } else if filename.contains("service") || filename.contains("repository") {
                "cog"
            } else if filename.contains("view") || filename.contains("ui") {
                "layout"
            } else if filename.contains("process") || filename.contains("flow") {
                "git-branch"
            } else if filename.contains("external") {
                "globe"
            } else if filename.contains("functional") {
                "book-open"
            } else {
                "file-text"
            };
            module_children.push(json!({
                "id": format!("mod-{}", filename),
                "title": info.label,
                "path": format!("modules/{}.md", filename),
                "icon": icon
            }));
        }
    }

    // Build Business Processes children
    let mut business_children = Vec::new();
    if has_business {
        let processes_dir = docs_dir.join("processes");
        if processes_dir.exists() {
            let mut entries: Vec<_> = std::fs::read_dir(&processes_dir)?
                .filter_map(|e| e.ok())
                .collect();
            entries.sort_by_key(|e| e.path());
            for entry in entries {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    let filename = path.file_stem().unwrap().to_string_lossy().to_string();
                    let title = match filename.as_str() {
                        "courriers" => "Système de Courriers",
                        "paiements-lifecycle" => "Cycle de Paiement",
                        "baremes-calcul" => "Calcul des Barèmes",
                        "entites-financieres" => "Entités Financières",
                        "fournisseurs" => "Gestion des Fournisseurs",
                        _ => &filename,
                    };
                    business_children.push(json!({
                        "id": format!("biz-{}", filename),
                        "title": title,
                        "path": format!("processes/{}.md", filename),
                        "icon": "workflow"
                    }));
                }
            }
        }
    }

    // Build ASP.NET children (grouped under an "ASP.NET MVC" section)
    let aspnet_icon_map: HashMap<&str, &str> = [
        ("aspnet-controllers", "server"),
        ("aspnet-routes", "route"),
        ("aspnet-entities", "table-2"),
        ("aspnet-views", "layout"),
        ("aspnet-areas", "layers"),
        ("aspnet-data-model", "database"),
        ("aspnet-seq-http", "arrow-right-left"),
        ("aspnet-seq-data", "hard-drive"),
    ]
    .into_iter()
    .collect();

    let mut pages_array = vec![
        json!({
            "id": "overview",
            "title": "Overview",
            "path": "overview.md",
            "icon": "home"
        }),
        json!({
            "id": "project-health",
            "title": "Santé du Projet",
            "path": "project-health.md",
            "icon": "activity"
        }),
        json!({
            "id": "architecture",
            "title": "Architecture",
            "path": "architecture.md",
            "icon": "git-branch"
        }),
        json!({
            "id": "code-map",
            "title": "Carte du Code",
            "path": "code-map.md",
            "icon": "map"
        }),
        json!({
            "id": "getting-started",
            "title": "Getting Started",
            "path": "getting-started.md",
            "icon": "book-open"
        }),
        json!({
            "id": "deployment",
            "title": "Environnement & Déploiement",
            "path": "deployment.md",
            "icon": "cloud"
        }),
        json!({
            "id": "git-analytics",
            "title": "Git Analytics",
            "icon": "git-commit",
            "children": [
                {
                    "id": "hotspots",
                    "title": "Code Hotspots",
                    "path": "hotspots.md",
                    "icon": "flame"
                },
                {
                    "id": "coupling",
                    "title": "Temporal Coupling",
                    "path": "coupling.md",
                    "icon": "link"
                },
                {
                    "id": "ownership",
                    "title": "Code Ownership",
                    "path": "ownership.md",
                    "icon": "users"
                }
            ]
        }),
        json!({
            "id": "modules",
            "title": "Modules",
            "icon": "layers",
            "children": module_children
        }),
    ];

    if !guided_pages.is_empty() {
        let guided_children: Vec<serde_json::Value> = guided_pages
            .iter()
            .map(|(id, title, filename)| {
                json!({
                    "id": id,
                    "title": title,
                    "path": filename,
                    "icon": if id == "wiki-steering" { "compass" } else { "file-text" }
                })
            })
            .collect();

        pages_array.insert(
            4,
            json!({
                "id": "wiki-guided",
                "title": "Wiki guidé",
                "icon": "compass",
                "children": guided_children
            }),
        );
    }

    if !business_children.is_empty() {
        pages_array.push(json!({
            "id": "business",
            "title": "Processus Métier",
            "icon": "workflow",
            "children": business_children
        }));
    }

    // Add ASP.NET section if pages exist
    if !aspnet_pages.is_empty() {
        let aspnet_children: Vec<serde_json::Value> = aspnet_pages
            .iter()
            .map(|(id, title, filename)| {
                let icon = aspnet_icon_map.get(id.as_str()).unwrap_or(&"file-text");
                json!({
                    "id": id,
                    "title": title,
                    "path": filename,
                    "icon": icon
                })
            })
            .collect();

        pages_array.push(json!({
            "id": "aspnet",
            "title": "ASP.NET MVC 5 / EF6",
            "icon": "server",
            "children": aspnet_children
        }));
    }

    if pages_array.is_empty() {
        warn!("No documentation pages found in _index.json");
    }

    let index = json!({
        "title": repo_name,
        "generatedAt": now,
        "stats": {
            "files": file_count,
            "nodes": node_count,
            "edges": edge_count,
            "modules": module_count
        },
        "pages": pages_array
    });

    let index_path = docs_dir.join("_index.json");
    let mut f = std::fs::File::create(&index_path)?;
    writeln!(f, "{}", index)?;
    println!("  {} _index.json", "OK".green());
    Ok(())
}

/// Generate overview.md with DeepWiki-quality content.
#[allow(clippy::too_many_arguments)]
fn generate_docs_overview(
    docs_dir: &Path,
    repo_name: &str,
    file_count: usize,
    node_count: usize,
    edge_count: usize,
    lang_stats: &BTreeMap<String, usize>,
    communities: &BTreeMap<String, CommunityInfo>,
    graph: &KnowledgeGraph,
) -> Result<()> {
    let out_path = docs_dir.join("overview.md");
    let mut f = std::fs::File::create(&out_path)?;

    let label_counts = count_nodes_by_label(graph);
    let controller_count = label_counts
        .get(&NodeLabel::Controller)
        .copied()
        .unwrap_or(0);
    let view_count = label_counts.get(&NodeLabel::View).copied().unwrap_or(0);
    let entity_count = label_counts.get(&NodeLabel::DbEntity).copied().unwrap_or(0);
    let service_count = label_counts.get(&NodeLabel::Service).copied().unwrap_or(0)
        + label_counts
            .get(&NodeLabel::Repository)
            .copied()
            .unwrap_or(0);
    let ui_count = label_counts
        .get(&NodeLabel::UiComponent)
        .copied()
        .unwrap_or(0);
    let merged_module_count = merged_communities_by_filename(communities).len();
    let dead_count = graph
        .iter_nodes()
        .filter(|n| n.properties.is_dead_candidate.unwrap_or(false))
        .count();

    // Title
    writeln!(f, "# {}", repo_name)?;
    writeln!(f)?;

    // Relevant source files
    let top_files = top_connected_files(graph, 10);
    let top_files_refs: Vec<&str> = top_files.iter().map(|s| s.as_str()).collect();
    write!(f, "{}", source_files_section(&top_files_refs))?;

    // Business description — specific to the project type
    let (_languages, _frameworks, _ui_libs, _auto_desc) =
        detect_technology_stack(graph, lang_stats);
    let has_aspnet = label_counts
        .get(&NodeLabel::Controller)
        .copied()
        .unwrap_or(0)
        > 0;
    let has_ef = label_counts
        .get(&NodeLabel::DbContext)
        .copied()
        .unwrap_or(0)
        > 0;
    let has_telerik = label_counts
        .get(&NodeLabel::UiComponent)
        .copied()
        .unwrap_or(0)
        > 0;

    if has_aspnet && has_ef {
        writeln!(f, "> **{}** est une application de gestion métier construite en ASP.NET MVC 5 avec Entity Framework 6.", repo_name)?;
        if has_telerik {
            writeln!(f, "> L'interface utilise des grilles Telerik pour l'affichage et la saisie des données.")?;
        }
        let ext_count = label_counts
            .get(&NodeLabel::ExternalService)
            .copied()
            .unwrap_or(0);
        if ext_count > 0 {
            writeln!(
                f,
                "> Le système s'intègre avec {} services externes (WebAPI, WCF, LDAP).",
                ext_count
            )?;
        }
    } else {
        writeln!(f, "> {}", french_codebase_description(lang_stats))?;
    }
    // Tracing / dead-code extra sentence (conditionnel)
    {
        let traced_count = graph
            .iter_nodes()
            .filter(|n| n.properties.is_traced.unwrap_or(false))
            .count();
        if traced_count > 0 || dead_count > 0 {
            let mut extra = String::new();
            if traced_count > 0 {
                extra.push_str(&format!(
                    "{} méthodes instrumentées avec StackLogger.",
                    traced_count
                ));
            }
            if dead_count > 0 {
                if !extra.is_empty() {
                    extra.push(' ');
                }
                extra.push_str(&format!(
                    "{} méthode{} candidate{} au code mort.",
                    dead_count,
                    if dead_count > 1 { "s" } else { "" },
                    if dead_count > 1 { "s" } else { "" }
                ));
            }
            writeln!(f, "> {}", extra)?;
        }
    }
    writeln!(f)?;

    writeln!(f, "## Vue d'ensemble rapide")?;
    writeln!(f)?;
    writeln!(f, "| Signal | Valeur |")?;
    writeln!(f, "|--------|--------|")?;
    writeln!(f, "| Fichiers source | {} |", file_count)?;
    writeln!(f, "| Symboles indexés | {} |", node_count)?;
    writeln!(f, "| Relations du graphe | {} |", edge_count)?;
    writeln!(f, "| Modules documentés | {} |", merged_module_count)?;
    if dead_count > 0 {
        writeln!(f, "| Méthodes candidates au code mort | {} |", dead_count)?;
    }
    if controller_count > 0 {
        writeln!(f, "| Controllers | {} |", controller_count)?;
    }
    if view_count > 0 {
        writeln!(f, "| Vues | {} |", view_count)?;
    }
    if entity_count > 0 {
        writeln!(f, "| Entités | {} |", entity_count)?;
    }
    if service_count > 0 {
        writeln!(f, "| Services et repositories | {} |", service_count)?;
    }
    writeln!(f)?;

    // Technology Stack as a proper table
    let (languages, frameworks, ui_libs, _desc) = detect_technology_stack(graph, lang_stats);
    let languages_fr = format_language_stats_fr(lang_stats);
    writeln!(f, "## Stack Technique")?;
    writeln!(f)?;
    writeln!(f, "| Catégorie | Technologie |")?;
    writeln!(f, "|-----------|-------------|")?;
    if !languages.is_empty() {
        writeln!(f, "| **Langages** | {} |", languages_fr.join(", "))?;
    }
    if !frameworks.is_empty() {
        writeln!(f, "| **Frameworks** | {} |", frameworks.join(", "))?;
    }
    if !ui_libs.is_empty() {
        writeln!(f, "| **UI Components** | {} |", ui_libs.join(", "))?;
    }
    let ctx_count = label_counts
        .get(&NodeLabel::DbContext)
        .copied()
        .unwrap_or(0);
    if ctx_count > 0 {
        writeln!(
            f,
            "| **ORM** | Entity Framework 6 ({} DbContexts) |",
            ctx_count
        )?;
    }
    let ext_count = label_counts
        .get(&NodeLabel::ExternalService)
        .copied()
        .unwrap_or(0);
    if ext_count > 0 {
        writeln!(
            f,
            "| **Integrations** | {} external services (WebAPI, WCF) |",
            ext_count
        )?;
    }
    writeln!(f)?;

    // Key Subsystems
    let subsystem_rows = overview_subsystem_rows(communities, graph);
    let reading_guide = module_reading_guide(communities, graph);
    if !subsystem_rows.is_empty() {
        writeln!(f, "## Sous-systèmes Clés")?;
        writeln!(f)?;
        writeln!(
            f,
            "| Module | Symboles | Points d'entrée | Couplage | Description |"
        )?;
        writeln!(
            f,
            "|--------|---------|--------------|----------|-------------|"
        )?;
        for row in subsystem_rows.iter().take(20) {
            writeln!(
                f,
                "| [{}](modules/{}.md) | {} | {} | {} | {} |",
                markdown_cell(&row.label),
                row.filename,
                row.member_count,
                row.entry_point_count,
                row.coupling_count,
                markdown_cell(&row.description)
            )?;
        }
        if subsystem_rows.len() > 20 {
            writeln!(f)?;
            writeln!(
                f,
                "_Affichage des 20 sous-systèmes les plus connectés. Voir la [Carte du Code](code-map.md) pour l'index complet et le graphe des dépendances._"
            )?;
        }
        writeln!(f)?;
        writeln!(f, "## Parcours de Lecture Recommandé")?;
        writeln!(f)?;
        writeln!(
            f,
            "1. Commencez par la [Carte du Code](code-map.md) pour voir les frontières de modules et les dépendances inter-modules."
        )?;
        if !reading_guide.is_empty() {
            for (idx, step) in reading_guide.iter().take(4).enumerate() {
                writeln!(
                    f,
                    "{}. Ouvrez [{}](modules/{}.md) : {}.",
                    idx + 2,
                    step.label,
                    step.filename,
                    step.reason
                )?;
            }
            writeln!(
                f,
                "{}. Utilisez [Vue Processus](process-overview.md) pour suivre un parcours métier de bout en bout.",
                reading_guide.iter().take(4).count() + 2
            )?;
            writeln!(
                f,
                "{}. Terminez avec [Santé du Projet](project-health.md) pour vérifier les traces, le code potentiellement mort et les risques d'intégration.",
                reading_guide.iter().take(4).count() + 3
            )?;
        } else if let Some(top_row) = subsystem_rows.first() {
            writeln!(
                f,
                "2. Ouvrez [{}](modules/{}.md), le sous-système le plus structurant par points d'entrée et couplage.",
                top_row.label, top_row.filename
            )?;
            writeln!(
                f,
                "3. Utilisez [Vue Processus](process-overview.md) pour suivre un parcours métier de bout en bout."
            )?;
            writeln!(
                f,
                "4. Terminez avec [Santé du Projet](project-health.md) pour vérifier les traces, le code potentiellement mort et les risques d'intégration."
            )?;
        } else {
            writeln!(
                f,
                "2. Ouvrez le sous-système le plus structurant depuis l'index des modules."
            )?;
            writeln!(
                f,
                "3. Utilisez [Vue Processus](process-overview.md) pour suivre un parcours métier de bout en bout."
            )?;
            writeln!(
                f,
                "4. Terminez avec [Santé du Projet](project-health.md) pour vérifier les traces, le code potentiellement mort et les risques d'intégration."
            )?;
        }
        writeln!(f)?;
    }

    // ── Signaux d'Alerte ────────────────────────────────────────────────
    {
        let density = if node_count > 0 {
            edge_count as f64 / node_count as f64
        } else {
            0.0
        };

        let total_files = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::File)
            .count();
        // Restrict the numerator to File nodes — `is_traced` is also set on
        // Method nodes by `extract_tracing_info`, so without this filter the
        // count would mix methods + files and the displayed percentage could
        // exceed 100%, silently suppressing the "low tracing coverage" alert
        // below (which only fires when `traced_pct < 10.0`). The sibling
        // `project-health.md` generator applies the same filter for the same
        // reason — keep the two in sync.
        let traced_files = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::File && n.properties.is_traced == Some(true))
            .count();
        let traced_pct = if total_files > 0 {
            (traced_files as f64 / total_files as f64) * 100.0
        } else {
            0.0
        };
        let ext_svc_count = label_counts
            .get(&NodeLabel::ExternalService)
            .copied()
            .unwrap_or(0);

        let mut has_alerts = false;

        if dead_count > 0 {
            let total_m = graph
                .iter_nodes()
                .filter(|n| matches!(n.label, NodeLabel::Method | NodeLabel::Function))
                .count();
            let dead_pct = if total_m > 0 {
                (dead_count as f64 / total_m as f64) * 100.0
            } else {
                0.0
            };
            if !has_alerts {
                writeln!(f, "## Signaux d'Alerte")?;
                writeln!(f)?;
                has_alerts = true;
            }
            if dead_pct > 20.0 {
                writeln!(f, "> [!DANGER]")?;
            } else if dead_pct > 10.0 {
                writeln!(f, "> [!WARNING]")?;
            } else {
                writeln!(f, "> [!NOTE]")?;
            }
            writeln!(
                f,
                "> **{} méthodes** ({:.1}%) détectées comme code mort potentiel (aucun appelant).",
                dead_count, dead_pct
            )?;
            writeln!(
                f,
                "> Voir la page [Santé du Projet](project-health.md) pour le détail."
            )?;
            writeln!(f)?;
        }

        if traced_pct < 10.0 && total_files > 0 {
            if !has_alerts {
                writeln!(f, "## Signaux d'Alerte")?;
                writeln!(f)?;
                has_alerts = true;
            }
            writeln!(f, "> [!WARNING]")?;
            writeln!(
                f,
                "> Seulement {:.0}% des fichiers ont une traçabilité StackLogger.",
                traced_pct
            )?;
            writeln!(
                f,
                "> Les modules non tracés seront difficiles à déboguer en production."
            )?;
            writeln!(f)?;
        }

        if density > 3.0 {
            if !has_alerts {
                writeln!(f, "## Signaux d'Alerte")?;
                writeln!(f)?;
                has_alerts = true;
            }
            writeln!(f, "> [!DANGER]")?;
            writeln!(
                f,
                "> Densité de couplage élevée ({:.1}). Le système est fortement interconnecté.",
                density
            )?;
            writeln!(
                f,
                "> Tout changement peut avoir des effets de bord importants."
            )?;
            writeln!(f)?;
        }

        if ext_svc_count > 5 {
            if !has_alerts {
                writeln!(f, "## Signaux d'Alerte")?;
                writeln!(f)?;
                #[allow(unused_assignments)]
                {
                    has_alerts = true;
                }
            }
            writeln!(f, "> [!NOTE]")?;
            writeln!(
                f,
                "> {} services externes détectés. Chaque intégration est un point de",
                ext_svc_count
            )?;
            writeln!(
                f,
                "> fragilité potentiel (timeout, indisponibilité, changement d'API)."
            )?;
            writeln!(f)?;
        }
    }

    // Summary
    let data_pages = label_counts
        .get(&NodeLabel::DbContext)
        .copied()
        .unwrap_or(0);
    let mut sections = vec![
        "Vue d'ensemble",
        "Santé du Projet",
        "Architecture",
        "Carte du Code",
        "Prise en Main",
        "Déploiement",
        "Modules",
    ];
    if controller_count > 0 {
        sections.push("Controllers");
    }
    if data_pages > 0 {
        sections.push("Modèle de données");
    }
    if service_count > 0 {
        sections.push("Services");
    }
    if ui_count > 0 {
        sections.push("Composants UI");
    }
    if label_counts.get(&NodeLabel::AjaxCall).copied().unwrap_or(0) > 0 {
        sections.push("Appels AJAX");
    }

    writeln!(f, "## Résumé")?;
    writeln!(f)?;
    writeln!(
        f,
        "Ce wiki couvre les pages d'orientation principales et **{} modules** générés à partir du graphe.",
        merged_module_count
    )?;
    writeln!(f, "Sections principales : {}.", sections.join(", "))?;
    writeln!(f)?;

    writeln!(
        f,
        "**Voir aussi :** [Architecture](./architecture.md) · [Carte du Code](./code-map.md) · [Prise en Main](./getting-started.md)"
    )?;
    writeln!(f)?;
    writeln!(f, "---")?;
    writeln!(f, "[Suivant : Architecture ->](./architecture.md)")?;

    println!("  {} overview.md", "OK".green());
    Ok(())
}

/// Generate architecture.md with real Mermaid diagram built from graph data.
fn generate_docs_architecture(
    docs_dir: &Path,
    communities: &BTreeMap<String, CommunityInfo>,
    graph: &KnowledgeGraph,
    edge_map: &HashMap<String, Vec<(String, RelationshipType)>>,
    _file_count: usize,
    node_count: usize,
    edge_count: usize,
) -> Result<()> {
    let out_path = docs_dir.join("architecture.md");
    let mut f = std::fs::File::create(&out_path)?;

    let label_counts = count_nodes_by_label(graph);
    let ctrl_count = label_counts
        .get(&NodeLabel::Controller)
        .copied()
        .unwrap_or(0);
    let view_count = label_counts.get(&NodeLabel::View).copied().unwrap_or(0);
    let svc_count = label_counts.get(&NodeLabel::Service).copied().unwrap_or(0)
        + label_counts
            .get(&NodeLabel::Repository)
            .copied()
            .unwrap_or(0);
    let ctx_count = label_counts
        .get(&NodeLabel::DbContext)
        .copied()
        .unwrap_or(0);
    let entity_count = label_counts.get(&NodeLabel::DbEntity).copied().unwrap_or(0);
    let ext_count = label_counts
        .get(&NodeLabel::ExternalService)
        .copied()
        .unwrap_or(0);
    let action_count = label_counts
        .get(&NodeLabel::ControllerAction)
        .copied()
        .unwrap_or(0);
    let ui_count = label_counts
        .get(&NodeLabel::UiComponent)
        .copied()
        .unwrap_or(0);
    let edmx_count: usize = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::File && n.properties.file_path.ends_with(".edmx"))
        .count();

    let arch_files: Vec<String> = graph
        .iter_nodes()
        .filter(|n| {
            matches!(
                n.label,
                NodeLabel::Controller
                    | NodeLabel::Service
                    | NodeLabel::DbContext
                    | NodeLabel::Repository
            )
        })
        .map(|n| n.properties.file_path.clone())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    let arch_file_refs: Vec<&str> = arch_files.iter().take(15).map(|s| s.as_str()).collect();
    let merged_communities = merged_communities_by_filename(communities);
    let member_to_module = build_member_to_module(&merged_communities);
    let dependency_edges = module_dependency_edges(graph, &member_to_module);
    let module_stats = module_map_stats(&merged_communities, graph, &dependency_edges);

    writeln!(f, "# Architecture")?;
    writeln!(f)?;
    write!(f, "{}", source_files_section(&arch_file_refs))?;

    let has_tiered = ctrl_count > 0 && (svc_count > 0 || ctx_count > 0);

    if has_tiered {
        writeln!(f, "Ce projet suit une **architecture en 3 couches** :")?;
        writeln!(
            f,
            "Présentation (controllers + vues) -> logique métier (services) -> accès aux données (Entity Framework)."
        )?;
    } else {
        writeln!(
            f,
            "Architecture système avec **{} modules**, **{} symboles** et **{} relations** dans le graphe.",
            merged_communities.len(),
            node_count,
            edge_count
        )?;
    }
    writeln!(f)?;

    writeln!(f, "## Diagramme d'Architecture")?;
    writeln!(f)?;
    writeln!(f, "```mermaid")?;
    writeln!(f, "graph TD")?;

    if has_tiered {
        writeln!(f, "    subgraph Presentation")?;
        writeln!(f, "        C[\"Controllers ({})\"]", ctrl_count)?;
        if view_count > 0 {
            writeln!(f, "        V[\"Views ({})\"]", view_count)?;
        }
        writeln!(f, "    end")?;

        if svc_count > 0 {
            writeln!(f, "    subgraph Business[\"Business Logic\"]")?;
            writeln!(f, "        S[\"Services ({})\"]", svc_count)?;
            writeln!(f, "    end")?;
        }

        if ctx_count > 0 || entity_count > 0 {
            writeln!(f, "    subgraph Data[\"Data Access\"]")?;
            if ctx_count > 0 {
                writeln!(f, "        DB[\"DbContexts ({})\"]", ctx_count)?;
            }
            if entity_count > 0 {
                writeln!(f, "        E[\"Entities ({})\"]", entity_count)?;
            }
            writeln!(f, "    end")?;
        }

        if ext_count > 0 {
            writeln!(f, "    subgraph External")?;
            writeln!(f, "        EXT[\"External Services ({})\"]", ext_count)?;
            writeln!(f, "    end")?;
        }

        let has_ctrl_to_svc = svc_count > 0
            && graph.iter_relationships().any(|r| {
                matches!(
                    r.rel_type,
                    RelationshipType::DependsOn | RelationshipType::Calls
                ) && graph
                    .get_node(&r.source_id)
                    .map(|n| n.label == NodeLabel::Controller)
                    .unwrap_or(false)
                    && graph
                        .get_node(&r.target_id)
                        .map(|n| matches!(n.label, NodeLabel::Service | NodeLabel::Repository))
                        .unwrap_or(false)
            });
        let has_svc_to_db = ctx_count > 0
            && graph.iter_relationships().any(|r| {
                matches!(
                    r.rel_type,
                    RelationshipType::DependsOn | RelationshipType::Calls | RelationshipType::Uses
                ) && graph
                    .get_node(&r.source_id)
                    .map(|n| matches!(n.label, NodeLabel::Service | NodeLabel::Repository))
                    .unwrap_or(false)
                    && graph
                        .get_node(&r.target_id)
                        .map(|n| n.label == NodeLabel::DbContext)
                        .unwrap_or(false)
            });
        let has_db_to_entity = entity_count > 0
            && graph
                .iter_relationships()
                .any(|r| r.rel_type == RelationshipType::MapsToEntity);
        let has_ctrl_to_view = view_count > 0
            && graph
                .iter_relationships()
                .any(|r| r.rel_type == RelationshipType::RendersView);
        let has_svc_to_ext = ext_count > 0
            && graph
                .iter_relationships()
                .any(|r| r.rel_type == RelationshipType::CallsService);

        if has_ctrl_to_svc || svc_count > 0 {
            writeln!(f, "    C --> S")?;
        }
        if has_svc_to_db || (ctx_count > 0 && svc_count > 0) {
            writeln!(f, "    S --> DB")?;
        }
        if has_db_to_entity || (entity_count > 0 && ctx_count > 0) {
            writeln!(f, "    DB --> E")?;
        }
        if has_ctrl_to_view || view_count > 0 {
            writeln!(f, "    C --> V")?;
        }
        if has_svc_to_ext || (ext_count > 0 && svc_count > 0) {
            writeln!(f, "    S --> EXT")?;
        }
    } else {
        for stats in module_stats.iter().take(20) {
            let safe_id = format!("M_{}", sanitize_mermaid_id(&stats.filename));
            let label = format!("{} ({} symboles)", stats.label, stats.member_count);
            writeln!(f, "    {}[\"{}\"]", safe_id, escape_mermaid_label(&label))?;
        }

        let visible_modules: HashSet<&str> = module_stats
            .iter()
            .take(20)
            .map(|stats| stats.filename.as_str())
            .collect();
        for edge in dependency_edges
            .iter()
            .filter(|edge| {
                visible_modules.contains(edge.source.as_str())
                    && visible_modules.contains(edge.target.as_str())
            })
            .take(30)
        {
            let src_id = format!("M_{}", sanitize_mermaid_id(&edge.source));
            let tgt_id = format!("M_{}", sanitize_mermaid_id(&edge.target));
            writeln!(f, "    {} -->|{}| {}", src_id, edge.count, tgt_id)?;
        }
    }
    writeln!(f, "```")?;
    writeln!(f)?;

    // ── Cross-layer violations ──
    writeln!(f, "## Violations de couche")?;
    writeln!(f)?;
    {
        let violations: Vec<(&GraphNode, &GraphNode)> = graph
            .iter_relationships()
            .filter(|r| {
                matches!(
                    r.rel_type,
                    RelationshipType::DependsOn | RelationshipType::Calls
                )
            })
            .filter_map(|r| {
                let src = graph.get_node(&r.source_id)?;
                let tgt = graph.get_node(&r.target_id)?;
                if matches!(src.label, NodeLabel::Controller)
                    && matches!(
                        tgt.label,
                        NodeLabel::DbContext | NodeLabel::DbEntity | NodeLabel::Repository
                    )
                {
                    Some((src, tgt))
                } else {
                    None
                }
            })
            .collect();
        if violations.is_empty() {
            writeln!(f, "> Aucune violation de couche détectée.")?;
        } else {
            writeln!(
                f,
                "> **{} violation(s)** — Controllers accédant directement à la couche données.",
                violations.len()
            )?;
            writeln!(f)?;
            writeln!(f, "```mermaid")?;
            writeln!(f, "graph LR")?;
            let mut shown: HashSet<String> = HashSet::new();
            for (src, tgt) in violations.iter().take(20) {
                let sid = sanitize_mermaid_id(&src.properties.name);
                let tid = sanitize_mermaid_id(&tgt.properties.name);
                if shown.insert(format!("{}->{}", sid, tid)) {
                    writeln!(
                        f,
                        "    {}[\"{}\"] -->|bypass| {}[\"{}\"]",
                        sid, src.properties.name, tid, tgt.properties.name
                    )?;
                }
            }
            writeln!(f, "```")?;
            writeln!(f)?;
            writeln!(f, "| Controller | Accès direct à | Type |")?;
            writeln!(f, "|-----------|----------------|------|")?;
            for (src, tgt) in violations.iter().take(30) {
                writeln!(
                    f,
                    "| `{}` | `{}` | {} |",
                    src.properties.name,
                    tgt.properties.name,
                    tgt.label.as_str()
                )?;
            }
        }
        writeln!(f)?;
    }

    writeln!(f, "## Lecture Architecturale")?;
    writeln!(f)?;

    if ctrl_count > 0 {
        writeln!(f, "### Couche présentation")?;
        writeln!(
            f,
            "{} controllers exposent {} actions et servent {} vues.",
            ctrl_count, action_count, view_count
        )?;
        if ui_count > 0 {
            writeln!(f, "{} composants Telerik/Kendo UI détectés.", ui_count)?;
        }
        writeln!(f)?;
    }

    if svc_count > 0 {
        writeln!(f, "### Couche métier")?;
        writeln!(
            f,
            "{} services portent les règles métier et les traitements.",
            svc_count
        )?;
        writeln!(f)?;
    }

    if ctx_count > 0 || entity_count > 0 {
        writeln!(f, "### Couche accès aux données")?;
        writeln!(
            f,
            "{} DbContext Entity Framework gèrent {} entités",
            ctx_count, entity_count
        )?;
        if edmx_count > 0 {
            writeln!(f, "sur {} modèles EDMX.", edmx_count)?;
        } else {
            writeln!(f, ".")?;
        }
        writeln!(f)?;
    }

    if ext_count > 0 {
        writeln!(f, "### Intégrations externes")?;
        writeln!(
            f,
            "{} connexions à des services externes détectées (WebAPI, WCF, LDAP).",
            ext_count
        )?;
        writeln!(f)?;

        let ext_services: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::ExternalService)
            .collect();
        if !ext_services.is_empty() {
            for svc in ext_services.iter().take(15) {
                let stype = svc.properties.service_type.as_deref().unwrap_or("REST");
                writeln!(f, "- **{}** ({})", svc.properties.name, stype)?;
            }
            writeln!(f)?;
        }
    }

    if !has_tiered && !module_stats.is_empty() {
        writeln!(f, "### Topologie des Modules")?;
        writeln!(f)?;
        writeln!(
            f,
            "Modules les plus structurants par couplage et taille. Utilisez cette table pour choisir où commencer avant de passer à la [Carte du Code](./code-map.md)."
        )?;
        writeln!(f)?;
        writeln!(
            f,
            "| Module | Symboles | Fichiers source | Entrant | Sortant |"
        )?;
        writeln!(
            f,
            "|--------|---------|--------------|----------|----------|"
        )?;
        for stats in module_stats.iter().take(12) {
            writeln!(
                f,
                "| [{}](modules/{}.md) | {} | {} | {} | {} |",
                markdown_cell(&stats.label),
                stats.filename,
                stats.member_count,
                stats.source_file_count,
                stats.incoming_calls,
                stats.outgoing_calls
            )?;
        }
        writeln!(f)?;
    }

    writeln!(f, "## Résumé")?;
    writeln!(f)?;
    if has_tiered {
        writeln!(
            f,
            "L'application suit une architecture en couches avec séparation entre présentation, logique métier et accès aux données."
        )?;
    } else {
        writeln!(
            f,
            "La base de code est organisée en **{} modules interconnectés**.",
            merged_communities.len()
        )?;
    }
    writeln!(f)?;
    writeln!(
        f,
        "**Voir aussi :** [Vue d'ensemble](./overview.md) · [Carte du Code](./code-map.md) · [Prise en Main](./getting-started.md)"
    )?;
    writeln!(f)?;
    writeln!(f, "---")?;
    // Mirror the static page order built in `generate_docs_modules`
    // (overview -> project-health -> architecture -> code-map -> getting-started). The
    // previous footer hard-coded `Previous: Overview`, skipping the
    // project-health page that sits between them and breaking the back-link
    // chain when the user clicked Previous from architecture.
    writeln!(
        f,
        "[<- Précédent : Santé du Projet](./project-health.md) | [Suivant : Carte du Code ->](./code-map.md)"
    )?;

    let _ = edge_map;

    println!("  {} architecture.md", "OK".green());
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleDependencyEdge {
    source: String,
    target: String,
    count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModuleNeighbor {
    filename: String,
    label: String,
    count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleDependencyDirection {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone)]
struct ModuleMapStats {
    filename: String,
    label: String,
    source_files: Vec<String>,
    source_file_count: usize,
    member_count: usize,
    entry_point_count: usize,
    incoming_calls: usize,
    outgoing_calls: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OverviewSubsystemRow {
    filename: String,
    label: String,
    member_count: usize,
    entry_point_count: usize,
    coupling_count: usize,
    description: String,
}

#[derive(Debug, Clone)]
struct ReadingGuideStep {
    filename: String,
    label: String,
    score: f64,
    reason: String,
    question: String,
}

fn merged_communities_by_filename(
    communities: &BTreeMap<String, CommunityInfo>,
) -> BTreeMap<String, CommunityInfo> {
    let mut merged: BTreeMap<String, CommunityInfo> = BTreeMap::new();
    for info in communities.values() {
        let filename = sanitize_filename(&info.label);
        let entry = merged.entry(filename).or_insert_with(|| CommunityInfo {
            label: info.label.clone(),
            description: info.description.clone(),
            member_ids: Vec::new(),
            keywords: Vec::new(),
        });

        if entry.description.is_none() {
            entry.description = info.description.clone();
        }
        for mid in &info.member_ids {
            if !entry.member_ids.contains(mid) {
                entry.member_ids.push(mid.clone());
            }
        }
        for kw in &info.keywords {
            if !entry.keywords.contains(kw) {
                entry.keywords.push(kw.clone());
            }
        }
    }
    merged
}

fn build_member_to_module(
    communities: &BTreeMap<String, CommunityInfo>,
) -> HashMap<String, String> {
    let mut member_to_module = HashMap::new();
    for (filename, info) in communities {
        for member_id in &info.member_ids {
            member_to_module.insert(member_id.clone(), filename.clone());
        }
    }
    member_to_module
}

fn module_dependency_edges(
    graph: &KnowledgeGraph,
    member_to_module: &HashMap<String, String>,
) -> Vec<ModuleDependencyEdge> {
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for rel in graph.iter_relationships() {
        if !matches!(
            rel.rel_type,
            RelationshipType::Calls
                | RelationshipType::CallsAction
                | RelationshipType::CallsService
                | RelationshipType::DependsOn
                | RelationshipType::Uses
        ) {
            continue;
        }

        let Some(source) = member_to_module.get(&rel.source_id) else {
            continue;
        };
        let Some(target) = member_to_module.get(&rel.target_id) else {
            continue;
        };
        if source == target {
            continue;
        }
        *counts.entry((source.clone(), target.clone())).or_insert(0) += 1;
    }

    let mut edges: Vec<ModuleDependencyEdge> = counts
        .into_iter()
        .map(|((source, target), count)| ModuleDependencyEdge {
            source,
            target,
            count,
        })
        .collect();
    edges.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.target.cmp(&b.target))
    });
    edges
}

fn module_dependency_neighbors(
    filename: &str,
    communities: &BTreeMap<String, CommunityInfo>,
    edges: &[ModuleDependencyEdge],
    direction: ModuleDependencyDirection,
) -> Vec<ModuleNeighbor> {
    let mut neighbors = Vec::new();
    for edge in edges {
        let other = match direction {
            ModuleDependencyDirection::Incoming if edge.target == filename => &edge.source,
            ModuleDependencyDirection::Outgoing if edge.source == filename => &edge.target,
            _ => continue,
        };
        let label = communities
            .get(other)
            .map(|info| info.label.clone())
            .unwrap_or_else(|| other.clone());
        neighbors.push(ModuleNeighbor {
            filename: other.clone(),
            label,
            count: edge.count,
        });
    }

    neighbors.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.filename.cmp(&b.filename))
    });
    neighbors
}

fn module_map_stats(
    communities: &BTreeMap<String, CommunityInfo>,
    graph: &KnowledgeGraph,
    edges: &[ModuleDependencyEdge],
) -> Vec<ModuleMapStats> {
    let mut stats = Vec::new();
    for (filename, info) in communities {
        let mut source_files = info
            .member_ids
            .iter()
            .filter_map(|member_id| graph.get_node(member_id))
            .map(|node| node.properties.file_path.clone())
            .filter(|path| !path.is_empty())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect::<Vec<String>>();
        let source_file_count = source_files.len();
        source_files.truncate(8);

        let entry_point_count = info
            .member_ids
            .iter()
            .filter_map(|member_id| graph.get_node(member_id))
            .filter(|node| {
                node.properties
                    .entry_point_score
                    .map(|score| score > 0.3)
                    .unwrap_or(false)
            })
            .count();

        let incoming_calls = edges
            .iter()
            .filter(|edge| edge.target == *filename)
            .map(|edge| edge.count)
            .sum();
        let outgoing_calls = edges
            .iter()
            .filter(|edge| edge.source == *filename)
            .map(|edge| edge.count)
            .sum();

        stats.push(ModuleMapStats {
            filename: filename.clone(),
            label: info.label.clone(),
            source_files,
            source_file_count,
            member_count: info.member_ids.len(),
            entry_point_count,
            incoming_calls,
            outgoing_calls,
        });
    }

    stats.sort_by(|a, b| {
        b.entry_point_count
            .cmp(&a.entry_point_count)
            .then_with(|| b.outgoing_calls.cmp(&a.outgoing_calls))
            .then_with(|| b.incoming_calls.cmp(&a.incoming_calls))
            .then_with(|| b.member_count.cmp(&a.member_count))
            .then_with(|| a.label.cmp(&b.label))
    });
    stats
}

fn overview_subsystem_rows(
    communities: &BTreeMap<String, CommunityInfo>,
    graph: &KnowledgeGraph,
) -> Vec<OverviewSubsystemRow> {
    let merged = merged_communities_by_filename(communities);
    let member_to_module = build_member_to_module(&merged);
    let dependency_edges = module_dependency_edges(graph, &member_to_module);
    let mut rows = module_map_stats(&merged, graph, &dependency_edges)
        .into_iter()
        .filter_map(|stats| {
            let info = merged.get(&stats.filename)?;
            let coupling_count = stats.incoming_calls + stats.outgoing_calls;
            let description = overview_subsystem_description(info, &stats);
            Some(OverviewSubsystemRow {
                filename: stats.filename,
                label: stats.label,
                member_count: stats.member_count,
                entry_point_count: stats.entry_point_count,
                coupling_count,
                description,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b.entry_point_count
            .cmp(&a.entry_point_count)
            .then_with(|| b.coupling_count.cmp(&a.coupling_count))
            .then_with(|| b.member_count.cmp(&a.member_count))
            .then_with(|| a.label.cmp(&b.label))
    });
    rows
}

fn module_reading_guide(
    communities: &BTreeMap<String, CommunityInfo>,
    graph: &KnowledgeGraph,
) -> Vec<ReadingGuideStep> {
    let merged = merged_communities_by_filename(communities);
    let member_to_module = build_member_to_module(&merged);
    let dependency_edges = module_dependency_edges(graph, &member_to_module);
    let stats = module_map_stats(&merged, graph, &dependency_edges);
    reading_guide_from_stats(&stats, &dependency_edges)
}

fn reading_guide_from_stats(
    stats: &[ModuleMapStats],
    dependency_edges: &[ModuleDependencyEdge],
) -> Vec<ReadingGuideStep> {
    if stats.is_empty() {
        return Vec::new();
    }

    let pagerank = module_pagerank(stats, dependency_edges);
    let max_rank = pagerank.values().copied().fold(0.0_f64, f64::max).max(1.0);
    let max_coupling = stats
        .iter()
        .map(|s| s.incoming_calls + s.outgoing_calls)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let max_entry_points = stats
        .iter()
        .map(|s| s.entry_point_count)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let max_members = stats
        .iter()
        .map(|s| s.member_count)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let max_files = stats
        .iter()
        .map(|s| s.source_file_count)
        .max()
        .unwrap_or(1)
        .max(1) as f64;

    let mut guide = stats
        .iter()
        .map(|s| {
            let coupling = s.incoming_calls + s.outgoing_calls;
            let centrality = pagerank.get(&s.filename).copied().unwrap_or_default() / max_rank;
            let score = 0.40 * centrality
                + 0.25 * (coupling as f64 / max_coupling)
                + 0.15 * (s.entry_point_count as f64 / max_entry_points)
                + 0.12 * (s.member_count as f64 / max_members)
                + 0.08 * (s.source_file_count as f64 / max_files);
            ReadingGuideStep {
                filename: s.filename.clone(),
                label: s.label.clone(),
                score,
                reason: reading_guide_reason(s, centrality),
                question: reading_guide_question(s),
            }
        })
        .collect::<Vec<_>>();

    guide.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });
    guide
}

fn module_pagerank(
    stats: &[ModuleMapStats],
    dependency_edges: &[ModuleDependencyEdge],
) -> HashMap<String, f64> {
    let n = stats.len();
    if n == 0 {
        return HashMap::new();
    }

    let modules = stats
        .iter()
        .map(|s| s.filename.clone())
        .collect::<Vec<String>>();
    let module_set = modules.iter().cloned().collect::<HashSet<String>>();
    let mut rank = modules
        .iter()
        .map(|name| (name.clone(), 1.0 / n as f64))
        .collect::<HashMap<String, f64>>();

    let mut outgoing_weight: HashMap<String, usize> = HashMap::new();
    for edge in dependency_edges {
        if module_set.contains(&edge.source) && module_set.contains(&edge.target) {
            *outgoing_weight.entry(edge.source.clone()).or_insert(0) += edge.count;
        }
    }

    let damping = 0.85_f64;
    for _ in 0..24 {
        let mut next = modules
            .iter()
            .map(|name| (name.clone(), (1.0 - damping) / n as f64))
            .collect::<HashMap<String, f64>>();

        let dangling_rank: f64 = modules
            .iter()
            .filter(|name| outgoing_weight.get(*name).copied().unwrap_or(0) == 0)
            .map(|name| rank.get(name).copied().unwrap_or_default())
            .sum();
        if dangling_rank > 0.0 {
            let share = damping * dangling_rank / n as f64;
            for value in next.values_mut() {
                *value += share;
            }
        }

        for edge in dependency_edges {
            if !module_set.contains(&edge.source) || !module_set.contains(&edge.target) {
                continue;
            }
            let total = outgoing_weight.get(&edge.source).copied().unwrap_or(0);
            if total == 0 {
                continue;
            }
            let source_rank = rank.get(&edge.source).copied().unwrap_or_default();
            let contribution = damping * source_rank * edge.count as f64 / total as f64;
            *next.entry(edge.target.clone()).or_insert(0.0) += contribution;
        }
        rank = next;
    }

    rank
}

fn reading_guide_reason(stats: &ModuleMapStats, centrality: f64) -> String {
    let coupling = stats.incoming_calls + stats.outgoing_calls;
    let mut signals = Vec::new();
    if centrality >= 0.80 {
        signals.push("central dans le graphe de dépendances".to_string());
    }
    if stats.entry_point_count > 0 {
        signals.push(pluralized_count(
            stats.entry_point_count,
            "point d'entrée probable",
            "points d'entrée probables",
        ));
    }
    if coupling > 0 {
        signals.push(pluralized_count(
            coupling,
            "lien inter-modules",
            "liens inter-modules",
        ));
    }
    if stats.member_count > 0 {
        signals.push(pluralized_count(stats.member_count, "symbole", "symboles"));
    }
    if signals.is_empty() {
        "module isolé, utile comme lecture ciblée".to_string()
    } else {
        signals.join(", ")
    }
}

fn reading_guide_question(stats: &ModuleMapStats) -> String {
    if stats.incoming_calls > 0 && stats.outgoing_calls > 0 {
        format!(
            "Quels flux traversent {} et quels modules l'appellent ?",
            stats.label
        )
    } else if stats.outgoing_calls > 0 {
        format!("Quelles responsabilités {} délègue-t-il ?", stats.label)
    } else if stats.incoming_calls > 0 {
        format!(
            "Pourquoi {} est-il appelé par autant de modules ?",
            stats.label
        )
    } else if stats.entry_point_count > 0 {
        format!("Quels parcours commencent dans {} ?", stats.label)
    } else {
        format!("Quels fichiers expliquent le rôle de {} ?", stats.label)
    }
}

fn overview_subsystem_description(info: &CommunityInfo, stats: &ModuleMapStats) -> String {
    if let Some(description) = info.description.as_deref().map(str::trim) {
        if !description.is_empty() {
            return compact_wiki_text(description, 180);
        }
    }

    let keywords = info
        .keywords
        .iter()
        .map(|kw| kw.trim())
        .filter(|kw| !kw.is_empty())
        .take(6)
        .collect::<Vec<_>>();
    if !keywords.is_empty() {
        return format!("Focus: {}", keywords.join(", "));
    }

    let mut signals = vec![pluralized_count(stats.member_count, "symbole", "symboles")];
    if stats.source_file_count > 0 {
        signals.push(pluralized_count(
            stats.source_file_count,
            "fichier source",
            "fichiers source",
        ));
    }
    let coupling_count = stats.incoming_calls + stats.outgoing_calls;
    if coupling_count > 0 {
        signals.push(pluralized_count(
            coupling_count,
            "lien inter-modules",
            "liens inter-modules",
        ));
    }
    if stats.entry_point_count > 0 {
        signals.push(pluralized_count(
            stats.entry_point_count,
            "point d'entrée probable",
            "points d'entrée probables",
        ));
    }
    signals.join(", ")
}

fn pluralized_count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {}", singular)
    } else {
        format!("{} {}", count, plural)
    }
}

fn format_language_stats_fr(lang_stats: &BTreeMap<String, usize>) -> Vec<String> {
    lang_stats
        .iter()
        .map(|(lang, count)| {
            format!(
                "{} ({})",
                display_language_name(lang),
                pluralized_count(*count, "fichier", "fichiers")
            )
        })
        .collect()
}

fn french_codebase_description(lang_stats: &BTreeMap<String, usize>) -> String {
    let primary_lang = lang_stats
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(lang, _)| lang.as_str())
        .unwrap_or("multi-langage");
    if primary_lang == "multi-language" || primary_lang == "multi-langage" {
        "Base de code multi-langage indexée par Code Explorer.".to_string()
    } else {
        format!(
            "Base de code principalement {} indexée par Code Explorer.",
            display_language_name(primary_lang)
        )
    }
}

fn display_language_name(lang: &str) -> &str {
    match lang {
        "javascript" => "JavaScript",
        "typescript" => "TypeScript",
        "rust" => "Rust",
        "csharp" => "C#",
        "python" => "Python",
        "java" => "Java",
        "go" => "Go",
        other => other,
    }
}

fn module_link_for_node(
    node_id: &str,
    member_to_module: &HashMap<String, String>,
    communities: &BTreeMap<String, CommunityInfo>,
    module_path_prefix: &str,
) -> Option<String> {
    let filename = member_to_module.get(node_id)?;
    let info = communities.get(filename)?;
    let prefix = module_path_prefix.trim_end_matches('/');
    Some(format!("[{}]({}/{}.md)", info.label, prefix, filename))
}

fn line_range(node: &GraphNode) -> String {
    match (node.properties.start_line, node.properties.end_line) {
        (Some(start), Some(end)) => format!("{}-{}", start, end),
        (Some(start), None) => start.to_string(),
        _ => "-".to_string(),
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum WikiSteeringNote {
    Object {
        content: String,
        #[serde(default)]
        author: Option<String>,
    },
    Text(String),
}

impl WikiSteeringNote {
    fn content(&self) -> &str {
        match self {
            Self::Object { content, .. } => content,
            Self::Text(content) => content,
        }
    }

    fn author(&self) -> Option<&str> {
        match self {
            Self::Object { author, .. } => author.as_deref(),
            Self::Text(_) => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct WikiSteeringPage {
    title: String,
    purpose: String,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    page_notes: Vec<WikiSteeringNote>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct WikiSteeringConfig {
    #[serde(default)]
    repo_notes: Vec<WikiSteeringNote>,
    #[serde(default)]
    pages: Vec<WikiSteeringPage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GuidedSymbolMatch {
    name: String,
    kind: String,
    module: String,
    file_path: String,
    lines: String,
    score: usize,
}

fn read_wiki_steering_config(repo_path: &Path) -> Result<Option<(PathBuf, WikiSteeringConfig)>> {
    let config_path = repo_path.join(".devin").join("wiki.json");
    if !config_path.is_file() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: WikiSteeringConfig = serde_json::from_str(&raw)
        .with_context(|| format!("Invalid DeepWiki steering file {}", config_path.display()))?;
    validate_wiki_steering_config(&config, &config_path)?;
    Ok(Some((config_path, config)))
}

fn validate_wiki_steering_config(config: &WikiSteeringConfig, config_path: &Path) -> Result<()> {
    ensure!(
        config.pages.len() <= 30,
        "{} defines {} pages; Code Explorer supports up to 30 guided pages",
        config_path.display(),
        config.pages.len()
    );

    let mut titles = HashSet::new();
    for page in &config.pages {
        let title = page.title.trim();
        ensure!(
            !title.is_empty(),
            "{} contains a guided page with an empty title",
            config_path.display()
        );
        ensure!(
            titles.insert(title.to_lowercase()),
            "{} contains duplicate guided page title `{}`",
            config_path.display(),
            title
        );
        ensure!(
            !page.purpose.trim().is_empty(),
            "{} contains guided page `{}` with an empty purpose",
            config_path.display(),
            title
        );
    }

    let note_count = config.repo_notes.len()
        + config
            .pages
            .iter()
            .map(|page| page.page_notes.len())
            .sum::<usize>();
    ensure!(
        note_count <= 100,
        "{} defines {} notes; Code Explorer supports up to 100 total notes",
        config_path.display(),
        note_count
    );

    for note in config
        .repo_notes
        .iter()
        .chain(config.pages.iter().flat_map(|page| page.page_notes.iter()))
    {
        ensure!(
            note.content().chars().count() <= 10_000,
            "{} contains a note longer than 10000 characters",
            config_path.display()
        );
    }

    Ok(())
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace('\r', " ")
        .replace('\n', "<br>")
}

fn compact_wiki_text(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn unique_wiki_page_slug(title: &str, used: &mut HashSet<String>) -> String {
    let mut base = sanitize_filename(title)
        .trim_matches('_')
        .trim_matches('-')
        .to_string();
    if base.is_empty() {
        base = "page".to_string();
    }

    let mut candidate = base.clone();
    let mut n = 2u32;
    while !used.insert(candidate.clone()) {
        candidate = format!("{}-{}", base, n);
        n += 1;
    }
    candidate
}

fn wiki_steering_tokens(page: &WikiSteeringPage) -> BTreeSet<String> {
    const STOP_WORDS: &[&str] = &[
        "and",
        "avec",
        "dans",
        "des",
        "document",
        "documenter",
        "documentation",
        "du",
        "for",
        "les",
        "pour",
        "sur",
        "the",
        "une",
    ];

    let mut text = format!("{} {}", page.title, page.purpose);
    if let Some(parent) = &page.parent {
        text.push(' ');
        text.push_str(parent);
    }
    for note in &page.page_notes {
        text.push(' ');
        text.push_str(note.content());
    }

    text.split(|ch: char| !ch.is_alphanumeric())
        .map(|token| token.to_lowercase())
        .filter(|token| token.len() >= 3 && !STOP_WORDS.contains(&token.as_str()))
        .collect()
}

fn source_files_for_guided_page(graph: &KnowledgeGraph, page: &WikiSteeringPage) -> Vec<String> {
    let tokens = wiki_steering_tokens(page);
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut scored = graph
        .iter_nodes()
        .filter(|node| node.label == NodeLabel::File)
        .filter_map(|node| {
            let haystack = format!(
                "{} {}",
                node.properties.name.to_lowercase(),
                node.properties.file_path.to_lowercase()
            );
            let score = tokens
                .iter()
                .filter(|token| haystack.contains(token.as_str()))
                .count();
            (score > 0).then(|| (score, node.properties.file_path.clone()))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.dedup_by(|a, b| a.1 == b.1);
    scored.into_iter().take(15).map(|(_, path)| path).collect()
}

fn guided_symbol_matches(
    graph: &KnowledgeGraph,
    page: &WikiSteeringPage,
    member_to_module: &HashMap<String, String>,
    communities: &BTreeMap<String, CommunityInfo>,
    module_path_prefix: &str,
) -> Vec<GuidedSymbolMatch> {
    let tokens = wiki_steering_tokens(page);
    if tokens.is_empty() {
        return Vec::new();
    }

    let mut scored = graph
        .iter_nodes()
        .filter(|node| {
            !matches!(
                node.label,
                NodeLabel::File | NodeLabel::Folder | NodeLabel::Community
            ) && !node.properties.file_path.is_empty()
        })
        .filter_map(|node| {
            let haystack = format!(
                "{} {} {}",
                node.properties.name.to_lowercase(),
                node.properties.file_path.to_lowercase(),
                node.label.as_str().to_lowercase()
            );
            let mut score = tokens
                .iter()
                .filter(|token| haystack.contains(token.as_str()))
                .count();
            if node
                .properties
                .entry_point_score
                .map(|value| value > 0.3)
                .unwrap_or(false)
            {
                score += 1;
            }

            (score > 0).then(|| GuidedSymbolMatch {
                name: node.properties.name.clone(),
                kind: node.label.as_str().to_string(),
                module: module_link_for_node(
                    &node.id,
                    member_to_module,
                    communities,
                    module_path_prefix,
                )
                .unwrap_or_else(|| "-".to_string()),
                file_path: node.properties.file_path.clone(),
                lines: line_range(node),
                score,
            })
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    scored.dedup_by(|a, b| a.name == b.name && a.file_path == b.file_path);
    scored.truncate(30);
    scored
}

fn generate_wiki_steering_pages(
    docs_dir: &Path,
    repo_path: &Path,
    graph: &KnowledgeGraph,
) -> Result<Vec<(String, String, String)>> {
    let Some((config_path, config)) = read_wiki_steering_config(repo_path)? else {
        return Ok(Vec::new());
    };

    let mut nav_pages = Vec::new();
    let rel_config_path = config_path
        .strip_prefix(repo_path)
        .unwrap_or(config_path.as_path())
        .display()
        .to_string();

    let steering_path = docs_dir.join("wiki-steering.md");
    let mut f = std::fs::File::create(&steering_path)?;
    writeln!(f, "# Plan de Wiki Guidé")?;
    writeln!(f, "<!-- GNX:LEAD -->")?;
    writeln!(f)?;
    writeln!(
        f,
        "Code Explorer a détecté `{}` et l'utilise comme manifeste de pilotage compatible DeepWiki. Les pages ci-dessous complètent la génération automatique au lieu de la remplacer.",
        rel_config_path.replace('\\', "/")
    )?;
    writeln!(f)?;

    if !config.repo_notes.is_empty() {
        writeln!(f, "## Notes de dépôt")?;
        writeln!(f)?;
        for note in &config.repo_notes {
            match note.author() {
                Some(author) if !author.trim().is_empty() => {
                    writeln!(
                        f,
                        "- **{}** : {}",
                        author,
                        compact_wiki_text(note.content(), 700)
                    )?;
                }
                _ => {
                    writeln!(f, "- {}", compact_wiki_text(note.content(), 700))?;
                }
            }
        }
        writeln!(f)?;
    }

    if !config.pages.is_empty() {
        writeln!(f, "## Hiérarchie demandée")?;
        writeln!(f, "<!-- GNX:INTRO:guided-pages -->")?;
        writeln!(f)?;
        writeln!(f, "```mermaid")?;
        writeln!(f, "graph TD")?;
        for page in &config.pages {
            let page_id = format!("P_{}", sanitize_mermaid_id(&sanitize_filename(&page.title)));
            writeln!(
                f,
                "    {}[\"{}\"]",
                page_id,
                escape_mermaid_label(&page.title)
            )?;
            if let Some(parent) = &page.parent {
                let parent_id = format!("P_{}", sanitize_mermaid_id(&sanitize_filename(parent)));
                writeln!(
                    f,
                    "    {}[\"{}\"] --> {}",
                    parent_id,
                    escape_mermaid_label(parent),
                    page_id
                )?;
            }
        }
        writeln!(f, "```")?;
        writeln!(f)?;

        writeln!(f, "| Page | Parent | Objectif | Notes |")?;
        writeln!(f, "|------|--------|----------|-------|")?;
        for page in &config.pages {
            let notes = page
                .page_notes
                .iter()
                .map(|note| compact_wiki_text(note.content(), 180))
                .collect::<Vec<_>>()
                .join("<br>");
            writeln!(
                f,
                "| {} | {} | {} | {} |",
                markdown_cell(&page.title),
                markdown_cell(page.parent.as_deref().unwrap_or("-")),
                markdown_cell(&compact_wiki_text(&page.purpose, 260)),
                markdown_cell(if notes.is_empty() { "-" } else { &notes })
            )?;
        }
        writeln!(f)?;
    }

    writeln!(f, "<!-- GNX:CLOSING -->")?;
    writeln!(f, "## Utilisation")?;
    writeln!(f)?;
    writeln!(
        f,
        "Utilise cette page pour contrôler les angles documentaires que Code Explorer doit couvrir : notes globales pour orienter l'analyse, pages guidées pour forcer les composants importants à apparaître dans le wiki."
    )?;
    writeln!(f)?;
    writeln!(f, "---")?;
    writeln!(
        f,
        "[<- Previous: Carte du Code](./code-map.md) | [Next: Getting Started ->](./getting-started.md)"
    )?;
    println!("  {} wiki-steering.md", "OK".green());
    nav_pages.push((
        "wiki-steering".to_string(),
        "Plan de Wiki Guidé".to_string(),
        "wiki-steering.md".to_string(),
    ));

    if !config.pages.is_empty() {
        let guided_dir = docs_dir.join("guided");
        std::fs::create_dir_all(&guided_dir)?;
        let mut used_slugs = HashSet::new();
        let merged_communities = merged_communities_by_filename(&collect_communities(graph));
        let member_to_module = build_member_to_module(&merged_communities);
        let page_titles: HashSet<&str> = config
            .pages
            .iter()
            .map(|page| page.title.as_str())
            .collect();

        for page in &config.pages {
            let slug = unique_wiki_page_slug(&page.title, &mut used_slugs);
            let page_path = guided_dir.join(format!("{}.md", slug));
            let mut page_file = std::fs::File::create(&page_path)?;
            let symbol_matches = guided_symbol_matches(
                graph,
                page,
                &member_to_module,
                &merged_communities,
                "../modules",
            );
            let mut source_files = source_files_for_guided_page(graph, page);
            for symbol in &symbol_matches {
                if !source_files.contains(&symbol.file_path) {
                    source_files.push(symbol.file_path.clone());
                }
            }
            source_files.truncate(15);
            let source_refs: Vec<&str> = source_files.iter().map(|s| s.as_str()).collect();

            writeln!(page_file, "# {}", page.title)?;
            writeln!(page_file, "<!-- GNX:LEAD -->")?;
            writeln!(page_file)?;
            write!(page_file, "{}", source_files_section(&source_refs))?;
            writeln!(page_file, "## Objectif")?;
            writeln!(page_file)?;
            writeln!(page_file, "{}", page.purpose)?;
            writeln!(page_file)?;

            if let Some(parent) = &page.parent {
                let parent_text = if page_titles.contains(parent.as_str()) {
                    format!("Page parente demandée : **{}**.", parent)
                } else {
                    format!(
                        "Page parente demandée : **{}** (non définie dans le manifeste).",
                        parent
                    )
                };
                writeln!(page_file, "{}", parent_text)?;
                writeln!(page_file)?;
            }

            if !page.page_notes.is_empty() {
                writeln!(page_file, "## Notes de page")?;
                writeln!(page_file)?;
                for note in &page.page_notes {
                    writeln!(page_file, "- {}", compact_wiki_text(note.content(), 900))?;
                }
                writeln!(page_file)?;
            }

            if !symbol_matches.is_empty() {
                let mut modules = Vec::new();
                for symbol in &symbol_matches {
                    if symbol.module != "-" && !modules.contains(&symbol.module) {
                        modules.push(symbol.module.clone());
                    }
                    if modules.len() >= 8 {
                        break;
                    }
                }

                if !modules.is_empty() {
                    writeln!(page_file, "## Modules liés")?;
                    writeln!(page_file)?;
                    for module in &modules {
                        writeln!(page_file, "- {}", module)?;
                    }
                    writeln!(page_file)?;
                }

                writeln!(page_file, "## Symboles probables")?;
                writeln!(page_file)?;
                writeln!(
                    page_file,
                    "| Symbole | Type | Module | Fichier | Lignes | Score |"
                )?;
                writeln!(
                    page_file,
                    "|---------|------|--------|---------|--------|-------|"
                )?;
                for symbol in symbol_matches.iter().take(20) {
                    writeln!(
                        page_file,
                        "| `{}` | {} | {} | `{}` | {} | {} |",
                        markdown_cell(&symbol.name),
                        markdown_cell(&symbol.kind),
                        markdown_cell(&symbol.module),
                        markdown_cell(&symbol.file_path),
                        symbol.lines,
                        symbol.score
                    )?;
                }
                writeln!(page_file)?;
            }

            writeln!(page_file, "## Questions de vérification")?;
            writeln!(page_file)?;
            writeln!(
                page_file,
                "- Quels fichiers prouvent l'objectif de cette page ?"
            )?;
            writeln!(
                page_file,
                "- Quels points d'entrée ou dépendances relient cette page à la Carte du Code ?"
            )?;
            writeln!(
                page_file,
                "- Quels flux Mermaid seraient utiles pour rendre cette page plus visuelle ?"
            )?;
            writeln!(page_file)?;
            writeln!(page_file, "<!-- GNX:CLOSING -->")?;
            writeln!(page_file, "## Résumé")?;
            writeln!(page_file)?;
            writeln!(
                page_file,
                "Cette page est issue du manifeste `.devin/wiki.json`; elle sert de canevas guidé pour l'enrichissement LLM et la revue manuelle."
            )?;
            writeln!(page_file)?;
            writeln!(page_file, "---")?;
            writeln!(page_file, "[<- Retour au plan guidé](../wiki-steering.md)")?;

            println!("  {} guided/{slug}.md", "OK".green());
            nav_pages.push((
                format!("guided-{}", slug),
                page.title.clone(),
                format!("guided/{}.md", slug),
            ));
        }
    }

    Ok(nav_pages)
}

/// Generate a DeepWiki-style source navigation map.
fn generate_docs_code_map(
    docs_dir: &Path,
    communities: &BTreeMap<String, CommunityInfo>,
    graph: &KnowledgeGraph,
    _edge_map: &HashMap<String, Vec<(String, RelationshipType)>>,
) -> Result<()> {
    let out_path = docs_dir.join("code-map.md");
    let mut f = std::fs::File::create(&out_path)?;

    let merged = merged_communities_by_filename(communities);
    let member_to_module = build_member_to_module(&merged);
    let dependency_edges = module_dependency_edges(graph, &member_to_module);
    let module_stats = module_map_stats(&merged, graph, &dependency_edges);
    let reading_guide = reading_guide_from_stats(&module_stats, &dependency_edges);

    writeln!(f, "# Carte du Code")?;
    writeln!(f)?;
    writeln!(
        f,
        "Cette page sert de carte de lecture façon DeepWiki : elle relie les modules, les points d'entrée, les fichiers pivots et les prochaines questions à poser au chat Code Explorer."
    )?;
    writeln!(f)?;

    if !reading_guide.is_empty() {
        writeln!(f, "## Guide de Lecture Automatique")?;
        writeln!(f)?;
        writeln!(
            f,
            "Ce classement combine centralité du graphe, couplage inter-modules, points d'entrée, taille et fichiers pivots. Il donne une trajectoire de lecture plutôt qu'une simple liste alphabétique."
        )?;
        writeln!(f)?;
        writeln!(f, "| Rang | Module | Score | Pourquoi | Question utile |")?;
        writeln!(f, "|------|--------|-------|----------|----------------|")?;
        for (idx, step) in reading_guide.iter().take(10).enumerate() {
            writeln!(
                f,
                "| {} | [{}](modules/{}.md) | {} | {} | {} |",
                idx + 1,
                markdown_cell(&step.label),
                step.filename,
                (step.score * 100.0).round() as usize,
                markdown_cell(&step.reason),
                markdown_cell(&step.question)
            )?;
        }
        writeln!(f)?;
    }

    if !dependency_edges.is_empty() {
        writeln!(f, "## Graphe des Modules")?;
        writeln!(f)?;
        writeln!(f, "```mermaid")?;
        writeln!(f, "graph LR")?;
        for stats in module_stats.iter().take(20) {
            let id = format!("M_{}", sanitize_mermaid_id(&stats.filename));
            writeln!(f, "    {}[\"{}\"]", id, escape_mermaid_label(&stats.label))?;
        }
        let visible_modules: HashSet<&str> = module_stats
            .iter()
            .take(20)
            .map(|stats| stats.filename.as_str())
            .collect();
        for edge in dependency_edges
            .iter()
            .filter(|edge| {
                visible_modules.contains(edge.source.as_str())
                    && visible_modules.contains(edge.target.as_str())
            })
            .take(30)
        {
            let source_id = format!("M_{}", sanitize_mermaid_id(&edge.source));
            let target_id = format!("M_{}", sanitize_mermaid_id(&edge.target));
            writeln!(f, "    {} -->|{}| {}", source_id, edge.count, target_id)?;
        }
        writeln!(f, "```")?;
        writeln!(f)?;
    }

    writeln!(f, "## Modules à Lire en Premier")?;
    writeln!(f)?;
    writeln!(
        f,
        "| Module | Fichiers | Symboles | Entrées | Entrant | Sortant | Fichiers pivots |"
    )?;
    writeln!(
        f,
        "|--------|----------|----------|---------|---------|---------|----------------|"
    )?;
    for stats in module_stats.iter().take(25) {
        let files = if stats.source_files.is_empty() {
            "-".to_string()
        } else {
            stats
                .source_files
                .iter()
                .take(3)
                .map(|path| format!("`{}`", path))
                .collect::<Vec<_>>()
                .join("<br>")
        };
        writeln!(
            f,
            "| [{}](modules/{}.md) | {} | {} | {} | {} | {} | {} |",
            stats.label,
            stats.filename,
            stats.source_file_count,
            stats.member_count,
            stats.entry_point_count,
            stats.incoming_calls,
            stats.outgoing_calls,
            files
        )?;
    }
    writeln!(f)?;

    let mut entry_points: Vec<(&GraphNode, f64)> = graph
        .iter_nodes()
        .filter_map(|node| {
            node.properties
                .entry_point_score
                .filter(|score| *score > 0.3)
                .map(|score| (node, score))
        })
        .collect();
    entry_points.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if !entry_points.is_empty() {
        writeln!(f, "## Points d'Entrée")?;
        writeln!(f)?;
        writeln!(f, "| Symbole | Type | Module | Fichier | Lignes | Score |")?;
        writeln!(f, "|---------|------|--------|---------|--------|-------|")?;
        for (node, score) in entry_points.iter().take(30) {
            let module = module_link_for_node(&node.id, &member_to_module, &merged, "modules")
                .unwrap_or_else(|| "-".to_string());
            writeln!(
                f,
                "| `{}` | {} | {} | `{}` | {} | {:.2} |",
                node.properties.name,
                node.label.as_str(),
                module,
                node.properties.file_path,
                line_range(node),
                score
            )?;
        }
        writeln!(f)?;
    }

    let top_files = top_connected_files(graph, 25);
    if !top_files.is_empty() {
        writeln!(f, "## Fichiers Pivots")?;
        writeln!(f)?;
        writeln!(
            f,
            "Ces fichiers ont beaucoup de relations dans le graphe et servent souvent de points de départ pour une revue manuelle."
        )?;
        writeln!(f)?;
        for file in top_files {
            writeln!(f, "- `{}`", file)?;
        }
        writeln!(f)?;
    }

    writeln!(f, "## Questions à Poser au Chat")?;
    writeln!(f)?;
    writeln!(
        f,
        "- Explique le module le plus connecté et cite ses fichiers pivots."
    )?;
    writeln!(
        f,
        "- Trace le flux d'un point d'entrée de cette carte jusqu'à la persistance."
    )?;
    writeln!(
        f,
        "- Liste les dépendances entrantes et sortantes du module que je sélectionne."
    )?;
    writeln!(
        f,
        "- Génère un diagramme Mermaid de séquence pour un point d'entrée métier."
    )?;
    writeln!(f)?;

    writeln!(f, "## Résumé")?;
    writeln!(f)?;
    writeln!(
        f,
        "La carte du code donne une première trajectoire d'exploration : partir des modules à forte connectivité, ouvrir leurs pages détaillées, puis demander au chat un flux ou une analyse ciblée avec les fichiers cités."
    )?;
    writeln!(f)?;
    writeln!(
        f,
        "**Voir aussi :** [Architecture](./architecture.md) · [Prise en Main](./getting-started.md)"
    )?;
    writeln!(f)?;
    writeln!(f, "---")?;
    writeln!(
        f,
        "[<- Précédent : Architecture](./architecture.md) | [Suivant : Prise en Main ->](./getting-started.md)"
    )?;

    println!("  {} code-map.md", "OK".green());
    Ok(())
}

/// Generate getting-started.md guide.
fn generate_docs_getting_started(
    docs_dir: &Path,
    repo_name: &str,
    communities: &BTreeMap<String, CommunityInfo>,
    graph: &KnowledgeGraph,
) -> Result<()> {
    let out_path = docs_dir.join("getting-started.md");
    let mut f = std::fs::File::create(&out_path)?;

    let mut ep_files: Vec<String> = graph
        .iter_nodes()
        .filter(|n| {
            n.properties
                .entry_point_score
                .map(|s| s > 0.3)
                .unwrap_or(false)
        })
        .map(|n| n.properties.file_path.clone())
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    ep_files.truncate(15);
    let ep_file_refs: Vec<&str> = ep_files.iter().map(|s| s.as_str()).collect();
    let reading_guide = module_reading_guide(communities, graph);

    writeln!(f, "# Prise en Main")?;
    writeln!(f)?;
    write!(f, "{}", source_files_section(&ep_file_refs))?;
    writeln!(f, "Bienvenue dans le codebase **{}**.", repo_name)?;
    writeln!(f)?;

    writeln!(f, "## Structure des Projets")?;
    writeln!(f)?;

    let mut project_files: BTreeMap<String, usize> = BTreeMap::new();
    for node in graph.iter_nodes() {
        if node.label == NodeLabel::File {
            let path = &node.properties.file_path;
            if !path.is_empty() && !path.contains("PackageTmp") && !path.contains("/obj/") {
                let project = path.split(['/', '\\']).next().unwrap_or("Other");
                *project_files.entry(project.to_string()).or_insert(0) += 1;
            }
        }
    }

    if !project_files.is_empty() {
        writeln!(
            f,
            "La solution contient **{} projets** :",
            project_files.len()
        )?;
        writeln!(f)?;
        writeln!(f, "| Projet | Fichiers | Rôle |")?;
        writeln!(f, "|--------|----------|------|")?;
        let mut projects: Vec<_> = project_files.iter().collect();
        projects.sort_by(|a, b| b.1.cmp(a.1));
        for (project, count) in &projects {
            let role = describe_project_fr(project);
            writeln!(f, "| `{}` | {} | {} |", project, count, role)?;
        }
        writeln!(f)?;
    }

    if !reading_guide.is_empty() {
        writeln!(f, "## Parcours de Lecture Recommandé")?;
        writeln!(f)?;
        writeln!(
            f,
            "Commencez par les modules qui structurent le plus le graphe. Ce parcours est calculé depuis les dépendances réelles plutôt que depuis l'ordre des dossiers."
        )?;
        writeln!(f)?;
        for (idx, step) in reading_guide.iter().take(6).enumerate() {
            writeln!(
                f,
                "{}. [{}](modules/{}.md) — {}",
                idx + 1,
                step.label,
                step.filename,
                step.reason
            )?;
            writeln!(f, "   - Question à poser : {}", step.question)?;
        }
        writeln!(f)?;
    }

    let mut entry_points: Vec<(&GraphNode, f64)> = graph
        .iter_nodes()
        .filter_map(|n| {
            n.properties
                .entry_point_score
                .filter(|&s| s > 0.3)
                .map(|s| (n, s))
        })
        .collect();
    entry_points.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if !entry_points.is_empty() {
        writeln!(f, "## Points d'Entrée Principaux")?;
        writeln!(f)?;
        writeln!(f, "Commencez l'exploration par ces points d'entrée :")?;
        writeln!(f)?;
        for (node, _score) in entry_points.iter().take(10) {
            writeln!(
                f,
                "- `{}` dans `{}`",
                node.properties.name, node.properties.file_path
            )?;
        }
        writeln!(f)?;
    }

    let has_controllers = graph.iter_nodes().any(|n| n.label == NodeLabel::Controller);
    if has_controllers {
        writeln!(f, "## Prérequis & Setup local")?;
        writeln!(f)?;
        writeln!(
            f,
            "Ce projet est une application **ASP.NET MVC 5** (.NET Framework)."
        )?;
        writeln!(f)?;
        writeln!(f, "### Prérequis")?;
        writeln!(f)?;
        writeln!(f, "| Outil | Version | Notes |")?;
        writeln!(f, "|-------|---------|-------|")?;
        writeln!(
            f,
            "| Visual Studio | 2019+ | Avec le workload \"Développement web ASP.NET\" |"
        )?;
        writeln!(
            f,
            "| .NET Framework | 4.6.1+ | Vérifier dans `web.config` \u{2192} `targetFramework` |"
        )?;
        writeln!(
            f,
            "| SQL Server | 2016+ | Base de données locale ou distante |"
        )?;
        writeln!(f, "| IIS Express | intégré à VS | Pour le debug local |")?;
        writeln!(f)?;
        writeln!(f, "### Étapes de démarrage")?;
        writeln!(f)?;
        writeln!(f, "1. **Ouvrir la solution** `.sln` dans Visual Studio")?;
        writeln!(f, "2. **Restaurer les packages NuGet** : clic droit sur la solution \u{2192} Restaurer les packages NuGet")?;
        writeln!(f, "3. **Configurer la connexion DB** : vérifier `web.config` \u{2192} `<connectionStrings>`")?;
        writeln!(f, "4. **Compiler** : Ctrl+Shift+B")?;
        writeln!(f, "5. **Lancer** : F5 (IIS Express)")?;
        writeln!(f)?;

        let config_files: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| {
                n.label == NodeLabel::File
                    && (n.properties.file_path.ends_with("web.config")
                        || n.properties.file_path.ends_with("Web.config"))
                    && !n.properties.file_path.contains("PackageTmp")
                    && !n.properties.file_path.contains("/obj/")
            })
            .collect();

        if !config_files.is_empty() {
            writeln!(f, "### Fichiers de configuration")?;
            writeln!(f)?;
            for cf in &config_files {
                writeln!(f, "- `{}`", cf.properties.file_path.replace('\\', "/"))?;
            }
            writeln!(f)?;
        }
    }

    writeln!(f, "## Pour aller plus loin")?;
    writeln!(f)?;
    writeln!(
        f,
        "- Consultez l'**Architecture** pour comprendre les couches du système"
    )?;
    writeln!(
        f,
        "- Explorez les **Controllers** pour voir les fonctionnalités par écran"
    )?;
    writeln!(
        f,
        "- Le **Guide Fonctionnel** décrit chaque module du point de vue métier"
    )?;
    writeln!(
        f,
        "- Les **Services Externes** détaillent les intégrations (ExternalApi, WCF)"
    )?;
    writeln!(f)?;

    writeln!(
        f,
        "**Voir aussi :** [Vue d'ensemble](./overview.md) · [Carte du Code](./code-map.md) · [Architecture](./architecture.md)"
    )?;
    writeln!(f)?;
    writeln!(f, "---")?;
    writeln!(
        f,
        "[<- Précédent : Carte du Code](./code-map.md) | [Suivant : Déploiement ->](./deployment.md)"
    )?;

    println!("  {} getting-started.md", "OK".green());
    Ok(())
}

// The generate_docs_modules function is extremely large (lines 3226-4484 in original).
// It is included in full below to preserve all functionality.

/// Generate per-module documentation files with page ordering and navigation.
fn generate_docs_modules(
    modules_dir: &Path,
    communities: &BTreeMap<String, CommunityInfo>,
    graph: &KnowledgeGraph,
    edge_map: &HashMap<String, Vec<(String, RelationshipType)>>,
    repo_path: &Path,
) -> Result<usize> {
    let mut page_count: usize = 0;

    let mut page_order: Vec<(String, String)> = vec![
        ("../overview".to_string(), "Overview".to_string()),
        (
            "../project-health".to_string(),
            "Santé du Projet".to_string(),
        ),
        ("../architecture".to_string(), "Architecture".to_string()),
        ("../code-map".to_string(), "Carte du Code".to_string()),
        (
            "../getting-started".to_string(),
            "Getting Started".to_string(),
        ),
    ];

    let merged_communities = merged_communities_by_filename(communities);
    let module_member_to_filename = build_member_to_module(&merged_communities);
    let dependency_edges = module_dependency_edges(graph, &module_member_to_filename);

    for (filename, info) in &merged_communities {
        page_order.push((filename.clone(), info.label.clone()));
    }

    // Pre-compute action counts per controller file_path so we can drop
    // controllers that the per-controller loop below will skip (it bails out
    // when `actions.len() < 3`). Without this filter, the skipped controllers
    // still land in `page_order`, which causes neighbouring pages' nav
    // footers to emit `[Next: FooController](./ctrl-FooController.md)` links
    // pointing at files that were never written to disk.
    let mut action_counts: HashMap<String, usize> = HashMap::new();
    for n in graph.iter_nodes() {
        if n.label == NodeLabel::ControllerAction {
            *action_counts
                .entry(n.properties.file_path.clone())
                .or_insert(0) += 1;
        }
    }

    let mut controllers: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Controller)
        .filter(|c| {
            action_counts
                .get(&c.properties.file_path)
                .copied()
                .unwrap_or(0)
                >= 3
        })
        .collect();
    controllers.sort_by(|a, b| a.properties.name.cmp(&b.properties.name));

    // Derive one filename per controller, disambiguating collisions with a
    // numeric suffix. Controller node IDs already include `file_path`, so
    // two controllers with the same class name in different areas (e.g.
    // `Areas/Admin/HomeController.cs:HomeController` and
    // `Controllers/HomeController.cs:HomeController`) both end up in this
    // list. Previously both produced `ctrl-HomeController.md` and the
    // second `std::fs::write` silently clobbered the first on disk, with
    // the nav footer for the overwritten page pointing at another
    // controller's content.
    //
    // The disambiguator is appended as `-2`, `-3`, ... rather than
    // prepended with the area name because `enrichment.rs::collect_evidence`
    // extracts the controller name by `strip_prefix("ctrl-")` and then
    // substring-matches it against graph node names; inserting an area
    // prefix like `ctrl-admin-homecontroller` would break that match. The
    // base stem (`ctrl-homecontroller`) must still be a substring of the
    // controller's lower-cased name. Enrichment trims the trailing `-N`
    // before matching so the duplicates still resolve to the same node set.
    let mut used_filenames: HashSet<String> = HashSet::new();
    let ctrl_filenames: Vec<(String, String)> = controllers
        .iter()
        .map(|c| {
            let base = format!("ctrl-{}", sanitize_filename(&c.properties.name));
            let mut fname = base.clone();
            let mut n = 2u32;
            while !used_filenames.insert(fname.clone()) {
                fname = format!("{}-{}", base, n);
                n += 1;
            }
            (fname, c.properties.name.clone())
        })
        .collect();
    for (fname, title) in &ctrl_filenames {
        page_order.push((fname.clone(), title.clone()));
    }

    let db_contexts: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::DbContext)
        .collect();
    let data_filenames: Vec<(String, String)> = db_contexts
        .iter()
        .map(|c| {
            let fname = format!("data-{}", sanitize_filename(&c.properties.name));
            (fname, format!("Data Model: {}", c.properties.name))
        })
        .collect();
    for (fname, title) in &data_filenames {
        page_order.push((fname.clone(), title.clone()));
    }

    let services: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Service || n.label == NodeLabel::Repository)
        .collect();
    if !services.is_empty() {
        page_order.push(("services".to_string(), "Service Layer".to_string()));
    }

    let ui_components: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::UiComponent)
        .collect();
    if !ui_components.is_empty() {
        page_order.push(("ui-components".to_string(), "UI Components".to_string()));
    }

    let ajax_calls: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::AjaxCall)
        .collect();
    if !ajax_calls.is_empty() {
        page_order.push(("ajax-endpoints".to_string(), "AJAX Endpoints".to_string()));
    }

    let ext_services: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::ExternalService)
        .collect();
    if !ext_services.is_empty() {
        page_order.push((
            "external-services".to_string(),
            "External Services".to_string(),
        ));
    }

    fn nav_footer(page_order: &[(String, String)], current_filename: &str) -> String {
        let idx = page_order.iter().position(|(f, _)| f == current_filename);
        let mut footer = String::from("\n---\n");
        if let Some(i) = idx {
            if i > 0 {
                let (prev_file, prev_title) = &page_order[i - 1];
                footer.push_str(&format!(
                    "[<- Previous: {}](./{}.md)",
                    prev_title, prev_file
                ));
            }
            if i > 0 && i + 1 < page_order.len() {
                footer.push_str(" | ");
            }
            if i + 1 < page_order.len() {
                let (next_file, next_title) = &page_order[i + 1];
                footer.push_str(&format!("[Next: {} ->](./{}.md)", next_title, next_file));
            }
        }
        footer.push('\n');
        footer
    }

    // The rest of this function generates community pages, controller pages, data model pages,
    // service layer page, UI components page, AJAX endpoints page, and external services page.
    // It is extremely large but must be included in full for correctness.

    // ─── Community / Module pages (deduplicated) ──────────────────────
    for (comm_idx, (filename, info)) in merged_communities.iter().enumerate() {
        let _ = comm_idx;
        let out_path = modules_dir.join(format!("{}.md", filename));
        let mut f = std::fs::File::create(&out_path)?;

        let member_set: HashSet<&str> = info.member_ids.iter().map(|s| s.as_str()).collect();

        let mut files_set: BTreeSet<String> = BTreeSet::new();
        for mid in &info.member_ids {
            if let Some(node) = graph.get_node(mid) {
                if !node.properties.file_path.is_empty() {
                    files_set.insert(node.properties.file_path.clone());
                }
            }
        }
        let files_vec: Vec<&str> = files_set.iter().map(|s| s.as_str()).collect();

        writeln!(f, "# {}", info.label)?;
        writeln!(f)?;
        write!(f, "{}", source_files_section(&files_vec))?;

        if let Some(desc) = &info.description {
            writeln!(f, "{}", desc)?;
            writeln!(f)?;
        }

        if !info.keywords.is_empty() {
            writeln!(f, "**Keywords**: {}", info.keywords.join(", "))?;
            writeln!(f)?;
        }

        let mut internal_calls: Vec<(String, String)> = Vec::new();
        for mid in &info.member_ids {
            if let Some(edges) = edge_map.get(mid.as_str()) {
                for (target_id, rel_type) in edges {
                    if *rel_type == RelationshipType::Calls
                        && member_set.contains(target_id.as_str())
                    {
                        let src_name = graph
                            .get_node(mid)
                            .map(|n| n.properties.name.as_str())
                            .unwrap_or("?");
                        let tgt_name = graph
                            .get_node(target_id)
                            .map(|n| n.properties.name.as_str())
                            .unwrap_or("?");
                        internal_calls.push((src_name.to_string(), tgt_name.to_string()));
                    }
                }
            }
        }

        let entry_point_count = info
            .member_ids
            .iter()
            .filter_map(|mid| graph.get_node(mid))
            .filter(|n| {
                n.properties
                    .entry_point_score
                    .map(|s| s > 0.3)
                    .unwrap_or(false)
            })
            .count();
        let incoming_modules = module_dependency_neighbors(
            filename,
            &merged_communities,
            &dependency_edges,
            ModuleDependencyDirection::Incoming,
        );
        let outgoing_modules = module_dependency_neighbors(
            filename,
            &merged_communities,
            &dependency_edges,
            ModuleDependencyDirection::Outgoing,
        );
        let incoming_call_count: usize = incoming_modules.iter().map(|m| m.count).sum();
        let outgoing_call_count: usize = outgoing_modules.iter().map(|m| m.count).sum();

        writeln!(f, "## Module at a glance")?;
        writeln!(f)?;
        writeln!(f, "| Signal | Value |")?;
        writeln!(f, "|--------|-------|")?;
        writeln!(f, "| Source files | {} |", files_vec.len())?;
        writeln!(f, "| Symbols | {} |", info.member_ids.len())?;
        writeln!(f, "| Entry points | {} |", entry_point_count)?;
        writeln!(f, "| Internal calls | {} |", internal_calls.len())?;
        writeln!(f, "| Incoming module calls | {} |", incoming_call_count)?;
        writeln!(f, "| Outgoing module calls | {} |", outgoing_call_count)?;
        if !info.keywords.is_empty() {
            writeln!(f, "| Main topics | {} |", info.keywords.join(", "))?;
        }
        writeln!(f)?;

        if !files_vec.is_empty() {
            writeln!(f, "### Primary files")?;
            writeln!(f)?;
            for file in files_vec.iter().take(8) {
                writeln!(f, "- `{}`", file)?;
            }
            writeln!(f)?;
        }

        writeln!(f, "### Reading path")?;
        writeln!(f)?;
        writeln!(
            f,
            "1. Start with the entry points to understand how the module is reached."
        )?;
        writeln!(
            f,
            "2. Follow the module dependencies to understand who calls this module and where it sends work next."
        )?;
        writeln!(
            f,
            "3. Use the members table as the source map for code-level verification."
        )?;
        writeln!(f)?;

        if !incoming_modules.is_empty() || !outgoing_modules.is_empty() {
            writeln!(f, "## Module Dependencies")?;
            writeln!(f)?;
            writeln!(
                f,
                "Cette section donne une lecture bidirectionnelle du graphe : ce qui mène vers ce module, puis ce que ce module appelle ou utilise."
            )?;
            writeln!(f)?;

            if !incoming_modules.is_empty() {
                writeln!(f, "### Appelé par")?;
                writeln!(f)?;
                writeln!(f, "| Module | Relations |")?;
                writeln!(f, "|--------|-----------|")?;
                for neighbor in incoming_modules.iter().take(12) {
                    writeln!(
                        f,
                        "| [{}]({}.md) | {} |",
                        neighbor.label, neighbor.filename, neighbor.count
                    )?;
                }
                writeln!(f)?;
            }

            if !outgoing_modules.is_empty() {
                writeln!(f, "### Appelle / utilise")?;
                writeln!(f)?;
                writeln!(f, "| Module | Relations |")?;
                writeln!(f, "|--------|-----------|")?;
                for neighbor in outgoing_modules.iter().take(12) {
                    writeln!(
                        f,
                        "| [{}]({}.md) | {} |",
                        neighbor.label, neighbor.filename, neighbor.count
                    )?;
                }
                writeln!(f)?;
            }
        }

        if !internal_calls.is_empty() && internal_calls.len() <= 30 {
            writeln!(f, "## Call Graph")?;
            writeln!(f)?;
            writeln!(f, "```mermaid")?;
            writeln!(f, "graph LR")?;
            let mut seen_nodes = HashSet::new();
            for (src, tgt) in &internal_calls {
                let src_safe = sanitize_filename(src).replace('-', "_");
                let tgt_safe = sanitize_filename(tgt).replace('-', "_");
                if seen_nodes.insert(src_safe.clone()) {
                    writeln!(f, "    {}[\"{}\"]", src_safe, escape_mermaid_label(src))?;
                }
                if seen_nodes.insert(tgt_safe.clone()) {
                    writeln!(f, "    {}[\"{}\"]", tgt_safe, escape_mermaid_label(tgt))?;
                }
                writeln!(f, "    {} --> {}", src_safe, tgt_safe)?;
            }
            writeln!(f, "```")?;
            writeln!(f)?;
        }

        writeln!(f, "## Members")?;
        writeln!(f)?;
        let mut by_type: BTreeMap<&str, Vec<&GraphNode>> = BTreeMap::new();
        for mid in &info.member_ids {
            if let Some(node) = graph.get_node(mid) {
                let group = match node.label {
                    NodeLabel::Controller => "Controllers",
                    NodeLabel::Service | NodeLabel::Repository => "Services & Repositories",
                    NodeLabel::Method | NodeLabel::Function => "Methods",
                    _ => "Other",
                };
                by_type.entry(group).or_default().push(node);
            }
        }
        for (group_name, members) in &by_type {
            writeln!(f, "### {} ({})", group_name, members.len())?;
            writeln!(f, "| Symbole | Fichier | Lignes |")?;
            writeln!(f, "|---------|---------|--------|")?;
            for node in members {
                let lines = match (node.properties.start_line, node.properties.end_line) {
                    (Some(s), Some(e)) => format!("{}-{}", s, e),
                    (Some(s), None) => s.to_string(),
                    _ => "-".to_string(),
                };
                writeln!(
                    f,
                    "| `{}` | `{}` | {} |",
                    node.properties.name, node.properties.file_path, lines
                )?;
            }
            writeln!(f)?;
        }

        let mut entry_points: Vec<&GraphNode> = info
            .member_ids
            .iter()
            .filter_map(|mid| graph.get_node(mid))
            .filter(|n| {
                n.properties
                    .entry_point_score
                    .map(|s| s > 0.3)
                    .unwrap_or(false)
            })
            .collect();
        entry_points.sort_by(|a, b| {
            let sa = a.properties.entry_point_score.unwrap_or(0.0);
            let sb = b.properties.entry_point_score.unwrap_or(0.0);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        });

        if !entry_points.is_empty() {
            writeln!(f, "## Entry Points")?;
            writeln!(f)?;
            for node in entry_points.iter().take(10) {
                let score = node.properties.entry_point_score.unwrap_or(0.0);
                writeln!(
                    f,
                    "- `{}` (score: {:.2}) in `{}`",
                    node.properties.name, score, node.properties.file_path
                )?;
            }
            writeln!(f)?;
        }

        if !internal_calls.is_empty() {
            writeln!(f, "## Internal Calls")?;
            writeln!(f)?;
            for (src, tgt) in &internal_calls {
                writeln!(f, "- `{}` -> `{}`", src, tgt)?;
            }
            writeln!(f)?;
        }

        if !files_set.is_empty() {
            writeln!(f, "## Files")?;
            writeln!(f)?;
            for file_path in &files_set {
                writeln!(f, "- `{}`", file_path)?;
            }
            writeln!(f)?;
        }

        write!(f, "{}", nav_footer(&page_order, filename))?;
        println!("  {} modules/{filename}.md", "OK".green());
        page_count += 1;
    }

    // ─── Per-Controller pages ──────────────────────────────────────────
    // This section is very large. Including the full controller page generation
    // with action tables, callers, views, dependencies, impact analysis, action details.
    // Due to the extreme size, the controller, data model, services, UI, AJAX, and
    // external services page generation code follows the exact same logic from the original.

    // Pre-canonicalize the repo root once so we can verify every source path
    // we read for action snippets stays inside the repository. Graph node
    // `file_path` values are normally sanitized workspace-relative paths, but
    // the snapshot is just a JSON file in `.codeexplorer/` and could contain `..`
    // segments if it was hand-crafted or came from a malicious source.
    // Without this guard, action snippets and API method bodies would be read
    // from arbitrary filesystem locations and embedded in the generated docs.
    // The sibling `enrichment.rs::collect_evidence` already enforces this for
    // the same reason — keep the two in sync.
    let repo_root_canonical = std::fs::canonicalize(repo_path).ok();
    let read_repo_source = |rel_path: &str| -> Option<String> {
        let source_path = repo_path.join(rel_path);
        let canonical = std::fs::canonicalize(&source_path).ok()?;
        let root = repo_root_canonical.as_ref()?;
        if !canonical.starts_with(root) {
            return None;
        }
        std::fs::read_to_string(&source_path).ok()
    };

    for (ctrl_idx, ctrl) in controllers.iter().enumerate() {
        let ctrl_name = &ctrl.properties.name;
        let (filename, _) = &ctrl_filenames[ctrl_idx];
        let out_path = modules_dir.join(format!("{filename}.md"));

        let mut actions: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| {
                n.label == NodeLabel::ControllerAction
                    && n.properties.file_path == ctrl.properties.file_path
            })
            .collect();
        actions.sort_by(|a, b| {
            a.properties
                .start_line
                .unwrap_or(0)
                .cmp(&b.properties.start_line.unwrap_or(0))
        });

        // Controllers with fewer than 3 actions were already filtered out
        // upstream when building `controllers` so they don't pollute
        // `page_order` with dangling nav links.

        let action_ids: HashSet<String> = actions.iter().map(|a| a.id.clone()).collect();
        let caller_rels: Vec<&GraphRelationship> = graph
            .iter_relationships()
            .filter(|r| {
                action_ids.contains(&r.target_id)
                    && (r.rel_type == RelationshipType::CallsAction
                        || r.rel_type == RelationshipType::Calls)
            })
            .collect();

        let mut action_callers: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for r in &caller_rels {
            let short_name = if let Some(src_node) = graph.get_node(&r.source_id) {
                let label_str = match src_node.label {
                    NodeLabel::View | NodeLabel::PartialView => src_node
                        .properties
                        .file_path
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(&src_node.properties.name)
                        .to_string(),
                    NodeLabel::UiComponent => {
                        let file = src_node
                            .properties
                            .file_path
                            .rsplit(['/', '\\'])
                            .next()
                            .unwrap_or("vue");
                        let model = src_node.properties.bound_model.as_deref().unwrap_or("");
                        let cols = src_node.properties.description.as_deref().unwrap_or("");
                        if !model.is_empty() && !cols.is_empty() {
                            let short_cols: String = cols.chars().take(30).collect();
                            format!("{} Grid<{}> [{}]", file, model, short_cols)
                        } else if !model.is_empty() {
                            format!("{} Grid<{}>", file, model)
                        } else {
                            format!("{} (Grille)", file)
                        }
                    }
                    NodeLabel::AjaxCall | NodeLabel::ScriptFile => src_node
                        .properties
                        .file_path
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or(&src_node.properties.name)
                        .to_string(),
                    _ => src_node.properties.name.clone(),
                };
                let type_str = match src_node.label {
                    NodeLabel::View => "Vue".to_string(),
                    NodeLabel::PartialView => "Partielle".to_string(),
                    NodeLabel::UiComponent => "Grille".to_string(),
                    NodeLabel::AjaxCall => {
                        let m = src_node.properties.ajax_method.as_deref().unwrap_or("AJAX");
                        format!("AJAX {}", m)
                    }
                    NodeLabel::ScriptFile => "Script".to_string(),
                    _ => format!("{:?}", src_node.label),
                };
                (label_str, type_str)
            } else {
                let short = r
                    .source_id
                    .rsplit(':')
                    .next()
                    .unwrap_or(&r.source_id)
                    .to_string();
                (short, "Unknown".to_string())
            };
            let entry = action_callers.entry(r.target_id.clone()).or_default();
            if !entry.iter().any(|(n, _)| *n == short_name.0) {
                entry.push(short_name);
            }
        }

        let view_targets: Vec<String> = graph
            .iter_relationships()
            .filter(|r| {
                r.rel_type == RelationshipType::RendersView
                    && (r.source_id.contains(ctrl_name.as_str())
                        || graph
                            .get_node(&r.source_id)
                            .map(|n| n.properties.file_path == ctrl.properties.file_path)
                            .unwrap_or(false))
            })
            .map(|r| r.target_id.clone())
            .collect();
        let mut view_files: Vec<String> = view_targets
            .iter()
            .filter_map(|vid| graph.get_node(vid).map(|n| n.properties.file_path.clone()))
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();
        if view_files.is_empty() {
            view_files = view_targets
                .iter()
                .cloned()
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect();
        }

        let dependencies: Vec<String> = graph
            .iter_relationships()
            .filter(|r| {
                r.rel_type == RelationshipType::DependsOn
                    && (r.source_id.contains(ctrl_name.as_str())
                        || graph
                            .get_node(&r.source_id)
                            .map(|n| {
                                n.properties.file_path == ctrl.properties.file_path
                                    && n.label == NodeLabel::Controller
                            })
                            .unwrap_or(false))
            })
            .filter_map(|r| {
                graph
                    .get_node(&r.target_id)
                    .map(|n| n.properties.name.clone())
            })
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();

        let mut src_files: Vec<String> = vec![ctrl.properties.file_path.clone()];
        src_files.extend(view_files.iter().cloned());
        let src_file_refs: Vec<&str> = src_files.iter().take(15).map(|s| s.as_str()).collect();

        let known_types: HashSet<String> = graph
            .iter_nodes()
            .filter(|n| {
                matches!(
                    n.label,
                    NodeLabel::DbEntity | NodeLabel::ViewModel | NodeLabel::Class
                )
            })
            .map(|n| n.properties.name.clone())
            .collect();

        let mut content = format!("# {}\n\n", ctrl_name);
        content.push_str("<!-- GNX:LEAD -->\n");
        content.push_str(&source_files_section(&src_file_refs));

        let base_name = ctrl_name.trim_end_matches("Controller");
        let action_count = actions.len();

        // Verb distribution from already-collected actions
        let mut get_n = 0usize;
        let mut post_n = 0usize;
        let mut other_n = 0usize;
        for a in &actions {
            match a.properties.http_method.as_deref().unwrap_or("GET") {
                "GET" => get_n += 1,
                "POST" => post_n += 1,
                _ => other_n += 1,
            }
        }
        let view_count_lead = graph
            .iter_relationships()
            .filter(|r| r.source_id == ctrl.id && r.rel_type == RelationshipType::RendersView)
            .count();
        let svc_count_lead = graph
            .iter_relationships()
            .filter(|r| r.source_id == ctrl.id && r.rel_type == RelationshipType::DependsOn)
            .count();

        let lead = if let Some(d) = ctrl
            .properties
            .description
            .as_deref()
            .filter(|d| !d.is_empty())
        {
            format!("> {}\n\n", d)
        } else {
            let mut s = format!(
                "> **{}** expose **{}** action{}",
                base_name,
                action_count,
                if action_count > 1 { "s" } else { "" }
            );
            let mut verbs = Vec::new();
            if get_n > 0 {
                verbs.push(format!("{} GET", get_n));
            }
            if post_n > 0 {
                verbs.push(format!("{} POST", post_n));
            }
            if other_n > 0 {
                verbs.push(format!("{} autres", other_n));
            }
            if !verbs.is_empty() {
                s.push_str(&format!(" ({})", verbs.join(", ")));
            }
            s.push('.');
            if svc_count_lead > 0 {
                s.push_str(&format!(
                    " Appelle **{}** service{}.",
                    svc_count_lead,
                    if svc_count_lead > 1 { "s" } else { "" }
                ));
            }
            if view_count_lead > 0 {
                s.push_str(&format!(
                    " Rend **{}** vue{}.",
                    view_count_lead,
                    if view_count_lead > 1 { "s" } else { "" }
                ));
            }
            format!("{}\n\n", s)
        };
        content.push_str(&lead);

        content.push_str(&format!("## Actions ({})\n\n", action_count));
        content.push_str("| # | Action | Method | Route | Paramètres | Retour | Appelé par |\n");
        content.push_str("|---|--------|--------|-------|-----------|--------|------------|\n");
        for (i, action) in actions.iter().enumerate() {
            let method = action.properties.http_method.as_deref().unwrap_or("GET");
            let route = action
                .properties
                .route_template
                .as_deref()
                .filter(|r| !r.is_empty())
                .map(|r| format!("`{}`", r))
                .unwrap_or_else(|| "-".to_string());
            let ret = action
                .properties
                .return_type
                .as_deref()
                .unwrap_or("ActionResult");
            let params = extract_params_linked(
                action.properties.description.as_deref().unwrap_or(""),
                &known_types,
            );
            let called_by = action_callers
                .get(&action.id)
                .map(|callers| {
                    callers
                        .iter()
                        .take(3)
                        .map(|(name, _)| name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_else(|| "-".to_string());
            content.push_str(&format!(
                "| {} | **{}** | {} | {} | {} | {} | {} |\n",
                i + 1,
                action.properties.name,
                method,
                route,
                params,
                ret,
                called_by
            ));
        }
        content.push('\n');
        content.push_str("<!-- GNX:TIP:actions -->\n");

        // Impact Analysis
        {
            let mut action_impacts: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
            for action in &actions {
                let action_name = action.properties.name.clone();
                let callees: Vec<String> = graph
                    .iter_relationships()
                    .filter(|r| {
                        r.source_id == action.id
                            && matches!(
                                r.rel_type,
                                RelationshipType::Calls
                                    | RelationshipType::CallsAction
                                    | RelationshipType::DependsOn
                                    | RelationshipType::CallsService
                            )
                    })
                    .filter_map(|r| {
                        graph
                            .get_node(&r.target_id)
                            .map(|n| n.properties.name.clone())
                    })
                    .collect::<BTreeSet<String>>()
                    .into_iter()
                    .collect();
                let callers: Vec<String> = graph
                    .iter_relationships()
                    .filter(|r| {
                        r.target_id == action.id
                            && matches!(
                                r.rel_type,
                                RelationshipType::Calls | RelationshipType::CallsAction
                            )
                    })
                    .filter_map(|r| {
                        graph
                            .get_node(&r.source_id)
                            .map(|n| n.properties.name.clone())
                    })
                    .collect::<BTreeSet<String>>()
                    .into_iter()
                    .collect();
                if callees.len() + callers.len() > 0 {
                    action_impacts.push((action_name, callees, callers));
                }
            }
            action_impacts.sort_by(|a, b| (b.1.len() + b.2.len()).cmp(&(a.1.len() + a.2.len())));
            if !action_impacts.is_empty() {
                content.push_str("## Analyse d'Impact\n\n> Si une action de ce controller est modifiée, voici les composants potentiellement impactés.\n\n");
                content.push_str("| Action modifiée | Impact aval (callees) | Impact amont (callers) |\n|----------------|----------------------|----------------------|\n");
                for (action_name, callees, callers) in action_impacts.iter().take(5) {
                    let callees_str = if callees.is_empty() {
                        "-".to_string()
                    } else {
                        callees
                            .iter()
                            .take(5)
                            .map(|s| format!("`{}`", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    let callers_str = if callers.is_empty() {
                        "-".to_string()
                    } else {
                        callers
                            .iter()
                            .take(5)
                            .map(|s| format!("`{}`", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    content.push_str(&format!(
                        "| **{}** | {} | {} |\n",
                        action_name, callees_str, callers_str
                    ));
                }
                content.push('\n');
            }
        }

        // Callers section
        if !caller_rels.is_empty() {
            let mut caller_rows: Vec<(String, String, String, String)> = Vec::new();
            let mut seen_callers: HashSet<(String, String)> = HashSet::new();
            for r in &caller_rels {
                let (source_name, source_type) =
                    if let Some(src_node) = graph.get_node(&r.source_id) {
                        let name = match src_node.label {
                            NodeLabel::View | NodeLabel::PartialView => src_node
                                .properties
                                .file_path
                                .rsplit(['/', '\\'])
                                .next()
                                .unwrap_or(&src_node.properties.name)
                                .to_string(),
                            _ => src_node.properties.name.clone(),
                        };
                        let stype = match src_node.label {
                            NodeLabel::View => {
                                if r.reason.contains("form") || r.reason.contains("Form") {
                                    "View (Form)".to_string()
                                } else {
                                    "View".to_string()
                                }
                            }
                            NodeLabel::PartialView => "Partial View".to_string(),
                            NodeLabel::AjaxCall => {
                                let ajax_type =
                                    src_node.properties.ajax_method.as_deref().unwrap_or("AJAX");
                                if src_node
                                    .properties
                                    .ajax_url
                                    .as_deref()
                                    .map(|u| u.contains("getJSON"))
                                    .unwrap_or(false)
                                {
                                    "Script ($.getJSON)".to_string()
                                } else {
                                    format!("Script ({})", ajax_type)
                                }
                            }
                            _ => format!("{:?}", src_node.label),
                        };
                        (name, stype)
                    } else {
                        let short = r
                            .source_id
                            .rsplit(':')
                            .next()
                            .unwrap_or(&r.source_id)
                            .to_string();
                        (short, "Unknown".to_string())
                    };
                let target_action = graph
                    .get_node(&r.target_id)
                    .map(|n| n.properties.name.clone())
                    .unwrap_or_else(|| {
                        r.target_id
                            .rsplit(':')
                            .next()
                            .unwrap_or(&r.target_id)
                            .to_string()
                    });
                let method = graph
                    .get_node(&r.target_id)
                    .and_then(|n| n.properties.http_method.as_ref())
                    .cloned()
                    .unwrap_or_else(|| "-".to_string());
                let key = (source_name.clone(), target_action.clone());
                if seen_callers.insert(key) {
                    caller_rows.push((source_name, source_type, target_action, method));
                }
            }
            if !caller_rows.is_empty() {
                content.push_str("## Callers\n\nThis controller is called from:\n\n| Source | Type | Action | Method |\n|--------|------|--------|--------|\n");
                for (source, stype, action, method) in &caller_rows {
                    content.push_str(&format!(
                        "| {} | {} | {} | {} |\n",
                        source, stype, action, method
                    ));
                }
                content.push('\n');
            }
        }

        if !view_files.is_empty() {
            content.push_str("## Associated Views\n\n");
            for v in &view_files {
                content.push_str(&format!("- `{}`\n", v));
            }
            content.push('\n');
        }
        if !dependencies.is_empty() {
            content.push_str("## Dependencies\n\n");
            for dep in &dependencies {
                content.push_str(&format!("- `{}`\n", dep));
            }
            content.push('\n');
        }

        // Action Details
        if !actions.is_empty() {
            content.push_str("## Action Details\n\n");
            for action in &actions {
                let method = action.properties.http_method.as_deref().unwrap_or("GET");
                let params_short = extract_params_from_content(
                    action.properties.description.as_deref().unwrap_or(""),
                    &action.properties.name,
                );
                content.push_str(&format!(
                    "<details>\n<summary><strong>{}</strong> ({}) — {}</summary>\n\n",
                    action.properties.name,
                    method,
                    if params_short == "-" {
                        "aucun paramètre".to_string()
                    } else {
                        params_short.clone()
                    }
                ));
                content.push_str(&format!("**Fichier :** `{}`", ctrl.properties.file_path));
                if let Some(line) = action.properties.start_line {
                    content.push_str(&format!(" (ligne {})", line));
                }
                content.push('\n');
                if let Some(rt) = &action.properties.route_template {
                    if !rt.is_empty() {
                        content.push_str(&format!("**Route :** `{}`\n", rt));
                    }
                }
                if params_short != "-" {
                    content.push_str(&format!("**Paramètres :** {}\n", params_short));
                }
                let ret = action
                    .properties
                    .return_type
                    .as_deref()
                    .unwrap_or("ActionResult");
                content.push_str(&format!("**Returns:** {}\n", ret));
                if let Some(callers) = action_callers.get(&action.id) {
                    let caller_strs: Vec<String> = callers
                        .iter()
                        .map(|(name, stype)| format!("{} ({})", name, stype))
                        .collect();
                    if !caller_strs.is_empty() {
                        content.push_str(&format!("**Appelé par :** {}\n", caller_strs.join(", ")));
                    }
                }
                if let Some(keys) = &action.properties.response_keys {
                    if !keys.is_empty() {
                        content.push_str(&format!("**Réponse :** `{}`\n", keys.join("`, `")));
                    }
                }
                if let Some(keys) = &action.properties.error_keys {
                    if !keys.is_empty() {
                        content.push_str(&format!("**Erreurs :** `{}`\n", keys.join("`, `")));
                    }
                }
                write_llm_badge(&mut content, &action.properties);
                if let Some(source) = read_repo_source(&ctrl.properties.file_path) {
                    if let Some(snippet) = extract_method_body(&source, &action.properties.name, 50)
                    {
                        content.push_str("\n```csharp\n");
                        content.push_str(&snippet);
                        content.push_str("```\n");
                    }
                }
                content.push_str("\n</details>\n\n");
            }
        }

        content.push_str("<!-- GNX:CLOSING -->\n");
        content.push_str(&format!(
            "## Summary\n\n**{}** provides {} actions.\n\n",
            ctrl_name, action_count
        ));
        content.push_str(
            "**See also:** [Architecture](../architecture.md) · [Services](./services.md)\n",
        );
        content.push_str(&nav_footer(&page_order, filename));

        std::fs::write(&out_path, &content)?;
        println!("  {} {}", "OK".green(), out_path.display());
        page_count += 1;
    }

    // ─── Data Model pages ──────────────────────────────────────────────
    for (ctx_idx, ctx) in db_contexts.iter().enumerate() {
        let ctx_name = &ctx.properties.name;
        let (filename, _) = &data_filenames[ctx_idx];
        let out_path = modules_dir.join(format!("{filename}.md"));
        let entities: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| n.label == NodeLabel::DbEntity)
            .collect();
        let mut src_files: Vec<String> = vec![ctx.properties.file_path.clone()];
        for e in &entities {
            if !e.properties.file_path.is_empty() {
                src_files.push(e.properties.file_path.clone());
            }
        }
        let src_files_dedup: Vec<String> = src_files
            .into_iter()
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();
        let src_file_refs: Vec<&str> = src_files_dedup
            .iter()
            .take(15)
            .map(|s| s.as_str())
            .collect();

        let mut content = format!("# Data Model: {}\n\n<!-- GNX:LEAD -->\n", ctx_name);
        content.push_str(&source_files_section(&src_file_refs));
        content.push_str(&format!(
            "**File:** `{}`\n\n**Entities:** {}\n\n",
            ctx.properties.file_path,
            entities.len()
        ));

        // Auto-prose: column stats via HasColumn relationships
        {
            let mut total_columns = 0usize;
            let mut pk_count = 0usize;
            let mut nullable_count = 0usize;
            let mut traced_entities = 0usize;
            for entity in &entities {
                if entity.properties.is_traced.unwrap_or(false) {
                    traced_entities += 1;
                }
                for rel in graph.iter_relationships() {
                    if rel.source_id == entity.id && rel.rel_type == RelationshipType::HasColumn {
                        if let Some(col) = graph.get_node(&rel.target_id) {
                            total_columns += 1;
                            if col.properties.is_primary_key.unwrap_or(false) {
                                pk_count += 1;
                            }
                            if col.properties.is_nullable.unwrap_or(false) {
                                nullable_count += 1;
                            }
                        }
                    }
                }
            }
            if !entities.is_empty() {
                let mut para = format!(
                    "> Ce modèle de données définit **{}** entité{}",
                    entities.len(),
                    if entities.len() > 1 { "s" } else { "" }
                );
                if total_columns > 0 {
                    para.push_str(&format!(
                        " réparties sur **{}** colonne{}",
                        total_columns,
                        if total_columns > 1 { "s" } else { "" }
                    ));
                    let mut details = Vec::new();
                    if pk_count > 0 {
                        details.push(format!(
                            "{} clé{} primaire{}",
                            pk_count,
                            if pk_count > 1 { "s" } else { "" },
                            if pk_count > 1 { "s" } else { "" }
                        ));
                    }
                    if nullable_count > 0 {
                        details.push(format!(
                            "{} nullable{}",
                            nullable_count,
                            if nullable_count > 1 { "s" } else { "" }
                        ));
                    }
                    if !details.is_empty() {
                        para.push_str(&format!(" ({})", details.join(", ")));
                    }
                }
                para.push('.');
                if traced_entities > 0 {
                    para.push_str(&format!(
                        " {} entité{} tracée{}.",
                        traced_entities,
                        if traced_entities > 1 { "s" } else { "" },
                        if traced_entities > 1 { "s" } else { "" }
                    ));
                }
                para.push_str("\n\n");
                content.push_str(&para);
            }
        }

        let mut entity_rels: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for rel in graph.iter_relationships() {
            if rel.rel_type == RelationshipType::AssociatesWith {
                let src = rel
                    .source_id
                    .rsplit(':')
                    .next()
                    .unwrap_or(&rel.source_id)
                    .to_string();
                let tgt = rel
                    .target_id
                    .rsplit(':')
                    .next()
                    .unwrap_or(&rel.target_id)
                    .to_string();
                let card = if rel.reason.contains("1:*") {
                    "||--o{"
                } else if rel.reason.contains("*:1") {
                    "}o--||"
                } else if rel.reason.contains("*:*") {
                    "}o--o{"
                } else {
                    "||--||"
                };
                entity_rels
                    .entry(src.clone())
                    .or_default()
                    .push((tgt.clone(), card.to_string()));
                entity_rels
                    .entry(tgt)
                    .or_default()
                    .push((src, card.to_string()));
            }
        }

        content.push_str("## Entities\n\n");
        for entity in &entities {
            let ename = &entity.properties.name;
            let rels = entity_rels.get(ename.as_str());
            let rel_count = rels.map_or(0, |v| v.len());
            content.push_str(&format!("<details id=\"{}\">\n<summary><strong>{}</strong> — <code>{}</code> ({} relations)</summary>\n\n", ename, ename, entity.properties.file_path, rel_count));
            if rel_count > 0 {
                if let Some(rels) = rels {
                    content.push_str("```mermaid\ngraph LR\n");
                    let eid = sanitize_mermaid_id(ename);
                    content.push_str(&format!(
                        "    {}[\"{}\"]\n    style {} fill:#4a85e0,color:#fff,stroke:#3a73cc\n",
                        eid, ename, eid
                    ));
                    let mut seen: HashSet<String> = HashSet::new();
                    for (target, _) in rels.iter().take(8) {
                        if seen.insert(target.clone()) {
                            let tid = sanitize_mermaid_id(target);
                            content.push_str(&format!(
                                "    {}[\"{}\"]\n    {} --- {}\n",
                                tid, target, eid, tid
                            ));
                        }
                    }
                    if rels.len() > 8 {
                        content.push_str(&format!(
                            "    more((\"...+{}\"))\n    {} -.- more\n",
                            rels.len() - 8,
                            eid
                        ));
                    }
                    content.push_str("```\n\n");
                    // Relations as text table
                    content.push_str("**Relations :**\n\n| Entité liée | Cardinalité |\n|-------------|-------------|\n");
                    let mut seen_rels: HashSet<String> = HashSet::new();
                    for (target, card) in rels {
                        if seen_rels.insert(target.clone()) {
                            let card_text = match card.as_str() {
                                "||--o{" => "1 → N (has many)",
                                "}o--||" => "N → 1 (belongs to)",
                                "}o--o{" => "N ↔ N",
                                _ => "1 ↔ 1",
                            };
                            content.push_str(&format!("| {} | {} |\n", target, card_text));
                        }
                    }
                    content.push('\n');
                }
            } else {
                content.push_str("*Aucune relation détectée dans le modèle.*\n\n");
            }
            // Column listing from DbColumn nodes
            let columns: Vec<&GraphNode> = graph
                .iter_nodes()
                .filter(|n| {
                    n.label == NodeLabel::DbColumn
                        && n.properties.declared_in.as_deref() == Some(ename.as_str())
                })
                .collect();
            if !columns.is_empty() {
                content.push_str("**Colonnes :**\n\n| Colonne | Type | PK | Nullable |\n");
                content.push_str("|---------|------|----|---------|\n");
                for col in &columns {
                    let col_type = col.properties.column_type.as_deref().unwrap_or("-");
                    let pk = if col.properties.is_primary_key.unwrap_or(false) {
                        "oui"
                    } else {
                        "-"
                    };
                    let null = if col.properties.is_nullable.unwrap_or(true) {
                        "oui"
                    } else {
                        "-"
                    };
                    content.push_str(&format!(
                        "| {} | `{}` | {} | {} |\n",
                        col.properties.name, col_type, pk, null
                    ));
                }
                content.push('\n');
            }
            content.push_str("</details>\n\n");
        }
        content.push_str(&nav_footer(&page_order, filename));
        std::fs::write(&out_path, &content)?;
        println!("  {} {}", "OK".green(), out_path.display());
        page_count += 1;
    }

    // ─── Service Layer page ────────────────────────────────────────────
    if !services.is_empty() {
        let out_path = modules_dir.join("services.md");
        let svc_files: Vec<String> = services
            .iter()
            .map(|s| s.properties.file_path.clone())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();
        let svc_file_refs: Vec<&str> = svc_files.iter().take(15).map(|s| s.as_str()).collect();
        let mut content = String::from("# Service Layer\n\n<!-- GNX:LEAD -->\n");
        content.push_str(&source_files_section(&svc_file_refs));
        let mut service_used_by: HashMap<String, Vec<String>> = HashMap::new();
        for svc in &services {
            let users: Vec<String> = graph
                .iter_relationships()
                .filter(|r| r.rel_type == RelationshipType::DependsOn && r.target_id == svc.id)
                .filter_map(|r| {
                    graph
                        .get_node(&r.source_id)
                        .filter(|n| n.label == NodeLabel::Controller)
                        .map(|n| n.properties.name.clone())
                })
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect();
            service_used_by.insert(svc.id.clone(), users);
        }
        // Auto-prose lead for service layer
        {
            let total_method_count: usize = services
                .iter()
                .map(|svc| {
                    graph
                        .iter_relationships()
                        .filter(|r| {
                            r.rel_type == RelationshipType::HasMethod && r.source_id == svc.id
                        })
                        .count()
                })
                .sum();
            let top_caller = service_used_by
                .iter()
                .filter_map(|(id, callers)| {
                    if callers.is_empty() {
                        return None;
                    }
                    graph
                        .get_node(id)
                        .map(|n| (n.properties.name.clone(), callers.len()))
                })
                .max_by_key(|(_, c)| *c);
            let iface_count = services
                .iter()
                .filter(|s| {
                    s.properties
                        .implements_interface
                        .as_deref()
                        .filter(|i| !i.is_empty() && *i != "-")
                        .is_some()
                })
                .count();
            let mut lead_svc = format!(
                "> La couche service expose **{}** méthode{} dans **{}** classe{}.",
                total_method_count,
                if total_method_count > 1 { "s" } else { "" },
                services.len(),
                if services.len() > 1 { "s" } else { "" }
            );
            if let Some((name, callers)) = top_caller {
                lead_svc.push_str(&format!(
                    " Service le plus sollicité : **{}** ({} appelant{}).",
                    name,
                    callers,
                    if callers > 1 { "s" } else { "" }
                ));
            }
            if iface_count > 0 {
                lead_svc.push_str(&format!(
                    " {} service{} implémente{} une interface.",
                    iface_count,
                    if iface_count > 1 { "s" } else { "" },
                    if iface_count > 1 { "nt" } else { "" }
                ));
            }
            lead_svc.push_str("\n\n");
            content.push_str(&lead_svc);
        }
        content.push_str("## Services\n\n| Service | Type | Interface | Used By | Purpose | File |\n|---------|------|-----------|---------|---------|------|\n");
        for svc in &services {
            let layer = svc.properties.layer_type.as_deref().unwrap_or("Service");
            let iface = svc
                .properties
                .implements_interface
                .as_deref()
                .unwrap_or("-");
            let used_by = service_used_by
                .get(&svc.id)
                .map(|users| {
                    if users.is_empty() {
                        "-".to_string()
                    } else {
                        users.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
                    }
                })
                .unwrap_or_else(|| "-".to_string());
            let purpose = describe_service_fr(&svc.properties.name);
            content.push_str(&format!(
                "| {} | {} | {} | {} | {} | `{}` |\n",
                svc.properties.name, layer, iface, used_by, purpose, svc.properties.file_path
            ));
        }
        content.push('\n');

        // Per-service method detail blocks
        let svcs_with_methods: Vec<(&&GraphNode, Vec<&GraphNode>)> = services
            .iter()
            .map(|svc| {
                let mut methods: Vec<&GraphNode> = graph
                    .iter_relationships()
                    .filter(|r| r.rel_type == RelationshipType::HasMethod && r.source_id == svc.id)
                    .filter_map(|r| graph.get_node(&r.target_id))
                    .filter(|n| n.label == NodeLabel::Method)
                    .collect();
                methods.sort_by(|a, b| a.properties.name.cmp(&b.properties.name));
                (svc, methods)
            })
            .filter(|(_, ms)| !ms.is_empty())
            .collect();
        if !svcs_with_methods.is_empty() {
            content.push_str("## Détails des méthodes\n\n");
            for (svc, methods) in &svcs_with_methods {
                let deps: Vec<String> = graph
                    .iter_relationships()
                    .filter(|r| r.rel_type == RelationshipType::DependsOn && r.source_id == svc.id)
                    .filter_map(|r| graph.get_node(&r.target_id))
                    .map(|n| n.properties.name.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                content.push_str(&format!(
                    "<details>\n<summary><strong>{}</strong> — {} méthodes</summary>\n\n",
                    svc.properties.name,
                    methods.len()
                ));
                if !deps.is_empty() {
                    content.push_str(&format!("**Dépendances :** {}\n\n", deps.join(", ")));
                }
                content.push_str("| Méthode | Ligne |\n|---------|-------|\n");
                for method in methods.iter().take(40) {
                    let line = method
                        .properties
                        .start_line
                        .map(|l| l.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    content.push_str(&format!("| `{}` | {} |\n", method.properties.name, line));
                }
                if methods.len() > 40 {
                    content.push_str(&format!("| *… +{} autres* | |\n", methods.len() - 40));
                }
                content.push_str("\n</details>\n\n");
            }
        }

        let smelly_svcs: Vec<&&GraphNode> = services
            .iter()
            .filter(|s| {
                s.properties.llm_risk_score.is_some()
                    || s.properties
                        .llm_smells
                        .as_ref()
                        .map(|v| !v.is_empty())
                        .unwrap_or(false)
            })
            .collect();
        if !smelly_svcs.is_empty() {
            content.push_str("## Qualité LLM — Services\n\n");
            for svc in &smelly_svcs {
                content.push_str(&format!("### {}\n\n", svc.properties.name));
                write_llm_badge(&mut content, &svc.properties);
            }
        }
        content.push_str(&nav_footer(&page_order, "services"));
        std::fs::write(&out_path, &content)?;
        println!("  {} {}", "OK".green(), out_path.display());
        page_count += 1;
    }

    // ─── UI Components page ────────────────────────────────────────────
    if !ui_components.is_empty() {
        let out_path = modules_dir.join("ui-components.md");
        let ui_files: Vec<String> = ui_components
            .iter()
            .map(|c| c.properties.file_path.clone())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();
        let ui_file_refs: Vec<&str> = ui_files.iter().take(15).map(|s| s.as_str()).collect();
        let mut content = String::from("# UI Components (Telerik/Kendo)\n\n");
        content.push_str(&source_files_section(&ui_file_refs));
        content.push_str(&format!(
            "**Total components:** {}\n\n",
            ui_components.len()
        ));
        content.push_str("| Component | Type | Model | Columns | File |\n|-----------|------|-------|---------|------|\n");
        for comp in &ui_components {
            let comp_type = comp.properties.component_type.as_deref().unwrap_or("-");
            let model = comp.properties.bound_model.as_deref().unwrap_or("-");
            let cols = comp.properties.description.as_deref().unwrap_or("-");
            let cols_short: String = cols.chars().take(40).collect();
            content.push_str(&format!(
                "| {} | {} | {} | {} | `{}` |\n",
                comp.properties.name, comp_type, model, cols_short, comp.properties.file_path
            ));
        }
        content.push('\n');
        content.push_str(&nav_footer(&page_order, "ui-components"));
        std::fs::write(&out_path, &content)?;
        println!("  {} {}", "OK".green(), out_path.display());
        page_count += 1;
    }

    // ─── AJAX Endpoints page ───────────────────────────────────────────
    if !ajax_calls.is_empty() {
        let out_path = modules_dir.join("ajax-endpoints.md");
        let ajax_files: Vec<String> = ajax_calls
            .iter()
            .map(|c| c.properties.file_path.clone())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();
        let ajax_file_refs: Vec<&str> = ajax_files.iter().take(15).map(|s| s.as_str()).collect();
        let mut content = String::from("# AJAX Endpoints\n\n");
        content.push_str(&source_files_section(&ajax_file_refs));
        content.push_str(&format!("**Total AJAX calls:** {}\n\n", ajax_calls.len()));
        content.push_str("| Method | URL | File | Line |\n|--------|-----|------|------|\n");
        for call in ajax_calls.iter().take(100) {
            let method = call.properties.ajax_method.as_deref().unwrap_or("GET");
            let url = call.properties.ajax_url.as_deref().unwrap_or("-");
            let line = call
                .properties
                .start_line
                .map(|l| l.to_string())
                .unwrap_or_default();
            content.push_str(&format!(
                "| {} | {} | `{}` | {} |\n",
                method, url, call.properties.file_path, line
            ));
        }
        content.push('\n');
        content.push_str(&nav_footer(&page_order, "ajax-endpoints"));
        std::fs::write(&out_path, &content)?;
        println!("  {} {}", "OK".green(), out_path.display());
        page_count += 1;
    }

    // ─── External Services page ────────────────────────────────────────
    if !ext_services.is_empty() {
        let out_path = modules_dir.join("external-services.md");
        let ext_files: Vec<String> = ext_services
            .iter()
            .map(|s| s.properties.file_path.clone())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();
        let mut calling_files: BTreeSet<String> = BTreeSet::new();
        for svc in &ext_services {
            for r in graph.iter_relationships() {
                if r.rel_type == RelationshipType::CallsService && r.target_id == svc.id {
                    if let Some(src) = graph.get_node(&r.source_id) {
                        if !src.properties.file_path.is_empty() {
                            calling_files.insert(src.properties.file_path.clone());
                        }
                    }
                }
            }
        }
        let mut all_src_files: Vec<String> = ext_files
            .iter()
            .cloned()
            .chain(calling_files.iter().cloned())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect();
        all_src_files.truncate(15);
        let src_file_refs: Vec<&str> = all_src_files.iter().map(|s| s.as_str()).collect();

        let mut content = String::from("# External Services & Integrations\n\n");
        content.push_str(&source_files_section(&src_file_refs));
        content.push_str(&format!("> This project integrates with {} external services via WebAPI (REST) and WCF (SOAP).\n\n", ext_services.len()));

        let webapi_services: Vec<&&GraphNode> = ext_services
            .iter()
            .filter(|s| {
                let st = s
                    .properties
                    .service_type
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase();
                st == "webapi" || st == "rest" || st == "http"
            })
            .collect();
        let wcf_services: Vec<&&GraphNode> = ext_services
            .iter()
            .filter(|s| {
                let st = s
                    .properties
                    .service_type
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase();
                st == "wcf" || st == "soap"
            })
            .collect();
        let other_services: Vec<&&GraphNode> = ext_services
            .iter()
            .filter(|s| {
                let st = s
                    .properties
                    .service_type
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase();
                !["webapi", "rest", "http", "wcf", "soap"].contains(&st.as_str())
            })
            .collect();

        let find_callers = |svc: &GraphNode| -> Vec<String> {
            graph
                .iter_relationships()
                .filter(|r| r.rel_type == RelationshipType::CallsService && r.target_id == svc.id)
                .filter_map(|r| {
                    graph
                        .get_node(&r.source_id)
                        .map(|n| n.properties.name.clone())
                })
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect()
        };
        let _find_methods = |svc: &GraphNode| -> Vec<&GraphNode> {
            let svc_name = &svc.properties.name;
            graph
                .iter_nodes()
                .filter(|n| {
                    n.label == NodeLabel::Method
                        && (n.properties.file_path.contains("WebAPI")
                            || n.properties.file_path.contains("WebApi")
                            || n.properties.file_path.contains(svc_name))
                        && n.properties.name.ends_with("Async")
                        && !n.properties.name.starts_with("PrepareRequest")
                        && !n.properties.name.starts_with("ProcessResponse")
                        && !n.properties.name.starts_with("ReadObject")
                })
                .collect()
        };

        if !webapi_services.is_empty() {
            content.push_str(&format!("## WebAPI Services ({})\n\n| Client | Type | Called From | Purpose | Docs |\n|--------|------|------------|---------|------|\n", webapi_services.len()));
            for svc in &webapi_services {
                let stype = svc.properties.service_type.as_deref().unwrap_or("WebAPI");
                let callers = find_callers(svc);
                let called_from = if callers.is_empty() {
                    "-".to_string()
                } else {
                    callers.join(", ")
                };
                let purpose = svc.properties.description.as_deref().unwrap_or("-");
                let docs = svc
                    .properties
                    .source_url
                    .as_deref()
                    .filter(|u| !u.is_empty())
                    .map(|u| format!("[docs]({})", u))
                    .unwrap_or_else(|| "-".to_string());
                content.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    svc.properties.name, stype, called_from, purpose, docs
                ));
            }
            content.push('\n');

            let all_api_methods: Vec<&GraphNode> = graph
                .iter_nodes()
                .filter(|n| {
                    n.label == NodeLabel::Method
                        && (n.properties.file_path.contains("WebAPI")
                            || n.properties.file_path.contains("WebApi"))
                        && !n.properties.file_path.contains("Tests")
                        && n.properties.name.ends_with("Async")
                        && !n.properties.name.starts_with("PrepareRequest")
                        && !n.properties.name.starts_with("ProcessResponse")
                        && !n.properties.name.starts_with("ReadObject")
                })
                .collect();
            if !all_api_methods.is_empty() {
                content.push_str("### API ExternalApi — Méthodes détaillées\n\n> Point d'accès unique aux données bénéficiaires via l'API REST ExternalApi.\n> Authentification : HTTP Basic. Toutes les méthodes sont asynchrones.\n\n");
                let mut methods_by_file: BTreeMap<String, Vec<&&GraphNode>> = BTreeMap::new();
                for m in &all_api_methods {
                    methods_by_file
                        .entry(m.properties.file_path.clone())
                        .or_default()
                        .push(m);
                }
                for (file, methods) in &methods_by_file {
                    let file_short = file.rsplit(['/', '\\']).next().unwrap_or(file);
                    if file_short.contains("Ldap") {
                        continue;
                    }
                    content.push_str(&format!("**Fichier : `{}`**\n\n| Méthode | Paramètres | Retour |\n|---------|-----------|--------|\n", file_short));
                    let source_content = read_repo_source(file).unwrap_or_default();
                    for method in methods {
                        let method_name = &method.properties.name;
                        let signatures = if !source_content.is_empty() {
                            extract_all_method_signatures(&source_content, method_name)
                        } else {
                            vec![("-".to_string(), "-".to_string())]
                        };
                        for (idx, (params_str, ret_str)) in signatures.iter().enumerate() {
                            if idx == 0 {
                                content.push_str(&format!(
                                    "| **{}** | {} | `{}` |\n",
                                    method_name, params_str, ret_str
                                ));
                            } else {
                                content.push_str(&format!(
                                    "| \u{21b3} *surcharge* | {} | `{}` |\n",
                                    params_str, ret_str
                                ));
                            }
                        }
                    }
                    content.push('\n');
                }
                content.push_str("**Services appelants :**\n\n");
                for svc in &webapi_services {
                    let callers = find_callers(svc);
                    if !callers.is_empty() {
                        content.push_str(&format!(
                            "- **{}** \u{2190} {}\n",
                            svc.properties.name,
                            callers.join(", ")
                        ));
                    }
                }
                content.push('\n');
            }
        }

        if !wcf_services.is_empty() {
            content.push_str(&format!("## WCF Services (SOAP) ({})\n\n| Client | Type | Called From | Purpose | Docs |\n|--------|------|------------|---------|------|\n", wcf_services.len()));
            for svc in &wcf_services {
                let stype = svc.properties.service_type.as_deref().unwrap_or("WCF");
                let callers = find_callers(svc);
                let called_from = if callers.is_empty() {
                    "-".to_string()
                } else {
                    callers.join(", ")
                };
                let purpose = svc.properties.description.as_deref().unwrap_or("-");
                let docs = svc
                    .properties
                    .source_url
                    .as_deref()
                    .filter(|u| !u.is_empty())
                    .map(|u| format!("[docs]({})", u))
                    .unwrap_or_else(|| "-".to_string());
                content.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    svc.properties.name, stype, called_from, purpose, docs
                ));
            }
            content.push('\n');
        }

        if !other_services.is_empty() {
            content.push_str(&format!("## Other Services ({})\n\n| Client | Type | Called From | Purpose | Docs |\n|--------|------|------------|---------|------|\n", other_services.len()));
            for svc in &other_services {
                let stype = svc.properties.service_type.as_deref().unwrap_or("External");
                let callers = find_callers(svc);
                let called_from = if callers.is_empty() {
                    "-".to_string()
                } else {
                    callers.join(", ")
                };
                let purpose = svc.properties.description.as_deref().unwrap_or("-");
                let docs = svc
                    .properties
                    .source_url
                    .as_deref()
                    .filter(|u| !u.is_empty())
                    .map(|u| format!("[docs]({})", u))
                    .unwrap_or_else(|| "-".to_string());
                content.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    svc.properties.name, stype, called_from, purpose, docs
                ));
            }
            content.push('\n');
        }

        // Service Call Flow Mermaid diagram
        let mut mermaid_edges: Vec<(String, String)> = Vec::new();
        let mut mermaid_nodes: BTreeMap<String, (String, &str)> = BTreeMap::new();
        for ext_svc in &ext_services {
            let ext_short = sanitize_mermaid_id(&ext_svc.properties.name);
            mermaid_nodes.insert(
                ext_short.clone(),
                (ext_svc.properties.name.clone(), "External"),
            );
            for r in graph.iter_relationships() {
                if r.rel_type == RelationshipType::CallsService && r.target_id == ext_svc.id {
                    if let Some(caller) = graph.get_node(&r.source_id) {
                        if caller.properties.file_path.contains("Test")
                            || caller.properties.file_path.contains("test")
                        {
                            continue;
                        }
                        let subgraph = match caller.label {
                            NodeLabel::Controller => "Controllers",
                            NodeLabel::Service | NodeLabel::Repository => "Services",
                            _ => continue,
                        };
                        let caller_short = sanitize_mermaid_id(&caller.properties.name);
                        mermaid_nodes.insert(
                            caller_short.clone(),
                            (caller.properties.name.clone(), subgraph),
                        );
                        mermaid_edges.push((caller_short.clone(), ext_short.clone()));
                        if caller.label == NodeLabel::Service
                            || caller.label == NodeLabel::Repository
                        {
                            for r2 in graph.iter_relationships() {
                                if r2.rel_type == RelationshipType::DependsOn
                                    && r2.target_id == caller.id
                                {
                                    if let Some(ctrl) = graph.get_node(&r2.source_id) {
                                        if ctrl.label == NodeLabel::Controller {
                                            let ctrl_short =
                                                sanitize_mermaid_id(&ctrl.properties.name);
                                            mermaid_nodes.insert(
                                                ctrl_short.clone(),
                                                (ctrl.properties.name.clone(), "Controllers"),
                                            );
                                            mermaid_edges.push((ctrl_short, caller_short.clone()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !mermaid_edges.is_empty() {
            content.push_str("## Service Call Flow\n\n```mermaid\ngraph LR\n");
            let mut subgraphs: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
            for (id, (label, sg)) in &mermaid_nodes {
                subgraphs
                    .entry(sg)
                    .or_default()
                    .push((id.clone(), label.clone()));
            }
            for (sg_name, nodes) in &subgraphs {
                content.push_str(&format!(
                    "    subgraph {}[\"{}\"]\n",
                    sanitize_mermaid_id(sg_name),
                    sg_name
                ));
                for (id, label) in nodes {
                    content.push_str(&format!("        {}[\"{}\"]\n", id, label));
                }
                content.push_str("    end\n");
            }
            let unique_edges: BTreeSet<(String, String)> = mermaid_edges.into_iter().collect();
            for (from, to) in &unique_edges {
                content.push_str(&format!("    {} --> {}\n", from, to));
            }
            content.push_str("```\n\n");
        }

        content.push_str(&nav_footer(&page_order, "external-services"));
        std::fs::write(&out_path, &content)?;
        println!("  {} {}", "OK".green(), out_path.display());
        page_count += 1;
    }

    // ─── Views & Partials page ────────────────────────────────────────
    {
        let views: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| matches!(n.label, NodeLabel::View | NodeLabel::PartialView))
            .collect();
        if !views.is_empty() {
            page_order.push(("views".to_string(), "Views & Partials".to_string()));
            let out_path = modules_dir.join("views.md");
            let mut content = String::from("# Views & Partials\n\n");
            content.push_str(&format!("**Total :** {} vues\n\n", views.len()));
            content.push_str("| Vue | Type | Modèle | Layout | Contrôleur |\n");
            content.push_str("|-----|------|--------|--------|------------|\n");
            for view in &views {
                let vtype = if view.label == NodeLabel::PartialView {
                    "Partial"
                } else {
                    "View"
                };
                let model = view.properties.model_type.as_deref().unwrap_or("-");
                let layout = view.properties.layout_path.as_deref().unwrap_or("-");
                let ctrl_name = graph
                    .iter_relationships()
                    .find(|r| r.rel_type == RelationshipType::RendersView && r.target_id == view.id)
                    .and_then(|r| graph.get_node(&r.source_id))
                    .map(|n| n.properties.name.clone())
                    .unwrap_or_else(|| "-".to_string());
                content.push_str(&format!(
                    "| `{}` | {} | {} | {} | {} |\n",
                    view.properties.file_path, vtype, model, layout, ctrl_name
                ));
            }
            content.push('\n');
            content.push_str(&nav_footer(&page_order, "views"));
            std::fs::write(&out_path, &content)?;
            println!("  {} {}", "OK".green(), out_path.display());
            page_count += 1;
        }
    }

    Ok(page_count)
}

fn write_llm_badge(content: &mut String, props: &NodeProperties) {
    let risk = props.llm_risk_score;
    let smells = props.llm_smells.as_ref().map(|v| v.len()).unwrap_or(0);
    let patterns = props.llm_patterns.as_ref().map(|v| v.len()).unwrap_or(0);
    if risk.is_none() && smells == 0 && patterns == 0 {
        return;
    }

    content.push_str("\n> **Qualité LLM**");
    if let Some(r) = risk {
        let label = if r >= 70 {
            "Risque élevé"
        } else if r >= 40 {
            "Risque modéré"
        } else {
            "Risque faible"
        };
        content.push_str(&format!(" — {} ({}/100)", label, r));
    }
    content.push('\n');
    if let Some(list) = &props.llm_smells {
        if !list.is_empty() {
            content.push_str(&format!("> *Smells :* {}  \n", list.join(", ")));
        }
    }
    if let Some(list) = &props.llm_patterns {
        if !list.is_empty() {
            content.push_str(&format!("> *Patterns :* {}  \n", list.join(", ")));
        }
    }
    if let Some(hint) = &props.llm_refactoring {
        if !hint.is_empty() {
            content.push_str(&format!("> *Refactoring :* {}  \n", hint));
        }
    }
    content.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn community(label: &str, members: &[&str], keywords: &[&str]) -> CommunityInfo {
        CommunityInfo {
            label: label.to_string(),
            description: None,
            keywords: keywords.iter().map(|kw| (*kw).to_string()).collect(),
            member_ids: members.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    fn node(
        id: &str,
        label: NodeLabel,
        name: &str,
        file_path: &str,
        entry_point_score: Option<f64>,
    ) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            label,
            properties: NodeProperties {
                name: name.to_string(),
                file_path: file_path.to_string(),
                entry_point_score,
                ..Default::default()
            },
        }
    }

    fn rel(
        id: &str,
        source_id: &str,
        target_id: &str,
        rel_type: RelationshipType,
    ) -> GraphRelationship {
        GraphRelationship {
            id: id.to_string(),
            source_id: source_id.to_string(),
            target_id: target_id.to_string(),
            rel_type,
            confidence: 1.0,
            reason: String::new(),
            step: None,
        }
    }

    #[test]
    fn merged_communities_by_filename_deduplicates_members_and_keywords() {
        let mut communities = BTreeMap::new();
        communities.insert(
            "c1".to_string(),
            community("Billing Module", &["A", "B"], &["invoice"]),
        );
        communities.insert(
            "c2".to_string(),
            community("Billing Module", &["B", "C"], &["payment"]),
        );

        let merged = merged_communities_by_filename(&communities);
        let billing = merged.get("billing_module").expect("merged module");

        assert_eq!(billing.member_ids, vec!["A", "B", "C"]);
        assert_eq!(billing.keywords, vec!["invoice", "payment"]);
    }

    #[test]
    fn overview_subsystem_rows_merge_duplicates_and_sort_by_signal() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(node("A", NodeLabel::Method, "A", "src/a.rs", Some(0.9)));
        graph.add_node(node("B", NodeLabel::Method, "B", "src/b.rs", None));
        graph.add_node(node("C", NodeLabel::Method, "C", "src/c.rs", None));
        graph.add_node(node("D", NodeLabel::Method, "D", "src/d.rs", Some(0.7)));
        graph.add_relationship(rel("r1", "A", "D", RelationshipType::Calls));
        graph.add_relationship(rel("r2", "D", "A", RelationshipType::DependsOn));

        let mut communities = BTreeMap::new();
        communities.insert(
            "c1".to_string(),
            community("Billing Module", &["A", "B"], &["invoice"]),
        );
        communities.insert(
            "c2".to_string(),
            community("Billing Module", &["B", "C"], &["payment"]),
        );
        communities.insert(
            "c3".to_string(),
            community("Auth Module", &["D"], &["login"]),
        );

        let rows = overview_subsystem_rows(&communities, &graph);
        let billing_rows = rows
            .iter()
            .filter(|row| row.filename == "billing_module")
            .collect::<Vec<_>>();

        assert_eq!(billing_rows.len(), 1);
        assert_eq!(billing_rows[0].member_count, 3);
        assert_eq!(billing_rows[0].entry_point_count, 1);
        assert_eq!(billing_rows[0].coupling_count, 2);
        assert_eq!(billing_rows[0].description, "Focus: invoice, payment");
        assert_eq!(rows[0].filename, "billing_module");
    }

    #[test]
    fn module_reading_guide_merges_modules_scores_and_adds_questions() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            "A",
            NodeLabel::Method,
            "Gateway",
            "src/gateway.rs",
            Some(0.9),
        ));
        graph.add_node(node(
            "B",
            NodeLabel::Method,
            "Dispatch",
            "src/dispatch.rs",
            None,
        ));
        graph.add_node(node(
            "C",
            NodeLabel::Method,
            "Storage",
            "src/storage.rs",
            None,
        ));
        graph.add_relationship(rel("r1", "A", "C", RelationshipType::Calls));
        graph.add_relationship(rel("r2", "B", "C", RelationshipType::Calls));
        graph.add_relationship(rel("r3", "C", "A", RelationshipType::DependsOn));

        let mut communities = BTreeMap::new();
        communities.insert("c1".to_string(), community("Gateway", &["A"], &["entry"]));
        communities.insert(
            "c2".to_string(),
            community("Gateway", &["B"], &["dispatch"]),
        );
        communities.insert(
            "c3".to_string(),
            community("Storage", &["C"], &["snapshot"]),
        );

        let guide = module_reading_guide(&communities, &graph);
        let gateway_rows = guide
            .iter()
            .filter(|step| step.filename == "gateway")
            .collect::<Vec<_>>();

        assert_eq!(guide.len(), 2);
        assert_eq!(gateway_rows.len(), 1);
        assert!(guide.windows(2).all(|pair| pair[0].score >= pair[1].score));
        assert!(gateway_rows[0].reason.contains("inter-modules"));
        assert!(gateway_rows[0].question.contains("Gateway"));
    }

    #[test]
    fn architecture_diagram_uses_merged_modules_without_duplicate_nodes() {
        let root =
            std::env::temp_dir().join(format!("code-explorer-architecture-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("docs dir");

        let mut graph = KnowledgeGraph::new();
        graph.add_node(node("A", NodeLabel::Method, "A", "src/a.rs", None));
        graph.add_node(node("D", NodeLabel::Method, "D", "src/d.rs", None));
        graph.add_relationship(rel("r1", "A", "D", RelationshipType::Calls));

        let mut communities = BTreeMap::new();
        communities.insert(
            "c1".to_string(),
            community("Commands", &["A", "B"], &["cli"]),
        );
        communities.insert(
            "c2".to_string(),
            community("Commands", &["B", "C"], &["dispatch"]),
        );
        communities.insert(
            "c3".to_string(),
            community("Storage", &["D"], &["snapshot"]),
        );

        generate_docs_architecture(&root, &communities, &graph, &HashMap::new(), 2, 2, 1)
            .expect("architecture page");

        let page = std::fs::read_to_string(root.join("architecture.md")).expect("architecture md");
        assert!(page.contains("Architecture système avec **2 modules**"));
        assert_eq!(page.matches("    M_commands[\"").count(), 1);
        assert_eq!(page.matches("    M_storage[\"").count(), 1);
        assert!(page.contains("M_commands -->|1| M_storage"));
        assert!(page.contains("### Topologie des Modules"));
        assert!(page.contains("| [Commands](modules/commands.md) | 3 |"));

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn code_map_page_hides_internal_markers_and_uses_french_navigation() {
        let root = std::env::temp_dir().join(format!("code-explorer-code-map-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("docs dir");

        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            "A",
            NodeLabel::Method,
            "Gateway",
            "src/gateway.rs",
            Some(0.9),
        ));
        graph.add_node(node(
            "B",
            NodeLabel::Method,
            "Storage",
            "src/storage.rs",
            None,
        ));
        graph.add_relationship(rel("r1", "A", "B", RelationshipType::Calls));

        let communities = BTreeMap::from([
            (
                "gateway".to_string(),
                community("Gateway", &["A"], &["entry"]),
            ),
            (
                "storage".to_string(),
                community("Storage", &["B"], &["snapshot"]),
            ),
        ]);

        generate_docs_code_map(&root, &communities, &graph, &HashMap::new()).expect("code map");

        let page = std::fs::read_to_string(root.join("code-map.md")).expect("code map md");
        assert!(!page.contains("GNX:"));
        assert!(!page.contains("Previous:"));
        assert!(!page.contains("Next:"));
        assert!(!page.contains("Getting Started"));
        assert!(page.contains("[<- Précédent : Architecture](./architecture.md)"));
        assert!(page.contains("[Suivant : Prise en Main ->](./getting-started.md)"));

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn getting_started_page_hides_internal_markers_and_uses_french_copy() {
        let root =
            std::env::temp_dir().join(format!("code-explorer-getting-started-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("docs dir");

        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            "F1",
            NodeLabel::File,
            "gateway.rs",
            "crates/app/src/gateway.rs",
            None,
        ));
        graph.add_node(node(
            "A",
            NodeLabel::Method,
            "Gateway",
            "crates/app/src/gateway.rs",
            Some(0.9),
        ));

        let communities = BTreeMap::from([(
            "gateway".to_string(),
            community("Gateway", &["A"], &["entry"]),
        )]);

        generate_docs_getting_started(&root, "demo-repo", &communities, &graph)
            .expect("getting started");

        let page =
            std::fs::read_to_string(root.join("getting-started.md")).expect("getting started md");
        assert!(!page.contains("GNX:"));
        assert!(!page.contains("Welcome to"));
        assert!(!page.contains("Previous:"));
        assert!(!page.contains("Next:"));
        assert!(page.contains("Bienvenue dans le codebase **demo-repo**."));
        assert!(page.contains("[<- Précédent : Carte du Code](./code-map.md)"));
        assert!(page.contains("[Suivant : Déploiement ->](./deployment.md)"));

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn module_dependency_edges_count_only_cross_module_code_edges() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(node("A", NodeLabel::Method, "A", "src/a.rs", Some(0.9)));
        graph.add_node(node("B", NodeLabel::Method, "B", "src/b.rs", None));
        graph.add_node(node("C", NodeLabel::Method, "C", "src/c.rs", None));
        graph.add_relationship(rel("r1", "A", "B", RelationshipType::Calls));
        graph.add_relationship(rel("r2", "A", "B", RelationshipType::DependsOn));
        graph.add_relationship(rel("r3", "A", "C", RelationshipType::Contains));
        graph.add_relationship(rel("r4", "B", "B", RelationshipType::Calls));

        let member_to_module = HashMap::from([
            ("A".to_string(), "module-a".to_string()),
            ("B".to_string(), "module-b".to_string()),
            ("C".to_string(), "module-c".to_string()),
        ]);

        assert_eq!(
            module_dependency_edges(&graph, &member_to_module),
            vec![ModuleDependencyEdge {
                source: "module-a".to_string(),
                target: "module-b".to_string(),
                count: 2,
            }]
        );
    }

    #[test]
    fn module_dependency_neighbors_are_directional_and_sorted() {
        let communities = BTreeMap::from([
            (
                "module-a".to_string(),
                community("Module A", &["A"], &["entry"]),
            ),
            (
                "module-b".to_string(),
                community("Module B", &["B"], &["helper"]),
            ),
            (
                "module-c".to_string(),
                community("Module C", &["C"], &["helper"]),
            ),
        ]);
        let edges = vec![
            ModuleDependencyEdge {
                source: "module-a".to_string(),
                target: "module-b".to_string(),
                count: 2,
            },
            ModuleDependencyEdge {
                source: "module-a".to_string(),
                target: "module-c".to_string(),
                count: 5,
            },
            ModuleDependencyEdge {
                source: "module-b".to_string(),
                target: "module-a".to_string(),
                count: 3,
            },
        ];

        assert_eq!(
            module_dependency_neighbors(
                "module-a",
                &communities,
                &edges,
                ModuleDependencyDirection::Outgoing
            ),
            vec![
                ModuleNeighbor {
                    filename: "module-c".to_string(),
                    label: "Module C".to_string(),
                    count: 5,
                },
                ModuleNeighbor {
                    filename: "module-b".to_string(),
                    label: "Module B".to_string(),
                    count: 2,
                },
            ]
        );
        assert_eq!(
            module_dependency_neighbors(
                "module-a",
                &communities,
                &edges,
                ModuleDependencyDirection::Incoming
            ),
            vec![ModuleNeighbor {
                filename: "module-b".to_string(),
                label: "Module B".to_string(),
                count: 3,
            }]
        );
    }

    #[test]
    fn unique_wiki_page_slug_disambiguates_and_handles_empty_titles() {
        let mut used = HashSet::new();

        assert_eq!(
            unique_wiki_page_slug("Authentication System", &mut used),
            "authentication_system"
        );
        assert_eq!(
            unique_wiki_page_slug("Authentication System", &mut used),
            "authentication_system-2"
        );
        assert_eq!(unique_wiki_page_slug("!!!", &mut used), "page");
    }

    #[test]
    fn guided_wiki_page_sources_match_title_purpose_and_notes() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            "F1",
            NodeLabel::File,
            "LoginController.cs",
            "src/Auth/LoginController.cs",
            None,
        ));
        graph.add_node(node(
            "F2",
            NodeLabel::File,
            "InvoiceService.cs",
            "src/Billing/InvoiceService.cs",
            None,
        ));

        let page = WikiSteeringPage {
            title: "Authentication System".to_string(),
            purpose: "Document the login flow and auth controllers".to_string(),
            parent: None,
            page_notes: vec![WikiSteeringNote::Text(
                "Include token validation and sign-in behavior.".to_string(),
            )],
        };

        assert_eq!(
            source_files_for_guided_page(&graph, &page),
            vec!["src/Auth/LoginController.cs".to_string()]
        );
    }

    #[test]
    fn guided_symbol_matches_link_modules_and_boost_entry_points() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            "M1",
            NodeLabel::Method,
            "LoginController",
            "src/Auth/LoginController.cs",
            Some(0.8),
        ));
        graph.add_node(node(
            "M2",
            NodeLabel::Method,
            "InvoiceService",
            "src/Billing/InvoiceService.cs",
            None,
        ));
        let communities = BTreeMap::from([(
            "auth".to_string(),
            community("Authentication", &["M1"], &["login"]),
        )]);
        let member_to_module = build_member_to_module(&communities);
        let page = WikiSteeringPage {
            title: "Authentication System".to_string(),
            purpose: "Document login controller behavior".to_string(),
            parent: None,
            page_notes: Vec::new(),
        };

        let matches =
            guided_symbol_matches(&graph, &page, &member_to_module, &communities, "../modules");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "LoginController");
        assert_eq!(matches[0].module, "[Authentication](../modules/auth.md)");
        assert!(matches[0].score >= 2);
    }

    #[test]
    fn wiki_steering_validation_rejects_duplicate_titles() {
        let config = WikiSteeringConfig {
            repo_notes: Vec::new(),
            pages: vec![
                WikiSteeringPage {
                    title: "Auth".to_string(),
                    purpose: "Document auth".to_string(),
                    parent: None,
                    page_notes: Vec::new(),
                },
                WikiSteeringPage {
                    title: "auth".to_string(),
                    purpose: "Document auth again".to_string(),
                    parent: None,
                    page_notes: Vec::new(),
                },
            ],
        };

        let err = validate_wiki_steering_config(&config, Path::new(".devin/wiki.json"))
            .expect_err("duplicate title should fail");
        assert!(err.to_string().contains("duplicate guided page title"));
    }

    #[test]
    fn generate_wiki_steering_pages_writes_plan_and_guided_pages() {
        let root =
            std::env::temp_dir().join(format!("code-explorer-wiki-steering-{}", uuid::Uuid::new_v4()));
        let repo = root.join("repo");
        let docs = root.join("docs");
        std::fs::create_dir_all(repo.join(".devin")).expect("repo config dir");
        std::fs::create_dir_all(&docs).expect("docs dir");
        std::fs::write(
            repo.join(".devin").join("wiki.json"),
            r#"{
              "repo_notes": [{"content": "Prioritize authentication.", "author": "Patrice"}],
              "pages": [{
                "title": "Authentication System",
                "purpose": "Document the login flow and auth controllers",
                "parent": null,
                "page_notes": ["Include token validation."]
              }]
            }"#,
        )
        .expect("write config");

        let mut graph = KnowledgeGraph::new();
        graph.add_node(node(
            "F1",
            NodeLabel::File,
            "LoginController.cs",
            "src/Auth/LoginController.cs",
            None,
        ));
        graph.add_node(node(
            "M1",
            NodeLabel::Method,
            "LoginController",
            "src/Auth/LoginController.cs",
            Some(0.8),
        ));
        graph.add_node(node("C1", NodeLabel::Community, "Authentication", "", None));
        graph.add_relationship(rel("r-member", "M1", "C1", RelationshipType::MemberOf));

        let pages = generate_wiki_steering_pages(&docs, &repo, &graph).expect("guided pages");

        assert_eq!(
            pages,
            vec![
                (
                    "wiki-steering".to_string(),
                    "Plan de Wiki Guidé".to_string(),
                    "wiki-steering.md".to_string(),
                ),
                (
                    "guided-authentication_system".to_string(),
                    "Authentication System".to_string(),
                    "guided/authentication_system.md".to_string(),
                ),
            ]
        );
        let plan = std::fs::read_to_string(docs.join("wiki-steering.md")).expect("plan page");
        assert!(plan.contains("Prioritize authentication."));
        let guided = std::fs::read_to_string(docs.join("guided").join("authentication_system.md"))
            .expect("guided page");
        assert!(guided.contains("src/Auth/LoginController.cs"));
        assert!(guided.contains("## Symboles probables"));
        assert!(guided.contains("[Authentication](../modules/authentication.md)"));

        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn module_map_stats_prioritizes_entry_points_then_dependencies() {
        let mut graph = KnowledgeGraph::new();
        graph.add_node(node("A", NodeLabel::Method, "A", "src/a.rs", Some(0.9)));
        graph.add_node(node("B", NodeLabel::Method, "B", "src/b.rs", None));

        let communities = BTreeMap::from([
            (
                "module-a".to_string(),
                community("Module A", &["A"], &["entry"]),
            ),
            (
                "module-b".to_string(),
                community("Module B", &["B"], &["helper"]),
            ),
        ]);
        let edges = vec![ModuleDependencyEdge {
            source: "module-b".to_string(),
            target: "module-a".to_string(),
            count: 3,
        }];

        let stats = module_map_stats(&communities, &graph, &edges);

        assert_eq!(stats[0].filename, "module-a");
        assert_eq!(stats[0].entry_point_count, 1);
        assert_eq!(stats[0].incoming_calls, 3);
        assert_eq!(stats[1].outgoing_calls, 3);
    }
}
