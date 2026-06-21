use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessFlow {
    pub id: String,
    pub name: String,
    pub process_type: String,
    pub step_count: u32,
    pub steps: Vec<ProcessStep>,
    pub mermaid: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStep {
    pub node_id: String,
    pub name: String,
    pub label: String,
    pub file_path: String,
}

#[tauri::command]
pub async fn get_process_flows(state: State<'_, AppState>) -> Result<Vec<ProcessFlow>, String> {
    let (graph, indexes, _, _) = state.get_repo(None).await?;

    let mut flows = Vec::new();

    // Find Process-labeled nodes
    for node in graph.iter_nodes() {
        if node.label != code_explorer_core::graph::types::NodeLabel::Process {
            continue;
        }

        let name = node.properties.name.clone();
        // `ProcessType` derives `Serialize` with `rename_all = "snake_case"`,
        // so the snapshot's JSON form is "intra_community" / "cross_community".
        // The Debug-formatted variant ("IntraCommunity" / "CrossCommunity") was
        // a separate enum-name vocabulary that wouldn't match anything the
        // frontend reads from the snapshot, breaking process-type filtering.
        let process_type = node
            .properties
            .process_type
            .as_ref()
            .and_then(|pt| serde_json::to_value(pt).ok())
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());

        // Follow CALLS relationships from entry_point_id to build step chain
        let mut steps = Vec::new();
        let entry_id = node
            .properties
            .entry_point_id
            .clone()
            .unwrap_or_else(|| node.id.clone());

        // BFS to collect ordered steps
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(entry_id.clone());

        while let Some(current_id) = queue.pop_front() {
            if !visited.insert(current_id.clone()) {
                continue;
            }
            if steps.len() >= 20 {
                break; // safety limit
            }

            if let Some(step_node) = graph.get_node(&current_id) {
                steps.push(ProcessStep {
                    node_id: step_node.id.clone(),
                    name: step_node.properties.name.clone(),
                    label: step_node.label.as_str().to_string(),
                    file_path: step_node.properties.file_path.clone(),
                });

                // Follow outgoing CALLS edges
                if let Some(outs) = indexes.outgoing.get(&current_id) {
                    for (target_id, rel_type) in outs {
                        if matches!(
                            rel_type,
                            code_explorer_core::graph::types::RelationshipType::Calls
                                | code_explorer_core::graph::types::RelationshipType::CallsAction
                        ) && !visited.contains(target_id)
                        {
                            queue.push_back(target_id.clone());
                        }
                    }
                }
            }
        }

        // Generate Mermaid flowchart.
        //
        // Mermaid labels inside `["..."]` don't understand `\"` — the only
        // working escape is HTML entities. A previous pass only replaced `"`
        // and backtick with `'`, so step names containing `[`, `]`, `<`, `>`,
        // or `&` (very common: C# generics like `List<string>`, operator
        // overloads, indexers) broke the rendered diagram. Sanitize the same
        // way the sibling `diagram.rs` command does.
        let escape_mermaid_label = |s: &str| -> String {
            s.replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('[', "&#91;")
                .replace(']', "&#93;")
                .replace('(', "&#40;")
                .replace(')', "&#41;")
                .replace('`', "&#96;")
                .replace('{', "&#123;")
                .replace('}', "&#125;")
        };

        // Map real graph node_id → synthetic Mermaid id (`S0`, `S1`, ...).
        // The previous implementation assumed the BFS discovery order formed
        // a linear chain and drew `S0 --> S1 --> S2 --> …`, which is only
        // correct if every step has exactly one outgoing call. As soon as
        // a method fans out (e.g. `A→B, A→D, B→C`), BFS produces the order
        // `[A, B, D, C]` and the rendered diagram `A→B→D→C` is a structural
        // lie. Emit the actual Calls/CallsAction edges that exist in the
        // graph between collected steps instead.
        let step_id_map: std::collections::HashMap<String, String> = steps
            .iter()
            .enumerate()
            .map(|(i, s)| (s.node_id.clone(), format!("S{}", i)))
            .collect();

        let mut mermaid = String::from("graph TD\n");
        // Node definitions
        for (i, step) in steps.iter().enumerate() {
            let safe_name = escape_mermaid_label(&step.name);
            let safe_label = escape_mermaid_label(&step.label);
            mermaid.push_str(&format!("    S{}[\"{} ({})\"]\n", i, safe_name, safe_label));
        }
        // Real graph edges between collected steps
        for step in &steps {
            if let Some(outs) = indexes.outgoing.get(step.node_id.as_str()) {
                for (target_id, rel_type) in outs {
                    if !matches!(
                        rel_type,
                        code_explorer_core::graph::types::RelationshipType::Calls
                            | code_explorer_core::graph::types::RelationshipType::CallsAction
                    ) {
                        continue;
                    }
                    if let (Some(src_mid), Some(dst_mid)) =
                        (step_id_map.get(&step.node_id), step_id_map.get(target_id))
                    {
                        mermaid.push_str(&format!("    {} --> {}\n", src_mid, dst_mid));
                    }
                }
            }
        }

        if !steps.is_empty() {
            // step_count reflects the actually collected steps (post-BFS, post-cap),
            // not the stored property which may diverge from what we display.
            let step_count = steps.len() as u32;
            flows.push(ProcessFlow {
                id: node.id.clone(),
                name,
                process_type,
                step_count,
                steps,
                mermaid,
            });
        }
    }

    // Sort by step count desc
    flows.sort_by(|a, b| b.step_count.cmp(&a.step_count));

    // Add synthetic business flows (Heuristic-based for Sample)
    add_synthetic_business_flows(&graph, &mut flows);

    Ok(flows)
}

fn add_synthetic_business_flows(
    graph: &code_explorer_core::graph::KnowledgeGraph,
    flows: &mut Vec<ProcessFlow>,
) {
    // Courriers
    if graph
        .iter_nodes()
        .any(|n| n.properties.name.contains("Courrier"))
    {
        flows.push(ProcessFlow {
            id: "biz-courriers".into(),
            name: "Système de Courriers".into(),
            process_type: "business".into(),
            step_count: 5,
            steps: Vec::new(),
            mermaid: r#"sequenceDiagram
    participant U as Utilisateur
    participant C as CourrierController
    participant R as RegleCourrierMasse
    participant G as CourrierGenerer
    participant PDF as Aspose.Words
    U->>C: Sélection type + modèle
    C->>R: GetTypeDestinataire()
    R-->>C: Fournisseur/Dossier/Bénéficiaire
    U->>C: Sélection + Imprimer
    C->>R: PrepareCreationCourrierMasse()
    loop Pour chaque destinataire
        R->>G: GenererInfoCourrier()
        G->>PDF: Mail merge variables ELODIE
        G->>G: Sauver en base
    end
    G-->>U: Téléchargement PDF unique"#
                .into(),
        });
    }

    // Paiements
    if graph
        .iter_nodes()
        .any(|n| n.properties.name.contains("Reglement") || n.properties.name.contains("Facture"))
    {
        flows.push(ProcessFlow {
            id: "biz-paiements".into(),
            name: "Cycle de Paiement (Facture -> ELODIE)".into(),
            process_type: "business".into(),
            step_count: 6,
            steps: Vec::new(),
            mermaid: r#"stateDiagram-v2
    [*] --> DemPaiemVal : Création facture
    DemPaiemVal --> DemPaiemCtrler : Contrôle
    DemPaiemVal --> DemGrPrVal : Groupement
    DemGrPrVal --> DemTransmiseELODIE : Fonds nationaux
    DemGrPrVal --> BordereauEditeFP : Fonds propres
    DemTransmiseELODIE --> PaiementRegle : Règlement final
    BordereauEditeFP --> PaiementRegle : Règlement final"#
                .into(),
        });
    }

    // Barèmes
    if graph
        .iter_nodes()
        .any(|n| n.properties.name.contains("Bareme"))
    {
        flows.push(ProcessFlow {
            id: "biz-baremes".into(),
            name: "Moteur de Calcul des Barèmes".into(),
            process_type: "business".into(),
            step_count: 4,
            steps: Vec::new(),
            mermaid: r#"flowchart TD
    A["Ressources annuelles"] --> B["÷ Nombre de parts"]
    B --> C["Ressource comparable"]
    C --> D{Match Tranche ?}
    D -->|Oui| J["TauxFASS = TRA_TAUX_SERVI"]
    D -->|Non| K["Hors barème / Taux min"]"#
                .into(),
        });
    }
}
