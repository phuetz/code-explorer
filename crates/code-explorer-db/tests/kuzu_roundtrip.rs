#![cfg(feature = "kuzu-backend")]

use code_explorer_core::graph::types::{
    GraphNode, GraphRelationship, NodeLabel, NodeProperties, RelationshipType,
};
use code_explorer_core::graph::KnowledgeGraph;
use code_explorer_db::adapter::DbAdapter;
use code_explorer_db::csv_generator::generate_all_csvs;

#[test]
fn kuzu_import_preserves_nodes_edges_fts_and_reindexing() {
    let temp = tempfile::tempdir().unwrap();
    // A quote in the path and doubled quotes/newlines in source exercise COPY escaping.
    let root = temp.path().join("source's folder");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("sample.py"), "# café\n\"quoted\"\n").unwrap();
    let csv_dir = root.join("csv");
    let mut graph = KnowledgeGraph::new();
    for (id, label, name) in [
        ("entry", NodeLabel::Function, "authentication"),
        ("macro", NodeLabel::Macro, "reserved keyword"),
        ("todo", NodeLabel::TodoMarker, "TODO"),
        ("column", NodeLabel::DbColumn, "column"),
        ("env", NodeLabel::EnvVar, "CONFIG"),
    ] {
        graph.add_node(GraphNode {
            id: id.into(),
            label,
            properties: NodeProperties {
                name: name.into(),
                file_path: "sample.py".into(),
                start_line: Some(1),
                end_line: Some(2),
                ..Default::default()
            },
        });
    }
    for (id, from, to) in [("r1", "entry", "macro"), ("r2", "todo", "entry")] {
        graph.add_relationship(GraphRelationship {
            id: id.into(),
            source_id: from.into(),
            target_id: to.into(),
            rel_type: RelationshipType::Calls,
            confidence: 0.75,
            reason: "quoted \"reason\"\nsecond line".into(),
            step: Some(3),
        });
    }
    generate_all_csvs(&graph, &root, &csv_dir).unwrap();
    let db_path = temp.path().join("graph.db");
    let mut db = DbAdapter::new_kuzu();
    db.open(&db_path).unwrap();
    db.create_schema().unwrap();
    db.bulk_load_csv(&csv_dir).unwrap();
    let rows = db
        .execute_query("MATCH (n:Function) RETURN n.name AS name, n.content AS content")
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "authentication");
    assert_eq!(rows[0]["content"], "# café\n\"quoted\"");
    let rows = db.execute_query(
        "MATCH (a:Function)-[r:CodeRelation]->(b:`Macro`) RETURN r.confidence AS confidence, r.reason AS reason, r.step AS step"
    ).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["confidence"], 0.75);
    assert_eq!(rows[0]["reason"], "quoted \"reason\"\nsecond line");
    assert_eq!(rows[0]["step"], 3);
    assert_eq!(
        db.execute_query("MATCH (a:TodoMarker)-[:CodeRelation]->(b:Function) RETURN b.name")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db.execute_query(
            "CALL QUERY_FTS_INDEX('Function', 'fts_Function', 'authentication') RETURN node.name"
        )
        .unwrap()
        .len(),
        1
    );
    db.close().unwrap();
    db.open(&db_path).unwrap();
    assert_eq!(
        db.execute_query("MATCH (n:EnvVar) RETURN n.name")
            .unwrap()
            .len(),
        1
    );

    // An unchanged second import must not duplicate primary keys or relations.
    db.create_schema().unwrap();
    db.bulk_load_csv(&csv_dir).unwrap();
    assert_eq!(
        db.execute_query("MATCH (a:Function)-[:CodeRelation]->(b:`Macro`) RETURN b.name")
            .unwrap()
            .len(),
        1
    );

    // Regenerating CSVs must remove tables no longer present in the graph.
    let mut replacement = KnowledgeGraph::new();
    replacement.add_node(GraphNode {
        id: "entry".into(),
        label: NodeLabel::Function,
        properties: NodeProperties {
            name: "replacement".into(),
            ..Default::default()
        },
    });
    generate_all_csvs(&replacement, &root, &csv_dir).unwrap();
    db.bulk_load_csv(&csv_dir).unwrap();
    assert!(db
        .execute_query("MATCH (n:`Macro`) RETURN n.name")
        .unwrap()
        .is_empty());
    assert_eq!(
        db.execute_query("MATCH (n:Function) RETURN n.name AS name")
            .unwrap()[0]["name"],
        "replacement"
    );
    assert!(db
        .execute_query(
            "CALL QUERY_FTS_INDEX('Function', 'fts_Function', 'authentication') RETURN node.name"
        )
        .unwrap()
        .is_empty());
    db.close().unwrap();
}
