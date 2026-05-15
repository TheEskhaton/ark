use serde::Deserialize;

/// Top-level architecture configuration, deserialized from Pkl.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureConfig {
    pub layers: Vec<Layer>,
    pub dependency_rules: Vec<DependencyRule>,
    #[serde(default)]
    pub package_policies: Vec<PackagePolicy>,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

/// A logical layer that groups projects via glob patterns.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub name: String,
    /// Glob patterns matched against project names (e.g. "*.Api", "*.Domain")
    pub patterns: Vec<String>,
    /// Glob patterns matched against C# namespace declarations (e.g. "MyApp.Domain.*")
    /// Required to enable source-level scanning for this layer.
    #[serde(default)]
    pub namespace_patterns: Vec<String>,
}

/// Declares which layer can depend on which other layer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyRule {
    pub from: String,
    pub to: String,
    /// `true` = this connection is allowed; `false` = forbidden
    pub allowed: bool,
}

/// Restricts NuGet packages per layer.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagePolicy {
    pub layer: String,
    pub forbidden: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_config() {
        let json = r#"{
            "layers": [
                {"name": "Presentation", "patterns": ["*.Api"]},
                {"name": "Domain", "patterns": ["*.Domain", "*.Core"]}
            ],
            "dependencyRules": [
                {"from": "Presentation", "to": "Domain", "allowed": true}
            ],
            "packagePolicies": [
                {"layer": "Domain", "forbidden": ["EntityFramework"]}
            ]
        }"#;
        let cfg: ArchitectureConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.layers.len(), 2);
        assert_eq!(cfg.layers[0].name, "Presentation");
        assert_eq!(cfg.layers[1].patterns, vec!["*.Domain", "*.Core"]);
        assert_eq!(cfg.dependency_rules.len(), 1);
        assert!(cfg.dependency_rules[0].allowed);
        assert_eq!(cfg.package_policies[0].forbidden, vec!["EntityFramework"]);
    }

    #[test]
    fn package_policies_defaults_to_empty_when_absent() {
        let json = r#"{"layers": [{"name": "A", "patterns": ["*"]}], "dependencyRules": []}"#;
        let cfg: ArchitectureConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.package_policies.is_empty());
    }

    #[test]
    fn dependency_rule_allowed_field_deserialized_correctly() {
        let json = r#"{
            "layers": [],
            "dependencyRules": [
                {"from": "A", "to": "B", "allowed": false},
                {"from": "C", "to": "D", "allowed": true}
            ]
        }"#;
        let cfg: ArchitectureConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.dependency_rules[0].allowed);
        assert!(cfg.dependency_rules[1].allowed);
    }
}
