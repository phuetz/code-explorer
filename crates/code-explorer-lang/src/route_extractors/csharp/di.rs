//! Dependency Injection registration extraction (Autofac, Unity, Ninject, MS DI).

use once_cell::sync::Lazy;
use regex::Regex;

use super::types::DiRegistration;

/// Autofac: builder.RegisterType<ProductService>().As<IProductService>()
static RE_AUTOFAC: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"RegisterType<(\w+)>\s*\(\s*\)\s*\.As<(\w+)>"#)
        .expect("RE_AUTOFAC regex must compile")
});

/// Autofac lifetime: .SingleInstance(), .InstancePerRequest(), .InstancePerLifetimeScope(), .InstancePerDependency()
static RE_AUTOFAC_LIFETIME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\.(SingleInstance|InstancePerRequest|InstancePerLifetimeScope|InstancePerDependency)\s*\("#)
        .expect("RE_AUTOFAC_LIFETIME regex must compile")
});

/// Unity: container.RegisterType<IProductService, ProductService>()
static RE_UNITY: Lazy<Regex> = Lazy::new(|| Regex::new(r#"RegisterType<(\w+),\s*(\w+)>"#).unwrap());

/// Ninject: Bind<IProductService>().To<ProductService>()
static RE_NINJECT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"Bind<(\w+)>\s*\(\s*\)\s*\.To<(\w+)>"#).unwrap());

/// MS DI: services.AddScoped<IProductService, ProductService>()
/// Type names may be open generics: IAppLogger<> / EfRepository<>.
static RE_MS_DI: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?:AddScoped|AddTransient|AddSingleton)<(\w+(?:<>)?),\s*(\w+(?:<>)?)>"#)
        .expect("RE_MS_DI")
});

/// MS DI single-type: AddScoped<PizzaService>() or AddSingleton<IUriComposer>(...)
static RE_MS_DI_SINGLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?:AddScoped|AddTransient|AddSingleton)<(\w+(?:<>)?)>\s*\("#).expect("RE_MS_DI_SINGLE")
});

/// MS DI open-generic typeof: AddScoped(typeof(IReadRepository<>), typeof(EfRepository<>))
static RE_MS_DI_TYPEOF: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(AddScoped|AddTransient|AddSingleton)\(\s*typeof\((\w+(?:<>)?(?:\.\w+(?:<>)?)*)\),\s*typeof\((\w+(?:<>)?(?:\.\w+(?:<>)?)*)\)"#,
    )
    .expect("RE_MS_DI_TYPEOF")
});

/// MS DI lifetime from method name
static RE_MS_DI_LIFETIME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(AddScoped|AddTransient|AddSingleton)(?:<|\()"#).unwrap());

/// Extract DI container registrations from C# source (Autofac, Unity, Ninject, MS DI).
pub fn extract_di_registrations(source: &str) -> Vec<DiRegistration> {
    let mut results = Vec::new();

    for line in source.lines() {
        // --- Autofac: RegisterType<Impl>().As<IService>() ---
        if let Some(cap) = RE_AUTOFAC.captures(line) {
            let impl_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let svc_type = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            let lifetime = RE_AUTOFAC_LIFETIME.captures(line).map(|lc| {
                let raw = lc.get(1).map(|m| m.as_str()).unwrap_or_default();
                match raw {
                    "SingleInstance" => "Singleton".to_string(),
                    "InstancePerRequest" => "PerRequest".to_string(),
                    "InstancePerLifetimeScope" => "Scoped".to_string(),
                    "InstancePerDependency" => "Transient".to_string(),
                    other => other.to_string(),
                }
            });

            results.push(DiRegistration {
                implementation_type: impl_type,
                service_type: svc_type,
                framework: "Autofac".to_string(),
                lifetime,
            });
            continue;
        }

        // --- Unity: RegisterType<IService, Impl>() ---
        if let Some(cap) = RE_UNITY.captures(line) {
            let svc_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let impl_type = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            results.push(DiRegistration {
                implementation_type: impl_type,
                service_type: svc_type,
                framework: "Unity".to_string(),
                lifetime: None,
            });
            continue;
        }

        // --- Ninject: Bind<IService>().To<Impl>() ---
        if let Some(cap) = RE_NINJECT.captures(line) {
            let svc_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let impl_type = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            results.push(DiRegistration {
                implementation_type: impl_type,
                service_type: svc_type,
                framework: "Ninject".to_string(),
                lifetime: None,
            });
            continue;
        }

        // --- MS DI lifetime helper ---
        let ms_lifetime = || {
            RE_MS_DI_LIFETIME.captures(line).map(|lc| {
                let raw = lc.get(1).map(|m| m.as_str()).unwrap_or_default();
                match raw {
                    "AddScoped" => "Scoped".to_string(),
                    "AddTransient" => "Transient".to_string(),
                    "AddSingleton" => "Singleton".to_string(),
                    other => other.to_string(),
                }
            })
        };

        // --- MS DI: AddScoped/AddTransient/AddSingleton<IService, Impl>() ---
        if let Some(cap) = RE_MS_DI.captures(line) {
            let svc_type = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let impl_type = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();

            results.push(DiRegistration {
                implementation_type: impl_type,
                service_type: svc_type,
                framework: "Microsoft".to_string(),
                lifetime: ms_lifetime(),
            });
            continue;
        }

        // --- MS DI typeof open-generic: AddScoped(typeof(IFoo<>), typeof(Bar<>)) ---
        if let Some(cap) = RE_MS_DI_TYPEOF.captures(line) {
            let svc_type = cap
                .get(2)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let impl_type = cap
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            // Strip namespace qualifiers; keep simple name (+ optional <>)
            let simple = |s: &str| s.rsplit('.').next().unwrap_or(s).to_string();
            results.push(DiRegistration {
                implementation_type: simple(&impl_type),
                service_type: simple(&svc_type),
                framework: "Microsoft".to_string(),
                lifetime: ms_lifetime(),
            });
            continue;
        }

        // --- MS DI single-type: AddScoped<PizzaService>() / AddSingleton<IFoo>(...) ---
        // Must run after the two-type pattern so "IFoo, Bar" is not eaten as single.
        if let Some(cap) = RE_MS_DI_SINGLE.captures(line) {
            let ty = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            results.push(DiRegistration {
                implementation_type: ty.clone(),
                service_type: ty,
                framework: "Microsoft".to_string(),
                lifetime: ms_lifetime(),
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_di_autofac() {
        let source = r#"
builder.RegisterType<ParametrageService>().As<IParametrageService>().SingleInstance();
builder.RegisterType<DossierRepository>().As<IDossierRepository>().InstancePerRequest();
"#;
        let regs = extract_di_registrations(source);
        assert_eq!(regs.len(), 2);

        let first = &regs[0];
        assert_eq!(first.implementation_type, "ParametrageService");
        assert_eq!(first.service_type, "IParametrageService");
        assert_eq!(first.framework, "Autofac");
        assert_eq!(first.lifetime.as_deref(), Some("Singleton"));

        let second = &regs[1];
        assert_eq!(second.implementation_type, "DossierRepository");
        assert_eq!(second.service_type, "IDossierRepository");
        assert_eq!(second.framework, "Autofac");
        assert_eq!(second.lifetime.as_deref(), Some("PerRequest"));
    }

    #[test]
    fn test_extract_di_ms_two_type() {
        let source = r#"
services.AddScoped<IBasketService, BasketService>();
services.AddTransient<IEmailSender, EmailSender>();
"#;
        let regs = extract_di_registrations(source);
        assert_eq!(regs.len(), 2);
        assert_eq!(regs[0].service_type, "IBasketService");
        assert_eq!(regs[0].implementation_type, "BasketService");
        assert_eq!(regs[0].framework, "Microsoft");
        assert_eq!(regs[0].lifetime.as_deref(), Some("Scoped"));
        assert_eq!(regs[1].lifetime.as_deref(), Some("Transient"));
    }

    /// Cause principale eShopOnWeb : enregistrements open-generic typeof.
    #[test]
    fn test_extract_di_ms_typeof_open_generic() {
        let source = r#"
services.AddScoped(typeof(IReadRepository<>), typeof(EfRepository<>));
services.AddScoped(typeof(IRepository<>), typeof(EfRepository<>));
services.AddScoped(typeof(IAppLogger<>), typeof(LoggerAdapter<>));
"#;
        let regs = extract_di_registrations(source);
        assert_eq!(
            regs.len(),
            3,
            "typeof(IFoo<>), typeof(Bar<>) must be extracted; got {:?}",
            regs
        );
        assert_eq!(regs[0].service_type, "IReadRepository<>");
        assert_eq!(regs[0].implementation_type, "EfRepository<>");
        assert_eq!(regs[0].framework, "Microsoft");
        assert_eq!(regs[0].lifetime.as_deref(), Some("Scoped"));
    }

    /// ContosoPizza / eShop : AddScoped<Concrete>() sans interface.
    #[test]
    fn test_extract_di_ms_single_type() {
        let source = r#"
builder.Services.AddScoped<PizzaService>();
builder.Services.AddScoped<ToastService>();
services.AddSingleton<IUriComposer>(new UriComposer(settings));
"#;
        let regs = extract_di_registrations(source);
        assert!(
            regs.iter().any(|r| r.service_type == "PizzaService"
                && r.implementation_type == "PizzaService"),
            "single-type AddScoped<PizzaService>() missing: {:?}",
            regs
        );
        assert!(
            regs.iter().any(|r| r.service_type == "IUriComposer"
                && r.implementation_type == "IUriComposer"),
            "AddSingleton<IUriComposer>(...) single-type missing: {:?}",
            regs
        );
    }
}
