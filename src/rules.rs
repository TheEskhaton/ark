use crate::config::model::Layer;

pub fn resolve_layer<'a>(project_name: &str, layers: &'a [Layer]) -> Option<&'a Layer> {
    layers.iter().find(|l| {
        l.patterns.iter().any(|pat| {
            glob::Pattern::new(pat)
                .map(|p| p.matches(project_name))
                .unwrap_or(false)
        })
    })
}

pub fn resolve_layer_by_namespace<'a>(ns: &str, layers: &'a [Layer]) -> Option<&'a Layer> {
    layers.iter().find(|l| {
        l.namespace_patterns.iter().any(|pat| {
            glob::Pattern::new(pat)
                .map(|p| p.matches(ns))
                .unwrap_or(false)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(name: &str, patterns: &[&str]) -> Layer {
        Layer {
            name: name.to_string(),
            patterns: patterns.iter().map(|s| s.to_string()).collect(),
            namespace_patterns: vec![],
        }
    }

    #[test]
    fn resolve_layer_glob() {
        let layers = vec![layer("Domain", &["*.Domain"])];
        assert_eq!(resolve_layer("MyApp.Domain", &layers).unwrap().name, "Domain");
        assert!(resolve_layer("MyApp.Api", &layers).is_none());
    }

    #[test]
    fn resolve_layer_by_namespace_wildcard() {
        let layers = vec![Layer {
            name: "Domain".to_string(),
            patterns: vec![],
            namespace_patterns: vec!["MyApp.Domain.*".to_string()],
        }];
        assert!(resolve_layer_by_namespace("MyApp.Domain.Entities", &layers).is_some());
        assert!(resolve_layer_by_namespace("MyApp.Application.Foo", &layers).is_none());
    }
}
