//! Git analytics pages generator (hotspots, coupling, ownership).

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use tracing::debug;

/// Generate hotspots, coupling, and ownership pages using code-explorer-git.
/// Returns the number of pages successfully generated (0-3).
pub(super) fn generate_git_analytics_pages(docs_dir: &Path, repo_path: &Path) -> Result<usize> {
    let mut count = 0;

    // ── Hotspots ──
    match code_explorer_git::hotspots::analyze_hotspots(repo_path, 90) {
        Ok(hotspots) if !hotspots.is_empty() => {
            let out_path = docs_dir.join("hotspots.md");
            let mut f = std::fs::File::create(&out_path)?;

            writeln!(f, "# Code Hotspots")?;
            writeln!(f, "<!-- GNX:LEAD -->")?;
            writeln!(f)?;
            writeln!(
                f,
                "> Fichiers les plus fréquemment modifiés ces 90 derniers jours."
            )?;
            writeln!(f, "> Un hotspot élevé signale un fichier qui change souvent — risque de régressions, dette technique ou logique métier centrale.")?;
            writeln!(f)?;

            writeln!(f, "## Top 20 fichiers les plus modifiés")?;
            writeln!(f)?;
            writeln!(f, "| # | Fichier | Commits | Churn | Auteurs | Score |")?;
            writeln!(f, "|---|---------|---------|-------|---------|-------|")?;
            for (i, h) in hotspots.iter().take(20).enumerate() {
                let short_path = h.path.replace('\\', "/");
                let bar = "\u{2588}".repeat((h.score * 10.0) as usize);
                writeln!(
                    f,
                    "| {} | `{}` | {} | +{}/-{} | {} | {} {:.0}% |",
                    i + 1,
                    short_path,
                    h.commit_count,
                    h.lines_added,
                    h.lines_removed,
                    h.author_count,
                    bar,
                    h.score * 100.0
                )?;
            }
            writeln!(f)?;

            // Interpretation
            writeln!(f, "## Interprétation")?;
            writeln!(f)?;
            let top3: Vec<_> = hotspots.iter().take(3).collect();
            if !top3.is_empty() {
                writeln!(f, "Les fichiers les plus chauds sont :")?;
                for h in &top3 {
                    writeln!(
                        f,
                        "- **`{}`** — {} commits, churn {} lignes, {} auteurs",
                        h.path.replace('\\', "/"),
                        h.commit_count,
                        h.churn,
                        h.author_count
                    )?;
                }
                writeln!(f)?;
                writeln!(f, "> **Recommandation :** Les fichiers avec un score >70% et >3 auteurs sont des candidats prioritaires pour du refactoring ou de meilleurs tests.")?;
            }
            writeln!(f)?;

            println!(
                "  {} hotspots.md ({} fichiers)",
                "OK".green(),
                hotspots.len().min(20)
            );
            count += 1;
        }
        Ok(_) => {
            debug!("No hotspots found, skipping page");
        }
        Err(e) => {
            debug!("Could not analyze hotspots: {}", e);
        }
    }

    // ── Temporal Coupling ──
    match code_explorer_git::coupling::analyze_coupling(repo_path, 3, Some(180)) {
        Ok(couplings) if !couplings.is_empty() => {
            let out_path = docs_dir.join("coupling.md");
            let mut f = std::fs::File::create(&out_path)?;

            writeln!(f, "# Temporal Coupling")?;
            writeln!(f, "<!-- GNX:LEAD -->")?;
            writeln!(f)?;
            writeln!(f, "> Paires de fichiers qui changent toujours ensemble.")?;
            writeln!(f, "> Un couplage temporel élevé peut indiquer une dépendance implicite non visible dans le code.")?;
            writeln!(f)?;

            writeln!(f, "## Paires les plus couplées")?;
            writeln!(f)?;
            writeln!(
                f,
                "| # | Fichier A | Fichier B | Commits partagés | Force |"
            )?;
            writeln!(f, "|---|-----------|-----------|-----------------|-------|")?;
            for (i, c) in couplings.iter().take(20).enumerate() {
                let bar = "\u{2588}".repeat((c.coupling_strength * 10.0) as usize);
                writeln!(
                    f,
                    "| {} | `{}` | `{}` | {} | {} {:.0}% |",
                    i + 1,
                    c.file_a.replace('\\', "/"),
                    c.file_b.replace('\\', "/"),
                    c.shared_commits,
                    bar,
                    c.coupling_strength * 100.0
                )?;
            }
            writeln!(f)?;

            writeln!(f, "## Interprétation")?;
            writeln!(f)?;
            let strong: Vec<_> = couplings
                .iter()
                .filter(|c| c.coupling_strength > 0.7)
                .collect();
            if !strong.is_empty() {
                writeln!(
                    f,
                    "**{} paires fortement couplées** (>70%) détectées :",
                    strong.len()
                )?;
                writeln!(f)?;
                for c in strong.iter().take(5) {
                    writeln!(
                        f,
                        "- `{}` \u{2194} `{}` ({:.0}%)",
                        c.file_a.replace('\\', "/"),
                        c.file_b.replace('\\', "/"),
                        c.coupling_strength * 100.0
                    )?;
                }
                writeln!(f)?;
                writeln!(f, "> **Recommandation :** Un couplage >70% suggère que ces fichiers devraient peut-être être fusionnés, ou qu'une abstraction commune manque.")?;
            } else {
                writeln!(f, "Aucune paire n'est couplée à plus de 70%. Le codebase a un couplage temporel raisonnable.")?;
            }
            writeln!(f)?;

            println!(
                "  {} coupling.md ({} paires)",
                "OK".green(),
                couplings.len().min(20)
            );
            count += 1;
        }
        Ok(_) => {
            debug!("No coupling data found, skipping page");
        }
        Err(e) => {
            debug!("Could not analyze coupling: {}", e);
        }
    }

    // ── Code Ownership ──
    match code_explorer_git::ownership::analyze_ownership(repo_path) {
        Ok(ownerships) if !ownerships.is_empty() => {
            let out_path = docs_dir.join("ownership.md");
            let mut f = std::fs::File::create(&out_path)?;

            writeln!(f, "# Code Ownership")?;
            writeln!(f, "<!-- GNX:LEAD -->")?;
            writeln!(f)?;
            writeln!(
                f,
                "> Répartition de la propriété du code par auteur principal."
            )?;
            writeln!(f, "> Les fichiers avec un ownership faible (<50%) ou beaucoup d'auteurs indiquent un manque de propriétaire clair.")?;
            writeln!(f)?;

            // Group by primary author
            let mut by_author: BTreeMap<String, Vec<&code_explorer_git::types::FileOwnership>> =
                BTreeMap::new();
            for o in &ownerships {
                by_author
                    .entry(o.primary_author.clone())
                    .or_default()
                    .push(o);
            }

            writeln!(f, "## Résumé par auteur")?;
            writeln!(f)?;
            writeln!(f, "| Auteur | Fichiers possédés | Ownership moyen |")?;
            writeln!(f, "|--------|-------------------|-----------------|")?;
            let mut author_stats: Vec<_> = by_author
                .iter()
                .map(|(author, files)| {
                    let avg_pct =
                        files.iter().map(|f| f.ownership_pct).sum::<f64>() / files.len() as f64;
                    (author.clone(), files.len(), avg_pct)
                })
                .collect();
            author_stats.sort_by(|a, b| b.1.cmp(&a.1));
            for (author, file_count, avg_pct) in &author_stats {
                writeln!(f, "| {} | {} | {:.0}% |", author, file_count, avg_pct)?;
            }
            writeln!(f)?;

            writeln!(f, "## Fichiers à risque (ownership < 50%)")?;
            writeln!(f)?;
            let low_ownership: Vec<_> = ownerships
                .iter()
                .filter(|o| o.ownership_pct < 50.0)
                .collect();
            if low_ownership.is_empty() {
                writeln!(
                    f,
                    "Tous les fichiers ont un propriétaire clair (>50%). Bonne pratique."
                )?;
            } else {
                writeln!(f, "| Fichier | Auteur principal | Ownership | Auteurs |")?;
                writeln!(f, "|---------|-----------------|-----------|---------|")?;
                for o in low_ownership.iter().take(20) {
                    writeln!(
                        f,
                        "| `{}` | {} | {:.0}% | {} |",
                        o.path.replace('\\', "/"),
                        o.primary_author,
                        o.ownership_pct,
                        o.author_count
                    )?;
                }
                writeln!(f)?;
                writeln!(f, "> **Recommandation :** Ces {} fichiers n'ont pas de propriétaire clair. Assigner un responsable réduit le risque de régressions.", low_ownership.len())?;
            }
            writeln!(f)?;

            writeln!(f, "## Top 20 fichiers les plus distribués")?;
            writeln!(f)?;
            writeln!(f, "| # | Fichier | Auteurs | Ownership principal |")?;
            writeln!(f, "|---|---------|---------|---------------------|")?;
            // Sorted by author_count desc (most distributed first)
            let mut sorted_own = ownerships.clone();
            sorted_own.sort_by(|a, b| b.author_count.cmp(&a.author_count));
            for (i, o) in sorted_own.iter().take(20).enumerate() {
                writeln!(
                    f,
                    "| {} | `{}` | {} | {} ({:.0}%) |",
                    i + 1,
                    o.path.replace('\\', "/"),
                    o.author_count,
                    o.primary_author,
                    o.ownership_pct
                )?;
            }
            writeln!(f)?;

            println!(
                "  {} ownership.md ({} fichiers)",
                "OK".green(),
                ownerships.len().min(20)
            );
            count += 1;
        }
        Ok(_) => {
            debug!("No ownership data found, skipping page");
        }
        Err(e) => {
            debug!("Could not analyze ownership: {}", e);
        }
    }

    if count > 0 {
        println!("{} Generated {} git analytics pages", "OK".green(), count);
    }

    Ok(count)
}
