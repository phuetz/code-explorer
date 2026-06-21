//! Telerik / Kendo UI component extraction from Razor and JavaScript source.

use once_cell::sync::Lazy;
use regex::Regex;

use super::types::*;

/// Html.Kendo().Grid<Model>() or Html.Kendo().ComboBox()
static RE_KENDO: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"Html\.\s*Kendo\s*\(\s*\)\s*\.\s*(\w+)(?:<(\w+)>)?"#).unwrap());

/// Html.Telerik().Grid() -- older Telerik MVC Extensions syntax
static RE_TELERIK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"Html\.\s*Telerik\s*\(\s*\)\s*\.\s*(\w+)(?:<(\w+)>)?"#).unwrap());

/// Html.Telerik().DatePickerFor(m => m.Property) -- Telerik *For helpers with lambda bindings
static RE_TELERIK_FOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"Html\.\s*Telerik\s*\(\s*\)\s*\.\s*(\w+For)\s*\(\s*\w+\s*=>\s*\w+\.(\w+)"#)
        .expect("RE_TELERIK_FOR regex must compile")
});

/// DataSource action: .Read(.Action("GetAll", "Products")) or .Create(... etc.
static RE_DS_ACTION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\.\s*(Read|Create|Update|Destroy)\s*\(.*?\.Action\s*\(\s*"(\w+)"\s*,\s*"(\w+)""#)
        .unwrap()
});

// Legacy Telerik Extensions syntax: .Select("Action", "Controller"), .Insert(...), .Update(...), .Delete(...)
static RE_DS_LEGACY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\.\s*(Select|Insert|Update|Delete)\s*\(\s*"(\w+)"\s*,\s*"(\w+)""#).unwrap()
});

/// Client events: .Events(e => e.OnChange("onGridChange")) or .On("change", "handler")
static RE_CLIENT_EVENT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\.On(\w+)\s*\(\s*"(\w+)""#).unwrap());

/// Grid column binding: columns.Bound(e => e.PropertyName)
static RE_COLUMN_BOUND: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"columns\.Bound\(\s*\w+\s*=>\s*\w+\.(\w+)\s*\)"#)
        .expect("RE_COLUMN_BOUND regex must compile")
});

/// Grid column title: .Title("Some Title")
static RE_COLUMN_TITLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\.Title\(\s*"([^"]+)"\s*\)"#).expect("RE_COLUMN_TITLE regex must compile")
});

/// jQuery Kendo widget initialization: .kendoGrid(, .kendoComboBox( etc.
static RE_KENDO_JQUERY: Lazy<Regex> = Lazy::new(|| Regex::new(r#"\.\s*kendo(\w+)\s*\("#).unwrap());

/// Extract Telerik / Kendo UI component declarations from Razor or JavaScript source.
pub fn extract_telerik_components(source: &str) -> Vec<TelerikComponentInfo> {
    let mut results = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (line_idx, &line) in lines.iter().enumerate() {
        let line_number = (line_idx + 1) as u32;

        // --- Html.Kendo().Widget<T>() ---
        if let Some(cap) = RE_KENDO.captures(line) {
            let component_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let model_type = cap.get(2).map(|m| m.as_str().to_string());
            let (ds_actions, events, columns) = scan_component_body(&lines, line_idx, 50);

            results.push(TelerikComponentInfo {
                component_type,
                vendor: "Kendo".to_string(),
                model_type,
                data_source_actions: ds_actions,
                client_events: events,
                columns,
                line_number,
            });
            continue;
        }

        // --- Html.Telerik().DatePickerFor(m => m.Property) etc. ---
        // Try the more specific *For regex first to capture the bound property name
        if let Some(cap) = RE_TELERIK_FOR.captures(line) {
            let component_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let bound_property = cap.get(2).map(|m| m.as_str().to_string());
            let (ds_actions, events, columns) = scan_component_body(&lines, line_idx, 50);

            // Store the bound property as the model_type for *For helpers
            results.push(TelerikComponentInfo {
                component_type,
                vendor: "Telerik".to_string(),
                model_type: bound_property,
                data_source_actions: ds_actions,
                client_events: events,
                columns,
                line_number,
            });
            continue;
        }

        // --- Html.Telerik().Widget() ---
        if let Some(cap) = RE_TELERIK.captures(line) {
            let component_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let model_type = cap.get(2).map(|m| m.as_str().to_string());
            let (ds_actions, events, columns) = scan_component_body(&lines, line_idx, 50);

            results.push(TelerikComponentInfo {
                component_type,
                vendor: "Telerik".to_string(),
                model_type,
                data_source_actions: ds_actions,
                client_events: events,
                columns,
                line_number,
            });
            continue;
        }

        // --- jQuery: $(...).kendoGrid({ ... }) ---
        if let Some(cap) = RE_KENDO_JQUERY.captures(line) {
            let component_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let (ds_actions, events, columns) = scan_component_body(&lines, line_idx, 50);

            results.push(TelerikComponentInfo {
                component_type,
                vendor: "Kendo".to_string(),
                model_type: None,
                data_source_actions: ds_actions,
                client_events: events,
                columns,
                line_number,
            });
        }
    }

    results
}

/// Scan up to `lookahead` lines after a component declaration for DataSource actions, events, and grid columns.
fn scan_component_body(
    lines: &[&str],
    start: usize,
    lookahead: usize,
) -> (
    Vec<DataSourceAction>,
    Vec<(String, String)>,
    Vec<GridColumnInfo>,
) {
    let mut ds_actions = Vec::new();
    let mut events = Vec::new();
    let mut columns = Vec::new();
    let end = (start + lookahead).min(lines.len());

    for i in start..end {
        let line = lines[i];

        // Kendo syntax: .Read(read => read.Action("Action", "Controller"))
        if let Some(cap) = RE_DS_ACTION.captures(line) {
            let operation = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let action_name = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let controller_name = cap
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            ds_actions.push(DataSourceAction {
                operation,
                controller_name,
                action_name,
            });
        }
        // Legacy Telerik syntax: .Select("Action", "Controller")
        else if let Some(cap) = RE_DS_LEGACY.captures(line) {
            let raw_op = cap.get(1).map(|m| m.as_str()).unwrap_or_default();
            let operation = match raw_op {
                "Select" => "Read",
                "Insert" => "Create",
                "Delete" => "Destroy",
                other => other,
            }
            .to_string();
            let action_name = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let controller_name = cap
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            ds_actions.push(DataSourceAction {
                operation,
                controller_name,
                action_name,
            });
        }

        if let Some(cap) = RE_CLIENT_EVENT.captures(line) {
            let event_name = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let handler = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            events.push((event_name, handler));
        }

        // Grid column bindings: columns.Bound(e => e.Property).Title("...").ClientTemplate(...)
        if let Some(cap) = RE_COLUMN_BOUND.captures(line) {
            let property_name = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            // Check same line and next line for .Title("...") and .ClientTemplate(
            let context = if i + 1 < end {
                format!("{} {}", line, lines[i + 1])
            } else {
                line.to_string()
            };

            let title = RE_COLUMN_TITLE
                .captures(&context)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());

            let has_client_template = context.contains(".ClientTemplate(");

            columns.push(GridColumnInfo {
                property_name,
                title,
                has_client_template,
            });
        }
    }

    (ds_actions, events, columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_telerik_kendo_grid() {
        let source = r#"
@(Html.Kendo().Grid<ProductViewModel>()
    .Name("productsGrid")
    .Columns(columns => {
        columns.Bound(p => p.Name);
        columns.Bound(p => p.Price);
    })
    .DataSource(ds => ds
        .Ajax()
        .Read(read => read.Action("GetProducts", "Products"))
        .Create(create => create.Action("CreateProduct", "Products"))
        .Update(update => update.Action("UpdateProduct", "Products"))
        .Destroy(destroy => destroy.Action("DeleteProduct", "Products"))
    )
)
"#;
        let components = extract_telerik_components(source);
        assert_eq!(components.len(), 1);

        let grid = &components[0];
        assert_eq!(grid.component_type, "Grid");
        assert_eq!(grid.vendor, "Kendo");
        assert_eq!(grid.model_type.as_deref(), Some("ProductViewModel"));
        assert_eq!(grid.data_source_actions.len(), 4);

        let read = grid
            .data_source_actions
            .iter()
            .find(|a| a.operation == "Read")
            .unwrap();
        assert_eq!(read.action_name, "GetProducts");
        assert_eq!(read.controller_name, "Products");
    }

    #[test]
    fn test_extract_telerik_legacy() {
        let source = r#"
@(Html.Telerik().Grid<OrderViewModel>()
    .Name("ordersGrid")
    .DataBinding(db => db
        .Ajax()
        .Select("GetOrders", "Orders")
    )
)
"#;
        let components = extract_telerik_components(source);
        assert_eq!(components.len(), 1);

        let grid = &components[0];
        assert_eq!(grid.component_type, "Grid");
        assert_eq!(grid.vendor, "Telerik");
        assert_eq!(grid.model_type.as_deref(), Some("OrderViewModel"));
        // Legacy .Select("Action", "Controller") should be captured as a Read DataSource action
        assert_eq!(grid.data_source_actions.len(), 1);
        assert_eq!(grid.data_source_actions[0].operation, "Read");
        assert_eq!(grid.data_source_actions[0].action_name, "GetOrders");
        assert_eq!(grid.data_source_actions[0].controller_name, "Orders");
    }

    #[test]
    fn test_extract_telerik_real_world_grid() {
        // Real-world Telerik Extensions for ASP.NET MVC pattern from a legacy MVC5 app
        let source = r#"
@(Html.Telerik().Grid<Export>()
    .Name("GridExports")
    .DataBinding(dataBinding => dataBinding.Ajax()
        .Select("GetExportElodieGrid", "Factures"))
    .Columns(columns => {
        columns.Bound(e => e.DateCréation).Title("Date export");
        columns.Bound(e => e.NomExport).Title("Nom du fichier");
    })
    .ClientEvents(events => events
        .OnDataBinding("onGridDataBinding")
        .OnDataBound("onGridDataBound")
        .OnError("onGridError"))
)
"#;
        let components = extract_telerik_components(source);
        assert_eq!(
            components.len(),
            1,
            "Should detect one Telerik Grid component"
        );

        let grid = &components[0];
        assert_eq!(grid.component_type, "Grid");
        assert_eq!(grid.vendor, "Telerik");
        assert_eq!(grid.model_type.as_deref(), Some("Export"));

        // DataSource: .Select("GetExportElodieGrid", "Factures") -> Read action
        assert_eq!(grid.data_source_actions.len(), 1);
        assert_eq!(grid.data_source_actions[0].operation, "Read");
        assert_eq!(
            grid.data_source_actions[0].action_name,
            "GetExportElodieGrid"
        );
        assert_eq!(grid.data_source_actions[0].controller_name, "Factures");

        // Client events
        assert!(
            grid.client_events.len() >= 2,
            "Should detect at least OnDataBinding and OnDataBound"
        );
    }

    #[test]
    fn test_extract_telerik_kendo_jquery() {
        let source = r##"
<script>
    $("#grid").kendoGrid({
        dataSource: { transport: { read: "/api/data" } }
    });
</script>
"##;
        let components = extract_telerik_components(source);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].component_type, "Grid");
        assert_eq!(components[0].vendor, "Kendo");
    }

    #[test]
    fn test_extract_telerik_client_events() {
        let source = r#"
@(Html.Kendo().Grid<ProductViewModel>()
    .Name("grid")
    .Events(e => e
        .OnChange("onGridChange")
        .OnDataBound("onDataBound")
    )
)
"#;
        let components = extract_telerik_components(source);
        assert_eq!(components.len(), 1);

        let grid = &components[0];
        assert_eq!(grid.client_events.len(), 2);
        assert!(grid
            .client_events
            .iter()
            .any(|(e, h)| e == "Change" && h == "onGridChange"));
        assert!(grid
            .client_events
            .iter()
            .any(|(e, h)| e == "DataBound" && h == "onDataBound"));
    }

    #[test]
    fn test_extract_grid_columns() {
        let source = r#"
@(Html.Telerik().Grid<Export>()
    .Name("GridExports")
    .Columns(columns => {
        columns.Bound(e => e.DateCréation).Title("Date export").Format("{0:dd/MM/yyyy}");
        columns.Bound(e => e.NomExport).Title("Nom du fichier");
        columns.Bound(e => e.Etat).Title("État");
        columns.Bound(e => e.IdExport).ClientTemplate("...").Title("Action");
    })
)
"#;
        let components = extract_telerik_components(source);
        assert_eq!(components[0].columns.len(), 4);
        assert_eq!(components[0].columns[0].property_name, "DateCréation");
        assert_eq!(
            components[0].columns[0].title.as_deref(),
            Some("Date export")
        );
        assert!(components[0].columns[3].has_client_template);
    }

    #[test]
    fn test_telerik_for_regex() {
        let line = r#"@(Html.Telerik().DatePickerFor(m => m.DateNaissance)"#;
        let cap = RE_TELERIK_FOR.captures(line);
        assert!(cap.is_some(), "RE_TELERIK_FOR should match DatePickerFor");
        let cap = cap.unwrap();
        assert_eq!(cap.get(1).unwrap().as_str(), "DatePickerFor");
        assert_eq!(cap.get(2).unwrap().as_str(), "DateNaissance");
    }

    #[test]
    fn test_telerik_dropdownlistfor_regex() {
        let line = r#"@(Html.Telerik().DropDownListFor(m => m.TypeDossier)"#;
        let cap = RE_TELERIK_FOR.captures(line);
        assert!(cap.is_some(), "RE_TELERIK_FOR should match DropDownListFor");
        let cap = cap.unwrap();
        assert_eq!(cap.get(1).unwrap().as_str(), "DropDownListFor");
        assert_eq!(cap.get(2).unwrap().as_str(), "TypeDossier");
    }
}
