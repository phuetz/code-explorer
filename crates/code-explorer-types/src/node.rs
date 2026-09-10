use serde::{Deserialize, Serialize};
use crate::config::SupportedLanguage;

/// All possible node types in the knowledge graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeLabel {
    Project, Package, Module, Folder, File, Class, Function, Method,
    Variable, Interface, Enum, Decorator, Import, Type, CodeElement,
    Community, Process, Struct, Macro, Typedef, Union, Namespace,
    Trait, Impl, TypeAlias, Const, Static, Property, Record,
    Delegate, Annotation, Constructor, Template, Section, Route, Tool,
    // New: Code Property Graph types
    BasicBlock, BranchPoint, LoopHead, ExitPoint,
    // New: Git behavioral types
    Author,
    // New: External library / UI component library
    Library,
}

impl NodeLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "Project", Self::Package => "Package",
            Self::Module => "Module", Self::Folder => "Folder",
            Self::File => "File", Self::Class => "Class",
            Self::Function => "Function", Self::Method => "Method",
            Self::Variable => "Variable", Self::Interface => "Interface",
            Self::Enum => "Enum", Self::Decorator => "Decorator",
            Self::Import => "Import", Self::Type => "Type",
            Self::CodeElement => "CodeElement", Self::Community => "Community",
            Self::Process => "Process", Self::Struct => "Struct",
            Self::Macro => "Macro", Self::Typedef => "Typedef",
            Self::Union => "Union", Self::Namespace => "Namespace",
            Self::Trait => "Trait", Self::Impl => "Impl",
            Self::TypeAlias => "TypeAlias", Self::Const => "Const",
            Self::Static => "Static", Self::Property => "Property",
            Self::Record => "Record", Self::Delegate => "Delegate",
            Self::Annotation => "Annotation", Self::Constructor => "Constructor",
            Self::Template => "Template", Self::Section => "Section",
            Self::Route => "Route", Self::Tool => "Tool",
            Self::BasicBlock => "BasicBlock", Self::BranchPoint => "BranchPoint",
            Self::LoopHead => "LoopHead", Self::ExitPoint => "ExitPoint",
            Self::Author => "Author",
            Self::Library => "Library",
        }
    }

    pub fn from_str_label(s: &str) -> Option<Self> {
        match s {
            "Project" => Some(Self::Project), "Package" => Some(Self::Package),
            "Module" => Some(Self::Module), "Folder" => Some(Self::Folder),
            "File" => Some(Self::File), "Class" => Some(Self::Class),
            "Function" => Some(Self::Function), "Method" => Some(Self::Method),
            "Variable" => Some(Self::Variable), "Interface" => Some(Self::Interface),
            "Enum" => Some(Self::Enum), "Decorator" => Some(Self::Decorator),
            "Import" => Some(Self::Import), "Type" => Some(Self::Type),
            "CodeElement" => Some(Self::CodeElement), "Community" => Some(Self::Community),
            "Process" => Some(Self::Process), "Struct" => Some(Self::Struct),
            "Macro" => Some(Self::Macro), "Typedef" => Some(Self::Typedef),
            "Union" => Some(Self::Union), "Namespace" => Some(Self::Namespace),
            "Trait" => Some(Self::Trait), "Impl" => Some(Self::Impl),
            "TypeAlias" => Some(Self::TypeAlias), "Const" => Some(Self::Const),
            "Static" => Some(Self::Static), "Property" => Some(Self::Property),
            "Record" => Some(Self::Record), "Delegate" => Some(Self::Delegate),
            "Annotation" => Some(Self::Annotation), "Constructor" => Some(Self::Constructor),
            "Template" => Some(Self::Template), "Section" => Some(Self::Section),
            "Route" => Some(Self::Route), "Tool" => Some(Self::Tool),
            "BasicBlock" => Some(Self::BasicBlock), "BranchPoint" => Some(Self::BranchPoint),
            "LoopHead" => Some(Self::LoopHead), "ExitPoint" => Some(Self::ExitPoint),
            "Author" => Some(Self::Author),
            "Library" => Some(Self::Library),
            _ => None,
        }
    }

    pub fn is_callable(&self) -> bool {
        matches!(self, Self::Function | Self::Method | Self::Constructor)
    }
}

impl std::fmt::Display for NodeLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnrichedBy { Heuristic, Llm }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessType { IntraCommunity, CrossCommunity }

/// Properties attached to a graph node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeProperties {
    pub name: String,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<SupportedLanguage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_exported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ast_framework_multiplier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ast_framework_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heuristic_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cohesion: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enriched_by: Option<EnrichedBy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_type: Option<ProcessType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub communities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_point_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_keys: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_keys: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middleware: Option<Vec<String>>,
    // Git behavioral properties
    #[serde(skip_serializing_if = "Option::is_none")]
    pub churn_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_age_days: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volatility_score: Option<f64>,
    // Taint tracking (CPG)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taint_source: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taint_sink: Option<bool>,
}

/// A node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: NodeLabel,
    pub properties: NodeProperties,
}
