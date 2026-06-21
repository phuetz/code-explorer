//! Entity Framework 6 `.edmx` file parser.
//!
//! Parses the EDMX XML (Entity Data Model) to extract:
//! - Entity types (CSDL conceptual model)
//! - Properties with types and facets (Nullable, MaxLength, etc.)
//! - Navigation properties (relationships between entities)
//! - Associations with cardinality (1:*, *:*, 1:1)
//! - EntitySet to EntityType mappings
//!
//! Uses lightweight regex-based parsing to avoid adding an XML dependency.
//! The EDMX format is well-structured enough for this approach.

// ─── Result Types ────────────────────────────────────────────────────────

/// Complete parsed EDMX model.
#[derive(Debug, Clone)]
pub struct EdmxModel {
    /// Namespace of the conceptual model
    pub namespace: Option<String>,
    /// Entity container name (maps to DbContext)
    pub container_name: Option<String>,
    /// All entity types
    pub entity_types: Vec<EdmxEntityType>,
    /// All associations (relationships)
    pub associations: Vec<EdmxAssociation>,
    /// EntitySet → EntityType mapping
    pub entity_sets: Vec<EdmxEntitySet>,
    /// Complex types (value objects)
    pub complex_types: Vec<EdmxComplexType>,
}

/// An entity type from the CSDL section.
#[derive(Debug, Clone)]
pub struct EdmxEntityType {
    /// Entity name (e.g., "Product")
    pub name: String,
    /// Base type if inherited
    pub base_type: Option<String>,
    /// Whether this is abstract
    pub is_abstract: bool,
    /// Primary key property names
    pub key_properties: Vec<String>,
    /// Scalar properties
    pub properties: Vec<EdmxProperty>,
    /// Navigation properties
    pub navigation_properties: Vec<EdmxNavigationProperty>,
}

/// A scalar property on an entity.
#[derive(Debug, Clone)]
pub struct EdmxProperty {
    /// Property name
    pub name: String,
    /// EDM type (String, Int32, Decimal, DateTime, etc.)
    pub edm_type: String,
    /// Whether nullable
    pub nullable: bool,
    /// Max length facet, if any
    pub max_length: Option<u32>,
    /// Precision for decimal types
    pub precision: Option<u32>,
    /// Scale for decimal types
    pub scale: Option<u32>,
    /// Whether this has a StoreGeneratedPattern (Identity, Computed)
    pub store_generated: Option<String>,
    /// Whether this is a concurrency token (ConcurrencyMode=Fixed)
    pub is_concurrency_token: bool,
}

/// A navigation property (relationship reference).
#[derive(Debug, Clone)]
pub struct EdmxNavigationProperty {
    /// Property name (e.g., "Orders")
    pub name: String,
    /// Association name (references an EdmxAssociation)
    pub relationship: String,
    /// Role name on the "from" end
    pub from_role: String,
    /// Role name on the "to" end
    pub to_role: String,
}

/// An association between two entities (with cardinality).
#[derive(Debug, Clone)]
pub struct EdmxAssociation {
    /// Association name (e.g., "FK_Order_Customer")
    pub name: String,
    /// First end
    pub end1: EdmxAssociationEnd,
    /// Second end
    pub end2: EdmxAssociationEnd,
    /// Referential constraint (FK), if any
    pub referential_constraint: Option<EdmxReferentialConstraint>,
}

/// One end of an association.
#[derive(Debug, Clone)]
pub struct EdmxAssociationEnd {
    /// Role name
    pub role: String,
    /// Entity type (e.g., "Self.Customer" or just "Customer")
    pub entity_type: String,
    /// Multiplicity: "1", "0..1", "*"
    pub multiplicity: String,
}

/// FK constraint on an association.
#[derive(Debug, Clone)]
pub struct EdmxReferentialConstraint {
    /// Principal entity role
    pub principal_role: String,
    /// Principal property (PK)
    pub principal_property: String,
    /// Dependent entity role
    pub dependent_role: String,
    /// Dependent property (FK)
    pub dependent_property: String,
}

/// EntitySet in the EntityContainer.
#[derive(Debug, Clone)]
pub struct EdmxEntitySet {
    /// Set name (e.g., "Products")
    pub name: String,
    /// Entity type name (e.g., "Self.Product" or "Product")
    pub entity_type: String,
}

/// A complex type (value object without identity).
#[derive(Debug, Clone)]
pub struct EdmxComplexType {
    pub name: String,
    pub properties: Vec<EdmxProperty>,
}

// ─── Parser ──────────────────────────────────────────────────────────────

/// Parse an EDMX file content into an EdmxModel.
///
/// This parser focuses on the CSDL (Conceptual Schema Definition Language)
/// section of the EDMX, which contains the entity model that maps to C# classes.
pub fn parse_edmx(content: &str) -> EdmxModel {
    let mut model = EdmxModel {
        namespace: None,
        container_name: None,
        entity_types: Vec::new(),
        associations: Vec::new(),
        entity_sets: Vec::new(),
        complex_types: Vec::new(),
    };

    // Extract the ConceptualModels section (CSDL)
    let csdl = extract_section(content, "edmx:ConceptualModels")
        .or_else(|| extract_section(content, "ConceptualModels"))
        .unwrap_or(content);

    // Extract namespace from Schema element
    model.namespace = extract_attr(csdl, "Schema", "Namespace");

    // Extract EntityContainer name
    model.container_name = extract_attr(csdl, "EntityContainer", "Name");

    // Parse EntityTypes
    model.entity_types = parse_entity_types(csdl);

    // Parse Associations
    model.associations = parse_associations(csdl);

    // Parse EntitySets
    model.entity_sets = parse_entity_sets(csdl);

    // Parse ComplexTypes
    model.complex_types = parse_complex_types(csdl);

    model
}

/// Derive a human-readable cardinality string from an association.
pub fn cardinality_str(assoc: &EdmxAssociation) -> String {
    let m1 = &assoc.end1.multiplicity;
    let m2 = &assoc.end2.multiplicity;
    format!("{}:{}", multiplicity_display(m1), multiplicity_display(m2))
}

fn multiplicity_display(m: &str) -> &str {
    match m {
        "1" => "1",
        "0..1" => "0..1",
        "*" => "*",
        _ => m,
    }
}

/// Clean entity type reference: "Self.Product" → "Product", "Model.Product" → "Product"
pub fn clean_entity_type_name(type_ref: &str) -> &str {
    type_ref.rsplit('.').next().unwrap_or(type_ref)
}

// ─── Internal Parsing Helpers ────────────────────────────────────────────

/// Extract a named XML section between opening and closing tags.
fn extract_section<'a>(content: &'a str, tag: &str) -> Option<&'a str> {
    let open_pattern = format!("<{}", tag);
    let close_pattern = format!("</{}>", tag);
    let start = content.find(&open_pattern)?;
    // Search for the closing tag *after* the opening tag. Without this, a
    // malformed (or maliciously crafted) EDMX file containing an orphan
    // `</Tag>` somewhere before the matching `<Tag` would make
    // `content[start..end + close_pattern.len()]` panic with
    // `begin <= end (32 <= 24)`. Restrict the search range so end is always
    // ≥ start, returning None when no balanced close is found.
    let end_relative = content[start..].find(&close_pattern)?;
    let end = start + end_relative;
    Some(&content[start..end + close_pattern.len()])
}

/// Extract an attribute value from the first occurrence of a tag.
fn extract_attr(content: &str, tag: &str, attr: &str) -> Option<String> {
    let tag_start = content.find(&format!("<{}", tag))?;
    let tag_line = &content[tag_start..];
    let tag_end = tag_line.find('>')?;
    let tag_content = &tag_line[..tag_end];

    let attr_pattern = format!("{}=\"", attr);
    let attr_start = tag_content.find(&attr_pattern)?;
    let value_start = attr_start + attr_pattern.len();
    let value_end = tag_content[value_start..].find('"')?;
    Some(tag_content[value_start..value_start + value_end].to_string())
}

/// Parse all EntityType elements.
fn parse_entity_types(csdl: &str) -> Vec<EdmxEntityType> {
    let mut types = Vec::new();
    let mut search_from = 0;

    while let Some(start) = csdl[search_from..].find("<EntityType ") {
        let abs_start = search_from + start;
        let type_end = find_closing_tag(&csdl[abs_start..], "EntityType")
            .map(|e| abs_start + e)
            .unwrap_or(csdl.len());
        let block = &csdl[abs_start..type_end];

        let name = extract_attr(block, "EntityType", "Name").unwrap_or_default();
        let base_type = extract_attr(block, "EntityType", "BaseType");
        let is_abstract = block.contains("Abstract=\"true\"");

        // Key properties
        let key_properties = parse_key_properties(block);

        // Scalar properties
        let properties = parse_properties(block);

        // Navigation properties
        let navigation_properties = parse_navigation_properties(block);

        if !name.is_empty() {
            types.push(EdmxEntityType {
                name,
                base_type,
                is_abstract,
                key_properties,
                properties,
                navigation_properties,
            });
        }

        search_from = type_end;
    }

    types
}

/// Parse Key/PropertyRef elements.
fn parse_key_properties(block: &str) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(key_section) = extract_section(block, "Key") {
        let mut search = 0;
        while let Some(start) = key_section[search..].find("<PropertyRef ") {
            let abs = search + start;
            if let Some(name) = extract_attr(&key_section[abs..], "PropertyRef", "Name") {
                keys.push(name);
            }
            search = abs + 1;
        }
    }
    keys
}

/// Parse Property elements (scalar properties).
fn parse_properties(block: &str) -> Vec<EdmxProperty> {
    let mut props = Vec::new();
    let mut search = 0;

    while let Some(start) = block[search..].find("<Property ") {
        let abs = search + start;
        // `+2` is correct for self-closing `/>` (skips both bytes), but the
        // fallback `>` is only one byte — adding 2 would overshoot by one and
        // panic the next iteration's `block[search..]` when `>` happens to be
        // the last byte of `block`. Split the two cases so each adds the
        // right number of bytes, and clamp to `block.len()` defensively.
        let line_end = if let Some(e) = block[abs..].find("/>") {
            (abs + e + 2).min(block.len())
        } else if let Some(e) = block[abs..].find('>') {
            (abs + e + 1).min(block.len())
        } else {
            block.len()
        };
        let prop_str = &block[abs..line_end];

        let name = extract_attr(prop_str, "Property", "Name").unwrap_or_default();
        let edm_type = extract_attr(prop_str, "Property", "Type").unwrap_or_default();
        let nullable = extract_attr(prop_str, "Property", "Nullable")
            .map(|v| v != "false")
            .unwrap_or(true);
        let max_length =
            extract_attr(prop_str, "Property", "MaxLength").and_then(|v| v.parse().ok());
        let precision =
            extract_attr(prop_str, "Property", "Precision").and_then(|v| v.parse().ok());
        let scale = extract_attr(prop_str, "Property", "Scale").and_then(|v| v.parse().ok());
        let store_generated = extract_attr(prop_str, "Property", "StoreGeneratedPattern")
            .or_else(|| extract_attr(prop_str, "Property", "annotation:StoreGeneratedPattern"));
        let is_concurrency_token = prop_str.contains("ConcurrencyMode=\"Fixed\"");

        if !name.is_empty() {
            props.push(EdmxProperty {
                name,
                edm_type,
                nullable,
                max_length,
                precision,
                scale,
                store_generated,
                is_concurrency_token,
            });
        }

        search = line_end;
    }

    props
}

/// Parse NavigationProperty elements.
fn parse_navigation_properties(block: &str) -> Vec<EdmxNavigationProperty> {
    let mut navs = Vec::new();
    let mut search = 0;

    while let Some(start) = block[search..].find("<NavigationProperty ") {
        let abs = search + start;
        // Same off-by-one fix as parse_properties: `/>` is 2 bytes, `>` is 1.
        let line_end = if let Some(e) = block[abs..].find("/>") {
            (abs + e + 2).min(block.len())
        } else if let Some(e) = block[abs..].find('>') {
            (abs + e + 1).min(block.len())
        } else {
            block.len()
        };
        let nav_str = &block[abs..line_end];

        let name = extract_attr(nav_str, "NavigationProperty", "Name").unwrap_or_default();
        let relationship =
            extract_attr(nav_str, "NavigationProperty", "Relationship").unwrap_or_default();
        let from_role = extract_attr(nav_str, "NavigationProperty", "FromRole").unwrap_or_default();
        let to_role = extract_attr(nav_str, "NavigationProperty", "ToRole").unwrap_or_default();

        if !name.is_empty() {
            navs.push(EdmxNavigationProperty {
                name,
                relationship,
                from_role,
                to_role,
            });
        }

        search = line_end;
    }

    navs
}

/// Parse Association elements.
fn parse_associations(csdl: &str) -> Vec<EdmxAssociation> {
    let mut assocs = Vec::new();
    let mut search = 0;

    while let Some(start) = csdl[search..].find("<Association ") {
        let abs = search + start;
        // Skip AssociationSet — check if the text right after "<Association" is "Set"
        let tag_end = abs + "<Association".len();
        if csdl.get(tag_end..tag_end + 3) == Some("Set") {
            search = abs + 1;
            continue;
        }
        let assoc_end = find_closing_tag(&csdl[abs..], "Association")
            .map(|e| abs + e)
            .unwrap_or(csdl.len());
        let block = &csdl[abs..assoc_end];

        let name = extract_attr(block, "Association", "Name").unwrap_or_default();

        // Parse ends
        let ends = parse_association_ends(block);
        if ends.len() >= 2 && !name.is_empty() {
            let referential_constraint = parse_referential_constraint(block);
            assocs.push(EdmxAssociation {
                name,
                end1: ends[0].clone(),
                end2: ends[1].clone(),
                referential_constraint,
            });
        }

        search = assoc_end;
    }

    assocs
}

/// Parse End elements inside an Association.
fn parse_association_ends(block: &str) -> Vec<EdmxAssociationEnd> {
    let mut ends = Vec::new();
    let mut search = 0;

    while let Some(start) = block[search..].find("<End ") {
        let abs = search + start;
        // Same off-by-one fix as parse_properties: `/>` is 2 bytes, `>` is 1.
        let line_end = if let Some(e) = block[abs..].find("/>") {
            (abs + e + 2).min(block.len())
        } else if let Some(e) = block[abs..].find('>') {
            (abs + e + 1).min(block.len())
        } else {
            block.len()
        };
        let end_str = &block[abs..line_end];

        let role = extract_attr(end_str, "End", "Role").unwrap_or_default();
        let entity_type = extract_attr(end_str, "End", "Type").unwrap_or_default();
        let multiplicity = extract_attr(end_str, "End", "Multiplicity").unwrap_or_default();

        if !role.is_empty() {
            ends.push(EdmxAssociationEnd {
                role,
                entity_type,
                multiplicity,
            });
        }

        search = line_end;
    }

    ends
}

/// Parse ReferentialConstraint inside an Association.
fn parse_referential_constraint(block: &str) -> Option<EdmxReferentialConstraint> {
    let rc_block = extract_section(block, "ReferentialConstraint")?;

    let principal_role = extract_attr(rc_block, "Principal", "Role")?;
    let principal_property = {
        let principal = extract_section(rc_block, "Principal")?;
        extract_attr(principal, "PropertyRef", "Name")?
    };
    let dependent_role = extract_attr(rc_block, "Dependent", "Role")?;
    let dependent_property = {
        let dependent = extract_section(rc_block, "Dependent")?;
        extract_attr(dependent, "PropertyRef", "Name")?
    };

    Some(EdmxReferentialConstraint {
        principal_role,
        principal_property,
        dependent_role,
        dependent_property,
    })
}

/// Parse EntitySet elements from EntityContainer.
fn parse_entity_sets(csdl: &str) -> Vec<EdmxEntitySet> {
    let mut sets = Vec::new();
    let mut search = 0;

    while let Some(start) = csdl[search..].find("<EntitySet ") {
        let abs = search + start;
        // Same off-by-one fix as parse_properties: `/>` is 2 bytes, `>` is 1.
        let line_end = if let Some(e) = csdl[abs..].find("/>") {
            (abs + e + 2).min(csdl.len())
        } else if let Some(e) = csdl[abs..].find('>') {
            (abs + e + 1).min(csdl.len())
        } else {
            csdl.len()
        };
        let set_str = &csdl[abs..line_end];

        let name = extract_attr(set_str, "EntitySet", "Name").unwrap_or_default();
        let entity_type = extract_attr(set_str, "EntitySet", "EntityType").unwrap_or_default();

        if !name.is_empty() {
            sets.push(EdmxEntitySet { name, entity_type });
        }

        search = line_end;
    }

    sets
}

/// Parse ComplexType elements.
fn parse_complex_types(csdl: &str) -> Vec<EdmxComplexType> {
    let mut types = Vec::new();
    let mut search = 0;

    while let Some(start) = csdl[search..].find("<ComplexType ") {
        let abs = search + start;
        let type_end = find_closing_tag(&csdl[abs..], "ComplexType")
            .map(|e| abs + e)
            .unwrap_or(csdl.len());
        let block = &csdl[abs..type_end];

        let name = extract_attr(block, "ComplexType", "Name").unwrap_or_default();
        let properties = parse_properties(block);

        if !name.is_empty() {
            types.push(EdmxComplexType { name, properties });
        }

        search = type_end;
    }

    types
}

/// Find the closing tag position relative to the start of `content`.
fn find_closing_tag(content: &str, tag: &str) -> Option<usize> {
    let pattern = format!("</{}>", tag);
    content.find(&pattern).map(|p| p + pattern.len())
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_EDMX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<edmx:Edmx Version="3.0" xmlns:edmx="http://schemas.microsoft.com/ado/2009/11/edmx">
  <edmx:Runtime>
    <edmx:ConceptualModels>
      <Schema Namespace="MyApp.Models" Alias="Self" xmlns="http://schemas.microsoft.com/ado/2009/11/edm">
        <EntityContainer Name="MyAppContext">
          <EntitySet Name="Products" EntityType="Self.Product" />
          <EntitySet Name="Categories" EntityType="Self.Category" />
          <EntitySet Name="Orders" EntityType="Self.Order" />
        </EntityContainer>

        <EntityType Name="Product">
          <Key>
            <PropertyRef Name="ProductId" />
          </Key>
          <Property Name="ProductId" Type="Int32" Nullable="false" annotation:StoreGeneratedPattern="Identity" />
          <Property Name="Name" Type="String" Nullable="false" MaxLength="200" />
          <Property Name="Price" Type="Decimal" Nullable="false" Precision="18" Scale="2" />
          <Property Name="CategoryId" Type="Int32" Nullable="false" />
          <Property Name="CreatedDate" Type="DateTime" Nullable="false" />
          <NavigationProperty Name="Category" Relationship="Self.FK_Product_Category" FromRole="Product" ToRole="Category" />
          <NavigationProperty Name="OrderItems" Relationship="Self.FK_OrderItem_Product" FromRole="Product" ToRole="OrderItem" />
        </EntityType>

        <EntityType Name="Category">
          <Key>
            <PropertyRef Name="CategoryId" />
          </Key>
          <Property Name="CategoryId" Type="Int32" Nullable="false" annotation:StoreGeneratedPattern="Identity" />
          <Property Name="Name" Type="String" Nullable="false" MaxLength="100" />
          <NavigationProperty Name="Products" Relationship="Self.FK_Product_Category" FromRole="Category" ToRole="Product" />
        </EntityType>

        <Association Name="FK_Product_Category">
          <End Role="Category" Type="Self.Category" Multiplicity="1" />
          <End Role="Product" Type="Self.Product" Multiplicity="*" />
          <ReferentialConstraint>
            <Principal Role="Category">
              <PropertyRef Name="CategoryId" />
            </Principal>
            <Dependent Role="Product">
              <PropertyRef Name="CategoryId" />
            </Dependent>
          </ReferentialConstraint>
        </Association>
      </Schema>
    </edmx:ConceptualModels>
  </edmx:Runtime>
</edmx:Edmx>"#;

    #[test]
    fn test_parse_edmx_namespace() {
        let model = parse_edmx(SAMPLE_EDMX);
        assert_eq!(model.namespace.as_deref(), Some("MyApp.Models"));
    }

    #[test]
    fn test_parse_edmx_container() {
        let model = parse_edmx(SAMPLE_EDMX);
        assert_eq!(model.container_name.as_deref(), Some("MyAppContext"));
    }

    #[test]
    fn test_parse_entity_types() {
        let model = parse_edmx(SAMPLE_EDMX);
        assert!(model.entity_types.len() >= 2);

        let product = model.entity_types.iter().find(|e| e.name == "Product");
        assert!(product.is_some());
        let product = product.unwrap();

        assert_eq!(product.key_properties, vec!["ProductId"]);
        assert!(product.properties.len() >= 4);

        // Check Price property
        let price = product.properties.iter().find(|p| p.name == "Price");
        assert!(price.is_some());
        let price = price.unwrap();
        assert_eq!(price.edm_type, "Decimal");
        assert_eq!(price.precision, Some(18));
        assert_eq!(price.scale, Some(2));

        // Check navigation properties
        assert!(!product.navigation_properties.is_empty());
        let cat_nav = product
            .navigation_properties
            .iter()
            .find(|n| n.name == "Category");
        assert!(cat_nav.is_some());
    }

    #[test]
    fn test_parse_associations() {
        let model = parse_edmx(SAMPLE_EDMX);
        assert!(!model.associations.is_empty());

        let fk = model
            .associations
            .iter()
            .find(|a| a.name == "FK_Product_Category");
        assert!(fk.is_some());
        let fk = fk.unwrap();

        assert_eq!(fk.end1.multiplicity, "1");
        assert_eq!(fk.end2.multiplicity, "*");
        assert_eq!(cardinality_str(fk), "1:*");

        // Referential constraint
        assert!(fk.referential_constraint.is_some());
        let rc = fk.referential_constraint.as_ref().unwrap();
        assert_eq!(rc.principal_property, "CategoryId");
        assert_eq!(rc.dependent_property, "CategoryId");
    }

    #[test]
    fn test_parse_entity_sets() {
        let model = parse_edmx(SAMPLE_EDMX);
        assert!(model.entity_sets.len() >= 2);
        assert!(model.entity_sets.iter().any(|es| es.name == "Products"));
    }

    #[test]
    fn test_clean_entity_type_name() {
        assert_eq!(clean_entity_type_name("Self.Product"), "Product");
        assert_eq!(clean_entity_type_name("MyApp.Models.Product"), "Product");
        assert_eq!(clean_entity_type_name("Product"), "Product");
    }

    #[test]
    fn test_cardinality_str() {
        let assoc = EdmxAssociation {
            name: "test".into(),
            end1: EdmxAssociationEnd {
                role: "A".into(),
                entity_type: "A".into(),
                multiplicity: "0..1".into(),
            },
            end2: EdmxAssociationEnd {
                role: "B".into(),
                entity_type: "B".into(),
                multiplicity: "*".into(),
            },
            referential_constraint: None,
        };
        assert_eq!(cardinality_str(&assoc), "0..1:*");
    }

    #[test]
    fn test_parse_entity_with_basetype() {
        let edmx = r#"<?xml version="1.0" encoding="utf-8"?>
<edmx:Edmx Version="3.0" xmlns:edmx="http://schemas.microsoft.com/ado/2009/11/edmx">
  <edmx:Runtime>
    <edmx:ConceptualModels>
      <Schema Namespace="MyApp.Models" Alias="Self" xmlns="http://schemas.microsoft.com/ado/2009/11/edm">
        <EntityContainer Name="MyAppContext">
          <EntitySet Name="People" EntityType="Self.PersonEntity" />
          <EntitySet Name="Employees" EntityType="Self.EmployeeEntity" />
        </EntityContainer>

        <EntityType Name="PersonEntity">
          <Key>
            <PropertyRef Name="PersonId" />
          </Key>
          <Property Name="PersonId" Type="Int32" Nullable="false" />
          <Property Name="Name" Type="String" MaxLength="200" />
        </EntityType>

        <EntityType Name="EmployeeEntity" BaseType="Self.PersonEntity">
          <Property Name="EmployeeId" Type="Int32" Nullable="false" />
          <Property Name="Department" Type="String" MaxLength="100" />
        </EntityType>

        <EntityType Name="ManagerEntity" BaseType="Self.EmployeeEntity" Abstract="true">
          <Property Name="Level" Type="Int32" Nullable="false" />
        </EntityType>
      </Schema>
    </edmx:ConceptualModels>
  </edmx:Runtime>
</edmx:Edmx>"#;

        let model = parse_edmx(edmx);

        // PersonEntity has no base type
        let person = model
            .entity_types
            .iter()
            .find(|e| e.name == "PersonEntity")
            .unwrap();
        assert!(person.base_type.is_none());
        assert!(!person.is_abstract);

        // EmployeeEntity inherits from Self.PersonEntity
        let employee = model
            .entity_types
            .iter()
            .find(|e| e.name == "EmployeeEntity")
            .unwrap();
        assert_eq!(employee.base_type.as_deref(), Some("Self.PersonEntity"));
        assert!(!employee.is_abstract);
        // Should have its own properties (not inherited ones)
        assert!(employee.properties.iter().any(|p| p.name == "EmployeeId"));

        // ManagerEntity inherits from Self.EmployeeEntity and is abstract
        let manager = model
            .entity_types
            .iter()
            .find(|e| e.name == "ManagerEntity")
            .unwrap();
        assert_eq!(manager.base_type.as_deref(), Some("Self.EmployeeEntity"));
        assert!(manager.is_abstract);

        // clean_entity_type_name should strip the namespace prefix
        assert_eq!(clean_entity_type_name("Self.PersonEntity"), "PersonEntity");
    }

    #[test]
    fn extract_section_handles_orphan_close_tag() {
        // Malformed input where </EntityType> appears BEFORE the matching
        // <EntityType — used to panic with `begin <= end (32 <= 24)` because
        // the close tag was searched globally and could land before the open
        // tag. Should now find the close *after* the open and return a valid
        // slice (or None if there is no balanced close).
        let content =
            "<Container></EntityType><Schema><EntityType Name=\"X\"></EntityType></Schema>";
        let section = extract_section(content, "EntityType");
        assert!(
            section.is_some(),
            "Expected a slice once the close-after-open search is in place"
        );
        let s = section.unwrap();
        assert!(s.starts_with("<EntityType Name=\"X\""));
        assert!(s.ends_with("</EntityType>"));
    }

    #[test]
    fn extract_section_returns_none_when_no_balanced_close() {
        // Open tag exists but no matching close anywhere — must not panic.
        let content = "<EntityType Name=\"X\"><Property /";
        let section = extract_section(content, "EntityType");
        assert!(section.is_none());
    }

    #[test]
    fn parse_edmx_does_not_panic_on_malformed_input() {
        // Malformed EDMX with stray close tags — should parse to *something*
        // (possibly empty) without panicking.
        let bad = "</EntityType><edmx:Edmx><EntityType Name=\"X\"></EntityType>";
        let _ = parse_edmx(bad);
        let bad2 = "</Schema></EntityType></Property>";
        let _ = parse_edmx(bad2);
    }

    #[test]
    fn parse_properties_handles_non_self_closing_at_eof() {
        // Regression test: a non-self-closing `<Property ...>` tag where `>`
        // is the LAST byte of the block triggered an off-by-one panic. The
        // helpers used `+2` for both `/>` and `>` cases, so the fallback
        // overshot by one and the next iteration's `block[search..]` panicked
        // with "byte index N out of bounds".
        let bad = "<EntityType Name=\"E\"><Property Name=\"X\">";
        // Must not panic.
        let _ = parse_edmx(bad);

        // Same shape with NavigationProperty / End / EntitySet.
        let _ = parse_edmx("<EntityType Name=\"E\"><NavigationProperty Name=\"N\">");
        let _ = parse_edmx("<Association Name=\"A\"><End Role=\"R\">");
        let _ = parse_edmx("<EntityContainer><EntitySet Name=\"S\">");
    }
}
