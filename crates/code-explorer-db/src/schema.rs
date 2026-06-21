//! Schema DDL generation for the Code Explorer knowledge graph database.
//!
//! Generates CREATE NODE TABLE and CREATE REL TABLE statements compatible
//! with KuzuDB's Cypher dialect. Also generates FTS index creation queries
//! for full-text search on key tables.

/// All node table labels that have their own table in the database.
/// Each table shares a common set of base columns plus type-specific extras.
pub const NODE_LABELS: &[&str] = &[
    "File",
    "Folder",
    "Function",
    "Class",
    "Method",
    "Variable",
    "Interface",
    "Enum",
    "Community",
    "Process",
    "Project",
    "Package",
    "Module",
    "Decorator",
    "Import",
    "Type",
    "CodeElement",
    "Struct",
    "Macro",
    "Typedef",
    "Union",
    "Namespace",
    "Trait",
    "Impl",
    "TypeAlias",
    "Const",
    "Static",
    "Property",
    "Record",
    "Delegate",
    "Annotation",
    "Constructor",
    "Template",
    "Section",
    "Route",
    "Tool",
    "Library",
    // ASP.NET MVC 5 / EF6
    "Controller",
    "ControllerAction",
    "ApiEndpoint",
    "View",
    "ViewModel",
    "DbEntity",
    "DbContext",
    "Area",
    // ASP.NET additional node types
    "Filter",
    "WebConfig",
    "PartialView",
    "ScriptFile",
    "AjaxCall",
    "UiComponent",
    "Service",
    "Repository",
    "ExternalService",
    // GraphRAG (Documentation)
    "Document",
    "DocChunk",
];

/// Base columns shared by all node tables.
const BASE_COLUMNS: &str = "\
    id STRING PRIMARY KEY, \
    name STRING, \
    filePath STRING, \
    content STRING, \
    startLine INT32, \
    endLine INT32, \
    language STRING, \
    isExported BOOLEAN";

/// Generate all schema DDL statements (CREATE NODE TABLE + CREATE REL TABLE).
pub fn schema_queries() -> Vec<String> {
    let mut queries = Vec::with_capacity(NODE_LABELS.len() + 1);

    for label in NODE_LABELS {
        let extra = extra_columns_for(label);
        let ddl = if extra.is_empty() {
            format!("CREATE NODE TABLE IF NOT EXISTS {label} ({BASE_COLUMNS})")
        } else {
            format!("CREATE NODE TABLE IF NOT EXISTS {label} ({BASE_COLUMNS}, {extra})")
        };
        queries.push(ddl);
    }

    // CodeRelation: edges between any node tables
    // FROM/TO use a union of all node tables
    let from_tables: String = NODE_LABELS.join(", ");
    let to_tables: String = NODE_LABELS.join(", ");
    queries.push(format!(
        "CREATE REL TABLE IF NOT EXISTS CodeRelation (\
            FROM [{from_tables}] TO [{to_tables}], \
            type STRING, \
            confidence DOUBLE, \
            reason STRING, \
            step INT32)"
    ));

    queries
}

/// Generate FTS index creation queries for the 5 searchable tables.
pub fn fts_queries() -> Vec<String> {
    let fts_tables = [
        "File",
        "Function",
        "Class",
        "Method",
        "Interface",
        "Controller",
        "ControllerAction",
        "ApiEndpoint",
        "View",
        "ViewModel",
        "DbEntity",
        "DbContext",
        "ScriptFile",
        "UiComponent",
        "Service",
        "Repository",
        "ExternalService",
        "Document",
        "DocChunk",
    ];
    fts_tables
        .iter()
        .map(|table| {
            format!(
                "CALL CREATE_FTS_INDEX('{table}', 'fts_{table}', ['name', 'content', 'filePath'])"
            )
        })
        .collect()
}

/// Return extra columns (beyond the base set) for a given node label.
fn extra_columns_for(label: &str) -> String {
    match label {
        "Community" => [
            "heuristicLabel STRING",
            "cohesion DOUBLE",
            "symbolCount INT32",
            "keywords STRING",
            "description STRING",
            "enrichedBy STRING",
        ]
        .join(", "),
        "Process" => [
            "processType STRING",
            "stepCount INT32",
            "communities STRING",
            "entryPointId STRING",
            "terminalId STRING",
            "description STRING",
            "enrichedBy STRING",
        ]
        .join(", "),
        "Function" | "Method" | "Constructor" => [
            "parameterCount INT32",
            "returnType STRING",
            "entryPointScore DOUBLE",
            "entryPointReason STRING",
            "astFrameworkMultiplier DOUBLE",
            "astFrameworkReason STRING",
        ]
        .join(", "),
        "Class" | "Interface" | "Struct" | "Trait" | "Record" => [
            "parameterCount INT32",
            "entryPointScore DOUBLE",
            "entryPointReason STRING",
        ]
        .join(", "),
        "Route" => [
            "httpMethod STRING",
            "routePath STRING",
            "responseKeys STRING",
            "errorKeys STRING",
            "middleware STRING",
        ]
        .join(", "),
        "Tool" => ["toolDescription STRING", "inputSchema STRING"].join(", "),
        "Section" => "level INT32".to_string(),
        "Variable" | "Const" | "Static" | "Property" => "returnType STRING".to_string(),
        "Enum" => "members STRING".to_string(),
        "Import" => [
            "importPath STRING",
            "importedNames STRING",
            "isDefault BOOLEAN",
        ]
        .join(", "),
        "Decorator" | "Annotation" => "decoratorArgs STRING".to_string(),
        "Impl" => "traitName STRING, targetType STRING".to_string(),
        "TypeAlias" | "Typedef" => "aliasedType STRING".to_string(),
        "Macro" => "macroKind STRING".to_string(),
        "Namespace" | "Module" | "Package" | "Project" => "description STRING".to_string(),
        "Template" => "templateEngine STRING".to_string(),
        "Delegate" => "delegateSignature STRING".to_string(),
        "Union" => "members STRING".to_string(),
        // ASP.NET MVC 5 / EF6 tables
        "Controller" => [
            "areaName STRING",
            "routeTemplate STRING",
            "entryPointScore DOUBLE",
            "entryPointReason STRING",
        ]
        .join(", "),
        "ControllerAction" => [
            "httpMethod STRING",
            "routeTemplate STRING",
            "modelType STRING",
            "returnType STRING",
        ]
        .join(", "),
        "ApiEndpoint" => [
            "httpMethod STRING",
            "routeTemplate STRING",
            "modelType STRING",
            "returnType STRING",
            "responseKeys STRING",
            "errorKeys STRING",
        ]
        .join(", "),
        "View" => [
            "viewEngine STRING",
            "layoutPath STRING",
            "modelType STRING",
            "areaName STRING",
        ]
        .join(", "),
        "ViewModel" => "dataAnnotations STRING".to_string(),
        "DbEntity" => ["dbTableName STRING", "dataAnnotations STRING"].join(", "),
        "DbContext" => "connectionStringName STRING".to_string(),
        "Area" => "areaName STRING".to_string(),
        "AjaxCall" => "ajaxMethod STRING, ajaxUrl STRING".to_string(),
        "UiComponent" => "componentType STRING, boundModel STRING".to_string(),
        "Service" | "Repository" => "layerType STRING, implementsInterface STRING".to_string(),
        "ExternalService" => "serviceType STRING".to_string(),
        // GraphRAG document types
        "Document" => "title STRING, sourceUrl STRING".to_string(),
        "DocChunk" => "title STRING, pageNumber INT32, embedding STRING".to_string(),
        // ScriptFile uses base columns only
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_queries_count() {
        let queries = schema_queries();
        // 36 node tables + 1 rel table = 37
        assert_eq!(queries.len(), NODE_LABELS.len() + 1);
    }

    #[test]
    fn test_schema_queries_contain_create() {
        for q in schema_queries() {
            assert!(
                q.starts_with("CREATE NODE TABLE") || q.starts_with("CREATE REL TABLE"),
                "Unexpected query: {q}"
            );
        }
    }

    #[test]
    fn test_fts_queries_count() {
        let queries = fts_queries();
        assert_eq!(queries.len(), 19);
    }

    #[test]
    fn test_community_has_extra_columns() {
        let queries = schema_queries();
        let community_q = queries.iter().find(|q| q.contains("Community")).unwrap();
        assert!(community_q.contains("heuristicLabel"));
        assert!(community_q.contains("cohesion"));
    }

    #[test]
    fn test_code_relation_references_all_tables() {
        let queries = schema_queries();
        let rel_q = queries.last().unwrap();
        assert!(rel_q.contains("CodeRelation"));
        assert!(rel_q.contains("confidence DOUBLE"));
        for label in NODE_LABELS {
            assert!(rel_q.contains(label), "CodeRelation missing {label}");
        }
    }
}
