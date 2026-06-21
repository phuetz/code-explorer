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
static RE_MS_DI: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?:AddScoped|AddTransient|AddSingleton)<(\w+),\s*(\w+)>"#).unwrap());

/// MS DI lifetime from method name
static RE_MS_DI_LIFETIME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(AddScoped|AddTransient|AddSingleton)<"#).unwrap());

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

            let lifetime = RE_MS_DI_LIFETIME.captures(line).map(|lc| {
                let raw = lc.get(1).map(|m| m.as_str()).unwrap_or_default();
                match raw {
                    "AddScoped" => "Scoped".to_string(),
                    "AddTransient" => "Transient".to_string(),
                    "AddSingleton" => "Singleton".to_string(),
                    other => other.to_string(),
                }
            });

            results.push(DiRegistration {
                implementation_type: impl_type,
                service_type: svc_type,
                framework: "Microsoft".to_string(),
                lifetime,
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
}
