//! Entity Framework DbContext and entity extraction.

use std::collections::HashMap;

use super::helpers::*;
use super::types::*;

/// Detect DbContext classes in C# source code.
pub fn extract_db_contexts(source: &str) -> Vec<DbContextInfo> {
    let mut contexts = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        if let Some(class_match) = find_class_declaration(&lines, i) {
            if class_match
                .base_classes
                .iter()
                .any(|b| b == "DbContext" || b == "IdentityDbContext" || b == "ObjectContext")
            {
                let mut ctx = DbContextInfo {
                    class_name: class_match.name.clone(),
                    connection_string_name: None,
                    entity_sets: Vec::new(),
                };

                // Extract DbSet<T> properties
                if let Some(body_end) = class_match.body_end_line {
                    for j in class_match.body_start_line..=body_end {
                        if j < lines.len() {
                            if let Some(es) = extract_dbset(lines[j]) {
                                ctx.entity_sets.push(es);
                            }
                            // Look for connection string in constructor
                            if let Some(cs) = extract_connection_string(lines[j]) {
                                ctx.connection_string_name = Some(cs);
                            }
                        }
                    }
                }

                contexts.push(ctx);
            }

            i = class_match
                .body_end_line
                .unwrap_or(class_match.body_start_line)
                + 1;
        } else {
            i += 1;
        }
    }

    contexts
}

/// Detect entity classes (classes with [Table] attribute or DbSet<T> references).
pub fn extract_entities(source: &str, known_entity_types: &[String]) -> Vec<EntityInfo> {
    let mut entities = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        if let Some(class_match) = find_class_declaration(&lines, i) {
            let has_table_attr = class_match
                .attributes
                .iter()
                .any(|a| a.starts_with("Table"));
            let is_known_entity = known_entity_types.contains(&class_match.name);

            if has_table_attr || is_known_entity {
                let mut entity = EntityInfo {
                    class_name: class_match.name.clone(),
                    table_name: extract_attribute_value(&class_match.attributes, "Table"),
                    property_annotations: HashMap::new(),
                    navigation_properties: Vec::new(),
                };

                // Extract properties with annotations
                if let Some(body_end) = class_match.body_end_line {
                    extract_entity_properties(
                        &lines,
                        class_match.body_start_line,
                        body_end,
                        &mut entity,
                    );
                }

                entities.push(entity);
            }

            i = class_match
                .body_end_line
                .unwrap_or(class_match.body_start_line)
                + 1;
        } else {
            i += 1;
        }
    }

    entities
}

/// Extract entity properties and their data annotations.
fn extract_entity_properties(
    lines: &[&str],
    body_start: usize,
    body_end: usize,
    entity: &mut EntityInfo,
) {
    let mut pending_annotations: Vec<String> = Vec::new();
    let collection_types = [
        "ICollection<",
        "IList<",
        "List<",
        "IEnumerable<",
        "HashSet<",
        "Collection<",
    ];

    for line in &lines[body_start..=body_end.min(lines.len() - 1)] {
        let trimmed = line.trim();

        // Collect attributes
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let attr_content = trimmed.get(1..trimmed.len() - 1).unwrap_or_default();
            // May have multiple attributes: [Required, MaxLength(100)]
            for attr in attr_content.split(',') {
                let a = attr.trim().to_string();
                if DATA_ANNOTATIONS.iter().any(|da| a.starts_with(da)) {
                    pending_annotations.push(a);
                }
            }
            continue;
        }

        // Check for property declaration
        if trimmed.starts_with("public ")
            && (trimmed.contains("{ get;") || trimmed.contains("{ get "))
        {
            // Extract property name
            let parts: Vec<&str> = trimmed
                .split('{')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .collect();
            if parts.len() >= 3 {
                let prop_name = parts.last().unwrap_or(&"").to_string();
                let prop_type = parts[parts.len() - 2];

                // Store annotations
                if !pending_annotations.is_empty() {
                    entity
                        .property_annotations
                        .insert(prop_name.clone(), pending_annotations.clone());
                }

                // Check for navigation property
                let is_collection = collection_types.iter().any(|ct| prop_type.contains(ct));
                if is_collection {
                    // Extract target type from generic
                    if let Some(start) = prop_type.find('<') {
                        if let Some(end) = prop_type.find('>') {
                            let target = prop_type
                                .get(start + 1..end)
                                .unwrap_or_default()
                                .trim()
                                .to_string();
                            entity.navigation_properties.push(NavigationProperty {
                                name: prop_name,
                                target_type: target,
                                is_collection: true,
                            });
                        }
                    }
                } else if prop_type.starts_with("virtual ") || trimmed.contains("virtual ") {
                    // Single navigation: public virtual Order Order { get; set; }
                    let clean_type = prop_type.replace("virtual ", "");
                    if clean_type.chars().next().is_some_and(|c| c.is_uppercase())
                        && !is_primitive_type(&clean_type)
                    {
                        entity.navigation_properties.push(NavigationProperty {
                            name: prop_name,
                            target_type: clean_type,
                            is_collection: false,
                        });
                    }
                }
            }

            pending_annotations.clear();
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            pending_annotations.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_extractors::csharp::helpers::{extract_connection_string, extract_dbset};

    #[test]
    fn test_extract_db_context() {
        let source = r#"
public class ApplicationDbContext : DbContext
{
    public ApplicationDbContext()
        : base("DefaultConnection")
    {
    }

    public DbSet<Product> Products { get; set; }
    public DbSet<Order> Orders { get; set; }
    public DbSet<Customer> Customers { get; set; }
}
"#;
        let contexts = extract_db_contexts(source);
        assert_eq!(contexts.len(), 1);

        let ctx = &contexts[0];
        assert_eq!(ctx.class_name, "ApplicationDbContext");
        assert_eq!(
            ctx.connection_string_name.as_deref(),
            Some("DefaultConnection")
        );
        assert_eq!(ctx.entity_sets.len(), 3);
        assert!(ctx.entity_sets.iter().any(|es| es.entity_type == "Product"));
    }

    #[test]
    fn test_extract_entities() {
        let source = r#"
[Table("Products")]
public class Product
{
    [Key]
    public int Id { get; set; }

    [Required]
    [MaxLength(200)]
    public string Name { get; set; }

    [Range(0, 99999)]
    public decimal Price { get; set; }

    public int CategoryId { get; set; }

    [ForeignKey("CategoryId")]
    public virtual Category Category { get; set; }

    public virtual ICollection<OrderItem> OrderItems { get; set; }
}
"#;
        let entities = extract_entities(source, &["Product".to_string()]);
        assert_eq!(entities.len(), 1);

        let entity = &entities[0];
        assert_eq!(entity.class_name, "Product");
        assert_eq!(entity.table_name.as_deref(), Some("Products"));
        assert!(!entity.navigation_properties.is_empty());

        // Check collection navigation
        let order_items = entity
            .navigation_properties
            .iter()
            .find(|np| np.name == "OrderItems");
        assert!(order_items.is_some());
        assert!(order_items.unwrap().is_collection);
        assert_eq!(order_items.unwrap().target_type, "OrderItem");
    }

    #[test]
    fn test_extract_dbset() {
        assert!(extract_dbset("public DbSet<Product> Products { get; set; }").is_some());
        assert!(extract_dbset("public virtual DbSet<Order> Orders { get; set; }").is_some());
        assert!(extract_dbset("public IDbSet<Customer> Customers { get; set; }").is_some());
        assert!(extract_dbset("public int Count { get; set; }").is_none());
    }

    #[test]
    fn test_extract_connection_string() {
        assert_eq!(
            extract_connection_string("        : base(\"DefaultConnection\")"),
            Some("DefaultConnection".to_string())
        );
        assert_eq!(
            extract_connection_string("        : base(\"name=MyDb\")"),
            Some("MyDb".to_string())
        );
    }
}
