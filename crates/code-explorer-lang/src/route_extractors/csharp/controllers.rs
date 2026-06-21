//! ASP.NET controller detection and action extraction.

use super::helpers::*;
use super::types::*;

/// Detect if a C# source file contains ASP.NET controller(s) and extract their info.
pub fn extract_controllers(source: &str) -> Vec<ControllerInfo> {
    let mut controllers = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        // Look for class declarations
        if let Some(class_match) = find_class_declaration(&lines, i) {
            // Check if it inherits from a controller base class
            if is_controller_class(&class_match.base_classes) {
                // Determine custom base controller: if the base class is not
                // one of the standard ones, record it for inheritance tracking
                let base_controller = class_match
                    .base_classes
                    .iter()
                    .find(|b| {
                        // Must look like a controller (ends with "Controller") but not be standard
                        !CONTROLLER_BASE_CLASSES.iter().any(|cb| *b == *cb)
                            && (b.ends_with("Controller") || b.ends_with("ControllerBase"))
                    })
                    .cloned();

                let mut controller = ControllerInfo {
                    class_name: class_match.name.clone(),
                    area_name: extract_attribute_value(&class_match.attributes, "Area"),
                    route_prefix: extract_attribute_value(&class_match.attributes, "Route")
                        .or_else(|| {
                            extract_attribute_value(&class_match.attributes, "RoutePrefix")
                        }),
                    is_api_controller: class_match
                        .base_classes
                        .iter()
                        .any(|b| b == "ApiController")
                        || class_match
                            .attributes
                            .iter()
                            .any(|a| a.starts_with("ApiController")),
                    authorize: extract_attribute_value(&class_match.attributes, "Authorize"),
                    actions: Vec::new(),
                    base_controller,
                };

                // Extract actions from the class body
                if let Some(body_end) = class_match.body_end_line {
                    controller.actions = extract_actions(
                        &lines,
                        class_match.body_start_line,
                        body_end,
                        controller.is_api_controller,
                    );
                }

                controllers.push(controller);
            }

            i = class_match
                .body_end_line
                .unwrap_or(class_match.body_start_line)
                + 1;
        } else {
            i += 1;
        }
    }

    controllers
}

/// Extract action methods from a controller body.
fn extract_actions(
    lines: &[&str],
    body_start: usize,
    body_end: usize,
    is_api: bool,
) -> Vec<ActionInfo> {
    let mut actions = Vec::new();
    let mut i = body_start;

    while i <= body_end && i < lines.len() {
        let trimmed = lines[i].trim();

        // Collect attributes for this method
        let mut method_attrs = Vec::new();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            method_attrs.push(
                trimmed
                    .get(1..trimmed.len() - 1)
                    .unwrap_or_default()
                    .to_string(),
            );
            i += 1;
            // Collect multiple attribute lines
            while i <= body_end && i < lines.len() {
                let next = lines[i].trim();
                if next.starts_with('[') && next.ends_with(']') {
                    method_attrs.push(next.get(1..next.len() - 1).unwrap_or_default().to_string());
                    i += 1;
                } else {
                    break;
                }
            }
        }

        // Check if this line is a method declaration
        if i <= body_end && i < lines.len() {
            let line = lines[i].trim();
            if let Some(action) = parse_action_method(line, &method_attrs, is_api, i as u32 + 1) {
                actions.push(action);
            }
        }

        i += 1;
    }

    actions
}

/// Parse a single method declaration line into an ActionInfo.
fn parse_action_method(
    line: &str,
    attributes: &[String],
    is_api: bool,
    start_line: u32,
) -> Option<ActionInfo> {
    // Method pattern: public [virtual|override|async] ReturnType MethodName(params)
    if !line.contains('(') || !line.starts_with("public ") {
        return None;
    }

    // Skip non-action things like constructors, properties
    if line.contains(" class ") || line.contains(" get;") || line.contains(" set;") {
        return None;
    }

    // Extract return type and method name
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    // Find method name (the part before the first '(')
    let before_paren = line.split('(').next()?;
    let method_name = before_paren.split_whitespace().last()?.to_string();

    // Skip constructor (same name as class typically)
    if method_name.starts_with(|c: char| c.is_lowercase()) && !method_name.contains("_") {
        // lowercase-starting methods are fine, just check it's not weird
    }

    // Extract return type (word before method name)
    let return_type = {
        let words: Vec<&str> = before_paren.split_whitespace().collect();
        if words.len() >= 2 {
            let rt = words[words.len() - 2];
            // Skip modifiers
            if ["async", "virtual", "override", "static", "new", "sealed"].contains(&rt) {
                if words.len() >= 3 {
                    Some(words[words.len() - 3].to_string())
                } else {
                    None
                }
            } else {
                Some(rt.to_string())
            }
        } else {
            None
        }
    };

    // Check if return type looks like an action result
    let is_action_method = return_type.as_deref().is_some_and(|rt| {
        rt.contains("Result")
            || rt.contains("Response")
            || rt == "void"
            || rt.starts_with("Task")
            || rt.starts_with("IHttpActionResult")
            || rt.starts_with("IActionResult")
            || rt.starts_with("Json")
            || rt.starts_with("View")
    }) || attributes.iter().any(|a| {
        a.starts_with("Http")
            || a.starts_with("Route")
            || a.starts_with("Action")
            || a.starts_with("GridAction")
    });

    if !is_action_method {
        return None;
    }

    // Extract HTTP method from attributes
    let http_method = extract_http_method(attributes, is_api);

    // Extract route template
    let route_template = attributes.iter().find_map(|attr| {
        if attr.starts_with("Route(") || attr.starts_with("Http") {
            extract_attribute_value(std::slice::from_ref(attr), "Route").or_else(|| {
                // [HttpGet("path")] -> extract path
                for (http_attr, _) in HTTP_ATTRIBUTES {
                    if attr.starts_with(http_attr) {
                        if let Some(v) =
                            extract_attribute_value(std::slice::from_ref(attr), http_attr)
                        {
                            if !v.is_empty() {
                                return Some(v);
                            }
                        }
                    }
                }
                None
            })
        } else {
            None
        }
    });

    // Extract model type from parameters
    let model_type = extract_model_type_from_params(line);

    // Check for [Authorize]
    let requires_auth = attributes.iter().any(|a| a.starts_with("Authorize"));

    // Collect filter/attribute names (anything ending with Attribute, Filter, or Action,
    // plus standard filter names like Authorize, ValidateAntiForgeryToken, etc.)
    let filters = extract_action_filters(attributes);

    // Extract full parameter signature from parentheses
    let parameters = line
        .find('(')
        .and_then(|start| {
            line[start..].find(')').map(|end| {
                let params = line[start + 1..start + end].trim();
                if params.is_empty() {
                    None
                } else {
                    Some(params.to_string())
                }
            })
        })
        .flatten();

    Some(ActionInfo {
        name: method_name,
        http_method,
        route_template,
        model_type,
        return_type,
        requires_auth,
        start_line: Some(start_line),
        filters,
        parameters,
    })
}

/// Extract filter/attribute names from action method attributes.
///
/// Recognizes standard ASP.NET filters as well as custom attributes that follow
/// the naming conventions `*Attribute`, `*Filter`, or `*Action` (e.g.,
/// `[GridAction]`, `[AuthorizeADAttribute]`, `[VerifActionFilter]`).
fn extract_action_filters(attributes: &[String]) -> Vec<String> {
    let mut filters = Vec::new();
    for attr in attributes {
        let attr_name = attr.split('(').next().unwrap_or(attr).trim();
        // Skip HTTP method attributes and Route attributes (those are not filters)
        if attr_name.starts_with("Http")
            || attr_name == "Route"
            || attr_name == "RoutePrefix"
            || attr_name == "Area"
            || attr_name == "ApiController"
            || attr_name == "NonAction"
        {
            continue;
        }
        // Include standard filters
        if STANDARD_FILTERS.contains(&attr_name) {
            filters.push(attr_name.to_string());
            continue;
        }
        // Include custom attributes ending with Attribute, Filter, or Action
        if attr_name.ends_with("Attribute")
            || attr_name.ends_with("Filter")
            || attr_name.ends_with("Action")
        {
            filters.push(attr_name.to_string());
        }
    }
    filters
}

/// Determine HTTP method from attributes.
fn extract_http_method(attributes: &[String], _is_api: bool) -> String {
    for attr in attributes {
        for (attr_name, method) in HTTP_ATTRIBUTES {
            if attr.starts_with(attr_name) {
                return method.to_string();
            }
        }
    }
    // Default: GET for both MVC and API
    "GET".to_string()
}

/// Extract model type from method parameters (look for complex types).
fn extract_model_type_from_params(line: &str) -> Option<String> {
    let paren_start = line.find('(')?;
    let paren_end = line.rfind(')')?;
    let params = line.get(paren_start + 1..paren_end)?;

    for param in params.split(',') {
        let parts: Vec<&str> = param.split_whitespace().collect();
        if parts.len() >= 2 {
            let type_name = parts[parts.len() - 2];
            // Skip primitive types and common framework types
            if !is_primitive_type(type_name) && !type_name.starts_with('[') {
                // Looks like a model type
                let clean = type_name.trim_start_matches('[').split('<').next()?;
                if clean.chars().next()?.is_uppercase() {
                    return Some(clean.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_controllers_basic() {
        let source = r#"
using System.Web.Mvc;

[Authorize]
[RoutePrefix("products")]
public class ProductsController : Controller
{
    [HttpGet]
    [Route("")]
    public ActionResult Index()
    {
        return View();
    }

    [HttpPost]
    [Route("create")]
    public ActionResult Create(ProductViewModel model)
    {
        return RedirectToAction("Index");
    }

    [HttpGet]
    [Route("{id}")]
    public ActionResult Details(int id)
    {
        return View();
    }
}
"#;
        let controllers = extract_controllers(source);
        assert_eq!(controllers.len(), 1);

        let ctrl = &controllers[0];
        assert_eq!(ctrl.class_name, "ProductsController");
        assert_eq!(ctrl.route_prefix.as_deref(), Some("products"));
        assert!(!ctrl.is_api_controller);
        assert!(ctrl.authorize.is_some());
        assert!(ctrl.actions.len() >= 2);

        // Check first action
        let index = ctrl.actions.iter().find(|a| a.name == "Index");
        assert!(index.is_some());
        let index = index.unwrap();
        assert_eq!(index.http_method, "GET");
    }

    #[test]
    fn test_extract_api_controller() {
        let source = r#"
using System.Web.Http;

[RoutePrefix("api/orders")]
public class OrdersController : ApiController
{
    [HttpGet]
    [Route("")]
    public IHttpActionResult GetAll()
    {
        return Ok(orders);
    }

    [HttpPost]
    public IHttpActionResult Create(OrderDto dto)
    {
        return Created(dto);
    }
}
"#;
        let controllers = extract_controllers(source);
        assert_eq!(controllers.len(), 1);

        let ctrl = &controllers[0];
        assert!(ctrl.is_api_controller);
        assert_eq!(ctrl.route_prefix.as_deref(), Some("api/orders"));
    }
}
