//! Deployment guide generator.

use std::io::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;

use code_explorer_core::graph::types::*;
use code_explorer_core::graph::KnowledgeGraph;

pub(super) fn generate_deployment_guide(
    docs_dir: &Path,
    _repo_name: &str,
    graph: &KnowledgeGraph,
) -> Result<()> {
    let out_path = docs_dir.join("deployment.md");
    let mut f = std::fs::File::create(&out_path)?;

    writeln!(f, "# Guide Environnement & Déploiement")?;
    writeln!(f)?;
    writeln!(
        f,
        "> Informations techniques pour configurer et déployer l'application."
    )?;
    writeln!(f)?;

    writeln!(f, "## Prérequis")?;
    writeln!(f, "- .NET Framework 4.8")?;
    writeln!(f, "- Visual Studio 2019/2022")?;
    writeln!(f, "- SQL Server 2012+")?;
    writeln!(f, "- IIS / IIS Express")?;
    writeln!(f, "- Node.js (pour les scripts de build frontend)")?;
    writeln!(f)?;

    writeln!(f)?;

    // Databases from DbContext nodes
    let db_contexts: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::DbContext)
        .collect();
    writeln!(f, "## Bases de données")?;
    writeln!(f)?;
    if db_contexts.is_empty() {
        writeln!(f, "Aucun DbContext détecté.")?;
    } else {
        for ctx in &db_contexts {
            writeln!(
                f,
                "- **{}** (`{}`)",
                ctx.properties.name, ctx.properties.file_path
            )?;
        }
    }
    writeln!(f)?;

    // External services
    let ext_services: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| n.label == NodeLabel::ExternalService)
        .collect();
    writeln!(f, "## Services externes")?;
    writeln!(f)?;
    if ext_services.is_empty() {
        writeln!(f, "Aucun service externe détecté.")?;
    } else {
        for svc in &ext_services {
            let stype = svc.properties.service_type.as_deref().unwrap_or("REST");
            writeln!(f, "- **{}** ({})", svc.properties.name, stype)?;
        }
    }
    writeln!(f)?;

    writeln!(f, "## Configuration")?;
    writeln!(f, "<!-- GNX:INTRO:configuration -->")?;
    writeln!(f)?;
    writeln!(
        f,
        "Les fichiers `Web.config` contiennent les paramètres par environnement."
    )?;
    writeln!(
        f,
        "Chaque environnement a sa propre transformation `Web.{{env}}.config`."
    )?;
    writeln!(f)?;

    // List config files detected
    let config_files: Vec<&GraphNode> = graph
        .iter_nodes()
        .filter(|n| {
            n.label == NodeLabel::File
                && (n.properties.file_path.ends_with(".config")
                    || n.properties.file_path.ends_with(".Config"))
                && !n.properties.file_path.contains("PackageTmp")
                && !n.properties.file_path.contains("/obj/")
                && !n.properties.file_path.contains("\\obj\\")
        })
        .collect();

    if !config_files.is_empty() {
        writeln!(f, "### Fichiers de configuration détectés")?;
        writeln!(f)?;
        writeln!(f, "| Fichier | Rôle |")?;
        writeln!(f, "|---------|------|")?;
        for cf in &config_files {
            let path = cf.properties.file_path.replace('\\', "/");
            let role = if path.contains("Web.config")
                && !path.contains(".Release")
                && !path.contains(".Debug")
            {
                "Configuration principale"
            } else if path.contains("Release") {
                "Transformation production"
            } else if path.contains("Debug") {
                "Transformation développement"
            } else if path.contains("Qualification") {
                "Transformation qualification"
            } else if path.contains("packages.config") {
                "Dépendances NuGet"
            } else {
                "Configuration"
            };
            writeln!(f, "| `{}` | {} |", path, role)?;
        }
        writeln!(f)?;
    }

    // ASP.NET deployment checklist
    let has_controllers = graph.iter_nodes().any(|n| n.label == NodeLabel::Controller);
    if has_controllers {
        writeln!(f, "## Déploiement ASP.NET MVC")?;
        writeln!(f, "<!-- GNX:INTRO:deploiement-aspnet -->")?;
        writeln!(f)?;
        writeln!(f, "### Checklist")?;
        writeln!(f)?;
        writeln!(
            f,
            "1. **Compiler en Release** : `msbuild /p:Configuration=Release`"
        )?;
        writeln!(
            f,
            "2. **Publier** : clic droit \u{2192} Publier \u{2192} Profil de publication"
        )?;
        writeln!(
            f,
            "3. **Transformations** : `Web.Release.config` appliquée automatiquement"
        )?;
        writeln!(
            f,
            "4. **IIS** : pool .NET 4.x (pipeline intégré), pointer vers le dossier publié"
        )?;
        writeln!(
            f,
            "5. **ConnectionStrings** : configurer dans `Web.config` du serveur"
        )?;
        writeln!(f, "6. **Tester** : naviguer vers l'URL du site")?;
        writeln!(f)?;

        writeln!(f, "### Environnements")?;
        writeln!(f)?;
        writeln!(f, "| Environnement | Transformation | Usage |")?;
        writeln!(f, "|--------------|----------------|-------|")?;
        writeln!(
            f,
            "| Développement | `Web.Debug.config` | Debug local (IIS Express) |"
        )?;
        writeln!(
            f,
            "| Qualification | `Web.Qualification.config` | Tests pré-production |"
        )?;
        writeln!(
            f,
            "| Production | `Web.Release.config` | Serveur de production |"
        )?;
        writeln!(f)?;
    }

    println!("  {} deployment.md", "OK".green());
    Ok(())
}

// ─── Service Description Helper ───────────────────────────────────────

pub(super) fn describe_service_fr(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower.contains("aide") {
        "Gestion des aides financières et paramétrage"
    } else if lower.contains("bareme") {
        "Calcul des barèmes et tranches de revenus"
    } else if lower.contains("dossier") {
        "Création et suivi des dossiers d'aide"
    } else if lower.contains("facture") {
        "Facturation fournisseurs et paiements"
    } else if lower.contains("benef") {
        "Recherche et gestion des bénéficiaires"
    } else if lower.contains("courrier") {
        "Génération et envoi de courriers"
    } else if lower.contains("profil") {
        "Gestion des profils et habilitations"
    } else if lower.contains("utilisateur") {
        "Administration des comptes utilisateurs"
    } else if lower.contains("statistique") {
        "Tableaux de bord et restitutions"
    } else if lower.contains("parametr") {
        "Configuration et paramètres système"
    } else if lower.contains("message") {
        "Gestion des messages d'erreur et d'accueil"
    } else if lower.contains("grpaide") {
        "Gestion des groupes d'aides"
    } else if lower.contains("cmcas") {
        "Données et paramètres Acme"
    } else if lower.contains("background") {
        "Traitement asynchrone (Hangfire)"
    } else if lower.contains("elodie") {
        "Export comptable vers ELODIE"
    } else if lower.contains("numcommi") {
        "Numérotation des commissions"
    } else if lower.contains("unitofwork") {
        "Gestion transactionnelle des données"
    } else {
        "Service métier"
    }
}
