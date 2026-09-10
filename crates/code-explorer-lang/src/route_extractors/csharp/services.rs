//! Service / Repository class extraction via naming conventions and DI patterns.

use once_cell::sync::Lazy;
use regex::Regex;

use super::helpers::find_brace_bounds;
use super::types::ServiceInfo;

/// Pattern: public class FooService : IFooService
/// Also matches names containing UnitOfWork (e.g., UnitOfWorkAide) since UnitOfWork
/// classes in legacy codebases often have domain-specific suffixes.
static RE_SERVICE_CLASS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"public\s+class\s+(\w*(?:Service|Repository|Manager|Provider|UnitOfWork|Factory|Facade)\w*)\s*:\s*(I\w+)"#,
    )
    .unwrap()
});

/// Constructor parameter matching an interface: ISomeService someService
static RE_CTOR_PARAM: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(I[A-Z]\w+)\s+(\w+)"#).unwrap());

/// Extract service / repository / manager / provider classes from C# source.
pub fn extract_services_and_repositories(source: &str) -> Vec<ServiceInfo> {
    let mut results = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (line_idx, &line) in lines.iter().enumerate() {
        if let Some(cap) = RE_SERVICE_CLASS.captures(line) {
            let class_name = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let interface_name = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            let layer_type = if class_name.ends_with("Repository") {
                "Repository"
            } else if class_name.ends_with("Service") {
                "Service"
            } else if class_name.ends_with("Manager") {
                "Manager"
            } else if class_name.contains("UnitOfWork") {
                "UnitOfWork"
            } else if class_name.ends_with("Factory") {
                "Factory"
            } else if class_name.ends_with("Facade") {
                "Facade"
            } else {
                "Provider"
            }
            .to_string();

            let dependencies = extract_constructor_dependencies(source, &class_name);

            results.push(ServiceInfo {
                class_name: class_name.clone(),
                layer_type,
                implements_interface: Some(interface_name),
                dependencies,
            });

            // Skip past the class body to avoid re-matching inner classes
            if let Some(body_end) = find_brace_bounds(&lines, line_idx).1 {
                // We can't mutate the iterator, but duplicates are prevented by the
                // regex requiring "public class" which won't match again inside the body
                let _ = body_end;
            }
        }
    }

    results
}

/// Extract constructor-injected dependencies for a specific class.
///
/// Finds `public ClassName(IFoo foo, IBar bar)` and returns `[(IFoo, foo), (IBar, bar)]`.
///
/// When a class declares multiple `public ClassName(...)` overloads (e.g.
/// a parameterless constructor for serialization plus the real DI ctor),
/// the first match alone would silently drop all the DI parameters. We now
/// scan every overload and keep the one with the most dependencies — that
/// is essentially always the DI constructor.
pub fn extract_constructor_dependencies(source: &str, class_name: &str) -> Vec<(String, String)> {
    // Build a regex for this specific constructor: public ClassName(...)
    let pattern = format!(r"public\s+{}\s*\(([^)]*)\)", regex::escape(class_name));
    let re = Regex::new(&pattern).unwrap();

    let mut best: Vec<(String, String)> = Vec::new();

    for cap in re.captures_iter(source) {
        let params = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let mut deps = Vec::new();
        for param_cap in RE_CTOR_PARAM.captures_iter(params) {
            let iface = param_cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let name = param_cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            deps.push((iface, name));
        }
        if deps.len() > best.len() {
            best = deps;
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_service_class() {
        let source = r#"
public class ProductService : IProductService
{
    private readonly IProductRepository _repo;

    public ProductService(IProductRepository repo)
    {
        _repo = repo;
    }

    public Product GetById(int id) => _repo.GetById(id);
}
"#;
        let services = extract_services_and_repositories(source);
        assert_eq!(services.len(), 1);

        let svc = &services[0];
        assert_eq!(svc.class_name, "ProductService");
        assert_eq!(svc.layer_type, "Service");
        assert_eq!(svc.implements_interface.as_deref(), Some("IProductService"));
        assert_eq!(svc.dependencies.len(), 1);
        assert_eq!(svc.dependencies[0].0, "IProductRepository");
        assert_eq!(svc.dependencies[0].1, "repo");
    }

    #[test]
    fn test_extract_repository_class() {
        let source = r#"
public class OrderRepository : IOrderRepository
{
    private readonly ApplicationDbContext _context;

    public OrderRepository(IUnitOfWork unitOfWork, ILogger logger)
    {
        _context = unitOfWork.Context;
    }
}
"#;
        let services = extract_services_and_repositories(source);
        assert_eq!(services.len(), 1);

        let repo = &services[0];
        assert_eq!(repo.class_name, "OrderRepository");
        assert_eq!(repo.layer_type, "Repository");
        assert_eq!(
            repo.implements_interface.as_deref(),
            Some("IOrderRepository")
        );
        assert_eq!(repo.dependencies.len(), 2);
        assert!(repo.dependencies.iter().any(|(t, _)| t == "IUnitOfWork"));
        assert!(repo.dependencies.iter().any(|(t, _)| t == "ILogger"));
    }

    #[test]
    fn test_extract_constructor_deps() {
        let source = r#"
public class InvoiceManager : IInvoiceManager
{
    public InvoiceManager(IOrderService orderService, IEmailService emailService)
    {
    }
}
"#;
        let deps = extract_constructor_dependencies(source, "InvoiceManager");
        assert_eq!(deps.len(), 2);
        assert_eq!(
            deps[0],
            ("IOrderService".to_string(), "orderService".to_string())
        );
        assert_eq!(
            deps[1],
            ("IEmailService".to_string(), "emailService".to_string())
        );
    }

    #[test]
    fn test_extract_unitofwork() {
        let source = r#"
public class UnitOfWorkAide : IUnitOfWork
{
    private readonly ApplicationDbContext _context;

    public UnitOfWorkAide(IApplicationDbContext context)
    {
        _context = (ApplicationDbContext)context;
    }
}
"#;
        let services = extract_services_and_repositories(source);
        assert_eq!(services.len(), 1);

        let uow = &services[0];
        assert_eq!(uow.class_name, "UnitOfWorkAide");
        assert_eq!(uow.layer_type, "UnitOfWork");
        assert_eq!(uow.implements_interface.as_deref(), Some("IUnitOfWork"));
    }
}
