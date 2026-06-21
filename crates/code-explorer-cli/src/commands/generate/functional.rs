//! Functional guide generator (business-oriented documentation).

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

use super::utils::*;

pub(super) fn generate_functional_guide(
    docs_dir: &Path,
    repo_name: &str,
    graph: &KnowledgeGraph,
) -> Result<()> {
    let label_counts = count_nodes_by_label(graph);
    let has_controllers = label_counts
        .get(&NodeLabel::Controller)
        .copied()
        .unwrap_or(0)
        > 0;

    // Only generate for ASP.NET MVC projects with controllers
    if !has_controllers {
        return Ok(());
    }

    let out_path = docs_dir.join("functional-guide.md");
    let mut f = std::fs::File::create(&out_path)?;

    let ctrl_count = label_counts
        .get(&NodeLabel::Controller)
        .copied()
        .unwrap_or(0);
    let view_count = label_counts.get(&NodeLabel::View).copied().unwrap_or(0);
    let action_count = label_counts
        .get(&NodeLabel::ControllerAction)
        .copied()
        .unwrap_or(0);
    let entity_count = label_counts.get(&NodeLabel::DbEntity).copied().unwrap_or(0);
    let svc_count = label_counts.get(&NodeLabel::Service).copied().unwrap_or(0);
    let ui_count = label_counts
        .get(&NodeLabel::UiComponent)
        .copied()
        .unwrap_or(0);
    let ext_count = label_counts
        .get(&NodeLabel::ExternalService)
        .copied()
        .unwrap_or(0);

    // Collect controllers and group actions by controller
    let controllers: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::Controller)
        .collect();

    writeln!(f, "# Guide Fonctionnel — {}", repo_name)?;
    writeln!(f, "<!-- GNX:LEAD -->")?;
    writeln!(f)?;

    // Source files
    let ctrl_files: Vec<&str> = controllers
        .iter()
        .map(|c| c.properties.file_path.as_str())
        .take(10)
        .collect();
    write!(f, "{}", source_files_section(&ctrl_files))?;

    writeln!(
        f,
        "> Ce guide décrit les modules fonctionnels de l'application du point de vue métier."
    )?;
    writeln!(
        f,
        "> Il est destiné aux responsables de service et aux personnes reprenant l'application."
    )?;
    writeln!(f)?;

    // Quick stats
    writeln!(f, "| Métrique | Valeur |")?;
    writeln!(f, "|----------|--------|")?;
    writeln!(f, "| Modules fonctionnels | {} controllers |", ctrl_count)?;
    writeln!(f, "| Fonctionnalités | {} actions |", action_count)?;
    writeln!(f, "| Écrans | {} vues |", view_count)?;
    writeln!(f, "| Entités de données | {} |", entity_count)?;
    writeln!(f, "| Services métier | {} |", svc_count)?;
    writeln!(f, "| Composants UI | {} grilles Telerik |", ui_count)?;
    writeln!(f, "| Intégrations externes | {} services |", ext_count)?;
    writeln!(f)?;

    // Generate module documentation for each controller
    // Sort by action count descending (most important first)
    let mut ctrl_with_actions: Vec<(&GraphNode, Vec<&GraphNode>)> = controllers
        .iter()
        .map(|ctrl| {
            let actions: Vec<&GraphNode> = graph
                .iter_nodes()
                .filter(|n| {
                    n.label == NodeLabel::ControllerAction
                        && n.properties.file_path == ctrl.properties.file_path
                })
                .collect();
            (*ctrl, actions)
        })
        .collect();
    ctrl_with_actions.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    for (ctrl, actions) in &ctrl_with_actions {
        let name = ctrl
            .properties
            .name
            .strip_suffix("Controller")
            .unwrap_or(&ctrl.properties.name);

        // Skip RootController (base class, not a real module)
        if name == "Root" || name == "PdfView" || name == "Print" {
            continue;
        }

        writeln!(f, "---")?;
        writeln!(f)?;
        writeln!(f, "## {}", name)?;
        writeln!(f)?;

        // Heuristic business description
        let desc = describe_controller_fr(&ctrl.properties.name);
        writeln!(f, "**Finalité métier :** {}", desc)?;
        writeln!(f)?;

        // Count views for this controller. Match the conventional ASP.NET
        // MVC layout `Views/{ControllerName}/*.cshtml` so a controller like
        // `User` does not accidentally pick up unrelated directories whose
        // name merely CONTAINS "User" (e.g. `Views/PasswordUserReset/`).
        let path_segment = format!("/{}/", name);
        let ctrl_views: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| {
                n.label == NodeLabel::View && n.properties.file_path.contains(&path_segment)
            })
            .collect();

        // Same segment match for UI components (Telerik grids etc.).
        let ctrl_ui: Vec<&GraphNode> = graph
            .iter_nodes()
            .filter(|n| {
                n.label == NodeLabel::UiComponent && n.properties.file_path.contains(&path_segment)
            })
            .collect();

        writeln!(f, "| | |")?;
        writeln!(f, "|---|---|")?;
        writeln!(f, "| **Actions** | {} |", actions.len())?;
        writeln!(f, "| **Écrans** | {} vues |", ctrl_views.len())?;
        if !ctrl_ui.is_empty() {
            writeln!(f, "| **Grilles Telerik** | {} |", ctrl_ui.len())?;
        }
        writeln!(f)?;

        // Key actions (group by GET/POST)
        let _get_actions: Vec<&&GraphNode> = actions
            .iter()
            .filter(|a| a.properties.http_method.as_deref().unwrap_or("GET") == "GET")
            .collect();
        let _post_actions: Vec<&&GraphNode> = actions
            .iter()
            .filter(|a| a.properties.http_method.as_deref().unwrap_or("GET") == "POST")
            .collect();

        writeln!(f, "**Processus principaux :**")?;
        writeln!(f)?;

        // List top actions by name patterns
        let mut listed = 0;
        for action in actions.iter().take(15) {
            let aname = &action.properties.name;
            let method = action.properties.http_method.as_deref().unwrap_or("GET");
            let icon = if method == "POST" {
                "\u{270f}\u{fe0f}"
            } else {
                "\u{1f4c4}"
            };
            writeln!(f, "- {} **{}** ({})", icon, aname, method)?;
            listed += 1;
        }
        if actions.len() > listed {
            writeln!(f, "- *...et {} autres actions*", actions.len() - listed)?;
        }
        writeln!(f)?;

        // Key grids
        if !ctrl_ui.is_empty() {
            writeln!(f, "**Grilles principales :**")?;
            writeln!(f)?;
            for comp in ctrl_ui.iter().take(5) {
                let cols = comp.properties.description.as_deref().unwrap_or("");
                let model = comp.properties.bound_model.as_deref().unwrap_or("-");
                writeln!(f, "- **{}** (modèle: `{}`)", comp.properties.name, model)?;
                if !cols.is_empty() {
                    writeln!(f, "  - Colonnes : {}", cols)?;
                }
            }
            writeln!(f)?;
        }

        // Criticality
        let criticality = if actions.len() > 30 {
            "\u{1f534} **Très élevé** — Module complexe avec de nombreuses fonctionnalités"
        } else if actions.len() > 10 {
            "\u{1f7e1} **Élevé** — Module important dans le workflow quotidien"
        } else {
            "\u{1f7e2} **Moyen** — Module de support ou consultation"
        };
        writeln!(f, "**Niveau de criticité :** {}", criticality)?;
        writeln!(f)?;

        // Simple flow diagram (only for major controllers)
        if actions.len() > 5 {
            writeln!(f, "**Flux principal :**")?;
            writeln!(f)?;
            writeln!(f, "```mermaid")?;
            writeln!(f, "flowchart LR")?;

            // Show: Search → View/Create → Edit → Validate
            let has_search = actions.iter().any(|a| {
                let n = a.properties.name.to_lowercase();
                n.contains("rech")
                    || n.contains("search")
                    || n.contains("list")
                    || n.contains("get")
            });
            let has_create = actions.iter().any(|a| {
                let n = a.properties.name.to_lowercase();
                n.contains("cre") || n.contains("new") || n.contains("add")
            });
            let has_edit = actions.iter().any(|a| {
                let n = a.properties.name.to_lowercase();
                n.contains("modif") || n.contains("edit") || n.contains("update")
            });
            let has_detail = actions.iter().any(|a| {
                let n = a.properties.name.to_lowercase();
                n.contains("detail") || n.contains("view")
            });
            let has_export = actions.iter().any(|a| {
                let n = a.properties.name.to_lowercase();
                n.contains("export") || n.contains("excel") || n.contains("csv")
            });
            let has_delete = actions.iter().any(|a| {
                let n = a.properties.name.to_lowercase();
                n.contains("suppr") || n.contains("delete")
            });

            let mut steps = Vec::new();
            if has_search {
                steps.push(("Recherche", "Rechercher"));
            }
            if has_detail {
                steps.push(("Consultation", "Consulter"));
            }
            if has_create {
                steps.push(("Creation", "Créer"));
            }
            if has_edit {
                steps.push(("Modification", "Modifier"));
            }
            if has_delete {
                steps.push(("Suppression", "Supprimer"));
            }
            if has_export {
                steps.push(("Export", "Exporter"));
            }

            for (id, label) in &steps {
                writeln!(f, "    {}[\"{}\" ]", id, label)?;
            }
            for i in 0..steps.len().saturating_sub(1) {
                writeln!(f, "    {} --> {}", steps[i].0, steps[i + 1].0)?;
            }

            writeln!(f, "```")?;
            writeln!(f)?;
        }
    }

    // Sequence diagrams for critical flows
    writeln!(f, "---")?;
    writeln!(f)?;
    writeln!(f, "## Flux critiques")?;
    writeln!(f, "<!-- GNX:INTRO:flux-critiques -->")?;
    writeln!(f)?;

    writeln!(f, "### Recherche Bénéficiaire")?;
    writeln!(f)?;
    writeln!(f, "```mermaid")?;
    writeln!(f, "sequenceDiagram")?;
    writeln!(f, "    participant U as Utilisateur")?;
    writeln!(f, "    participant C as BeneficiaireController")?;
    writeln!(f, "    participant S as BenefService")?;
    writeln!(f, "    participant API as ExternalApi API")?;
    writeln!(f, "    U->>C: Recherche (NIA ou Nom)")?;
    writeln!(f, "    C->>S: RechercheOuvrantDroit(filtre)")?;
    writeln!(f, "    S->>API: AcmeClient.OuvrantsDroitGetAsync()")?;
    writeln!(f, "    API-->>S: FicheODLite[]")?;
    writeln!(f, "    S->>API: PartnerClient.MembersGetAsync()")?;
    writeln!(f, "    API-->>S: Members (household composition)")?;
    writeln!(f, "    S-->>C: Liste bénéficiaires")?;
    writeln!(f, "    C-->>U: Grille Telerik avec résultats")?;
    writeln!(f, "```")?;
    writeln!(f)?;

    writeln!(f, "### Création Dossier")?;
    writeln!(f)?;
    writeln!(f, "```mermaid")?;
    writeln!(f, "sequenceDiagram")?;
    writeln!(f, "    participant U as Utilisateur")?;
    writeln!(f, "    participant C as DossiersController")?;
    writeln!(f, "    participant S as DossierService")?;
    writeln!(f, "    participant DB as Entity Framework")?;
    writeln!(f, "    U->>C: Sélection Domaine + Groupe Aide")?;
    writeln!(f, "    C->>S: AfficherAides(idGrpAide)")?;
    writeln!(f, "    S-->>C: Liste Aides disponibles")?;
    writeln!(f, "    U->>C: Choix Aides + Dates")?;
    writeln!(f, "    C->>S: CreerDossier(DossierPresta)")?;
    writeln!(f, "    S->>S: Calcul Barème + Plafonds")?;
    writeln!(f, "    S->>DB: Insert Dossier + Prestations")?;
    writeln!(f, "    DB-->>S: OK")?;
    writeln!(f, "    S-->>C: Dossier créé")?;
    writeln!(f, "    C-->>U: Page détails dossier")?;
    writeln!(f, "```")?;
    writeln!(f)?;

    writeln!(f, "### Export ELODIE")?;
    writeln!(f)?;
    writeln!(f, "```mermaid")?;
    writeln!(f, "sequenceDiagram")?;
    writeln!(f, "    participant U as Utilisateur")?;
    writeln!(f, "    participant C as FacturesController")?;
    writeln!(f, "    participant S as FactureService")?;
    writeln!(f, "    participant DB as Entity Framework")?;
    writeln!(f, "    U->>C: Validation paiements")?;
    writeln!(f, "    C->>S: ValidationPaiement()")?;
    writeln!(f, "    S->>S: Vérification montants + plafonds")?;
    writeln!(f, "    S->>DB: Mise à jour statuts")?;
    writeln!(f, "    U->>C: Générer bordereau")?;
    writeln!(f, "    C->>S: GenerBordereau()")?;
    writeln!(f, "    S->>DB: Création bordereaux")?;
    writeln!(f, "    U->>C: Export ELODIE")?;
    writeln!(f, "    C->>S: ExportElodie()")?;
    writeln!(f, "    S->>S: Formatage Flux3")?;
    writeln!(f, "    S-->>C: Fichier ELODIE")?;
    writeln!(f, "    C-->>U: Téléchargement fichier")?;
    writeln!(f, "```")?;
    writeln!(f)?;

    // Synthesis
    writeln!(f, "<!-- GNX:CLOSING -->")?;
    writeln!(f, "---")?;
    writeln!(f)?;
    writeln!(f, "## Synthèse : Modules les plus critiques")?;
    writeln!(f)?;

    // Sort by action count, take top 3
    let top3: Vec<&(&GraphNode, Vec<&GraphNode>)> = ctrl_with_actions
        .iter()
        .filter(|(c, _)| {
            let n = c.properties.name.as_str();
            n != "RootController" && n != "PdfViewController" && n != "PrintController"
        })
        .take(3)
        .collect();

    for (i, (ctrl, actions)) in top3.iter().enumerate() {
        let name = ctrl
            .properties
            .name
            .strip_suffix("Controller")
            .unwrap_or(&ctrl.properties.name);
        writeln!(f, "### {}. {}", i + 1, name)?;
        writeln!(f)?;
        writeln!(
            f,
            "**{} actions** — {}",
            actions.len(),
            describe_controller_fr(&ctrl.properties.name)
        )?;
        writeln!(f)?;
    }

    writeln!(f, "---")?;
    writeln!(f)?;
    writeln!(
        f,
        "**Voir aussi :** [Vue d'ensemble](./overview.md) · [Architecture](./architecture.md)"
    )?;
    writeln!(f)?;
    writeln!(f, "[\u{2190} Previous: Overview](./overview.md) | [Next: Architecture \u{2192}](./architecture.md)")?;

    println!("  {} {}", "OK".green(), out_path.display());

    Ok(())
}

/// French business description for a project based on its name.
pub(super) fn describe_project_fr(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("ihm") && !lower.contains("test") {
        "Application web ASP.NET MVC (Présentation)"
    } else if lower.contains("bal") && !lower.contains("test") {
        "Couche métier (Business Logic)"
    } else if lower.contains("dal") && !lower.contains("test") {
        "Couche d'accès aux données (Entity Framework)"
    } else if lower.contains("entities") {
        "Entités / objets métier partagés"
    } else if lower.contains("commun") {
        "Utilitaires et attributs communs"
    } else if lower.contains("courrier") && !lower.contains("test") {
        "Génération de courriers (mail merge)"
    } else if lower.contains("erable") || lower.contains("webapi") {
        "Client API REST ExternalApi (bénéficiaires)"
    } else if lower.contains("ldap") {
        "Client LDAP / Active Directory"
    } else if lower.contains("pdf") {
        "Génération de rapports PDF"
    } else if lower.contains("ressource") {
        "Fichiers de ressources (localisation)"
    } else if lower.contains("traitement") || lower.contains("batch") {
        "Traitement batch / planifié"
    } else if lower.contains("console") {
        "Application console"
    } else if lower.contains("test") {
        "Tests unitaires / intégration"
    } else {
        "Projet"
    }
}

pub(super) fn describe_controller_fr(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("administration") {
        "Configurer le référentiel d'aides (groupes, aides, barèmes, plafonds, majorations, tarifs, justificatifs). C'est le socle de paramétrage dont dépend toute l'application."
    } else if lower.contains("dossier") {
        "Gérer le cycle de vie complet des dossiers d'aide sociale — de la demande à la clôture, en passant par le calcul des droits via les barèmes et la sélection des aides."
    } else if lower.contains("facture") {
        "Gérer la chaîne financière : facturation fournisseurs, paiement bénéficiaires, régularisations, validation et export ELODIE vers la comptabilité centrale."
    } else if lower.contains("beneficiaire") {
        "Rechercher et consulter les profils des ouvrants droit (OD) et ayants droit (AD) issus du WebAPI ExternalApi, puis les lier aux dossiers d'aide."
    } else if lower.contains("courrier") {
        "Générer des courriers personnalisés aux bénéficiaires — individuellement ou en masse — à partir de modèles avec champs de fusion."
    } else if lower.contains("statistique") {
        "Produire les tableaux de bord et rapports réglementaires : suivi budgétaire, comptage dossiers, analyse paiements, restitutions mensuelles."
    } else if lower.contains("fournisseur") {
        "Gérer le référentiel des fournisseurs de prestations sociales et leur association aux dossiers."
    } else if lower.contains("utilisateur") {
        "Administrer les comptes utilisateurs, les profils d'habilitation et les droits d'accès par Acme."
    } else if lower.contains("profil") {
        "Gérer les profils d'habilitation et les autorisations fonctionnelles des utilisateurs."
    } else if lower.contains("intervention") {
        "Suivre les interventions terrain liées aux dossiers de bénéficiaires."
    } else if lower.contains("commission") {
        "Gérer les commissions d'attribution des aides (nationales et locales)."
    } else if lower.contains("mco") {
        "Module de maintien en condition opérationnelle — suivi de l'éligibilité et des cas particuliers."
    } else if lower.contains("archiver") {
        "Archiver les dossiers clôturés pour libérer l'espace de travail courant."
    } else if lower.contains("home") {
        "Page d'accueil avec messages d'information, authentification et navigation principale."
    } else {
        "Module fonctionnel de l'application."
    }
}
