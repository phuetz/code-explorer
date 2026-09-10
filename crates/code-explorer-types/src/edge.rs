use serde::{Deserialize, Serialize};

/// All possible relationship types in the knowledge graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationshipType {
    Contains, Calls, Inherits, Overrides, Imports, Uses, Defines,
    Decorates, Implements, Extends, HasMethod, HasProperty, Accesses,
    MemberOf, StepInProcess, HandlesRoute, Fetches, HandlesTool,
    EntryPointOf, Wraps,
    // New: Code Property Graph edges
    FlowsTo, BranchesTo, LoopsBack,
    DefinesVar, ReadsVar, DataFlowsTo, ControlDependsOn,
    // New: Git behavioral edges
    ChangesWith, AuthoredBy,
}

impl RelationshipType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Contains => "CONTAINS", Self::Calls => "CALLS",
            Self::Inherits => "INHERITS", Self::Overrides => "OVERRIDES",
            Self::Imports => "IMPORTS", Self::Uses => "USES",
            Self::Defines => "DEFINES", Self::Decorates => "DECORATES",
            Self::Implements => "IMPLEMENTS", Self::Extends => "EXTENDS",
            Self::HasMethod => "HAS_METHOD", Self::HasProperty => "HAS_PROPERTY",
            Self::Accesses => "ACCESSES", Self::MemberOf => "MEMBER_OF",
            Self::StepInProcess => "STEP_IN_PROCESS",
            Self::HandlesRoute => "HANDLES_ROUTE", Self::Fetches => "FETCHES",
            Self::HandlesTool => "HANDLES_TOOL", Self::EntryPointOf => "ENTRY_POINT_OF",
            Self::Wraps => "WRAPS",
            Self::FlowsTo => "FLOWS_TO", Self::BranchesTo => "BRANCHES_TO",
            Self::LoopsBack => "LOOPS_BACK",
            Self::DefinesVar => "DEFINES_VAR", Self::ReadsVar => "READS_VAR",
            Self::DataFlowsTo => "DATA_FLOWS_TO",
            Self::ControlDependsOn => "CONTROL_DEPENDS_ON",
            Self::ChangesWith => "CHANGES_WITH", Self::AuthoredBy => "AUTHORED_BY",
        }
    }

    pub fn from_str_type(s: &str) -> Option<Self> {
        match s {
            "CONTAINS" => Some(Self::Contains), "CALLS" => Some(Self::Calls),
            "INHERITS" => Some(Self::Inherits), "OVERRIDES" => Some(Self::Overrides),
            "IMPORTS" => Some(Self::Imports), "USES" => Some(Self::Uses),
            "DEFINES" => Some(Self::Defines), "DECORATES" => Some(Self::Decorates),
            "IMPLEMENTS" => Some(Self::Implements), "EXTENDS" => Some(Self::Extends),
            "HAS_METHOD" => Some(Self::HasMethod), "HAS_PROPERTY" => Some(Self::HasProperty),
            "ACCESSES" => Some(Self::Accesses), "MEMBER_OF" => Some(Self::MemberOf),
            "STEP_IN_PROCESS" => Some(Self::StepInProcess),
            "HANDLES_ROUTE" => Some(Self::HandlesRoute), "FETCHES" => Some(Self::Fetches),
            "HANDLES_TOOL" => Some(Self::HandlesTool),
            "ENTRY_POINT_OF" => Some(Self::EntryPointOf), "WRAPS" => Some(Self::Wraps),
            "FLOWS_TO" => Some(Self::FlowsTo), "BRANCHES_TO" => Some(Self::BranchesTo),
            "LOOPS_BACK" => Some(Self::LoopsBack),
            "DEFINES_VAR" => Some(Self::DefinesVar), "READS_VAR" => Some(Self::ReadsVar),
            "DATA_FLOWS_TO" => Some(Self::DataFlowsTo),
            "CONTROL_DEPENDS_ON" => Some(Self::ControlDependsOn),
            "CHANGES_WITH" => Some(Self::ChangesWith), "AUTHORED_BY" => Some(Self::AuthoredBy),
            _ => None,
        }
    }
}

impl std::fmt::Display for RelationshipType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An edge in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphRelationship {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    #[serde(rename = "type")]
    pub rel_type: RelationshipType,
    pub confidence: f64,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
}
