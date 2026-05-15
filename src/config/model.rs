use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ArchitectureConfig {
    pub layers: Vec<Layer>,
    pub dependency_rules: Vec<DependencyRule>,
    #[serde(default)]
    pub package_policies: Vec<PackagePolicy>,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Layer {
    pub name: String,
    /// Glob patterns matched against project names (e.g. "*.Api", "*.Domain")
    pub patterns: Vec<String>,
    /// Glob patterns matched against C# namespace declarations (e.g. "MyApp.Domain.*")
    /// Required to enable source-level scanning for this layer.
    #[serde(default)]
    pub namespace_patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DependencyRule {
    pub from: String,
    pub to: String,
    /// `true` = this connection is allowed; `false` = forbidden
    pub allowed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PackagePolicy {
    pub layer: String,
    pub forbidden: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_full_config() {
        let toml = r#"
layers = [
  { name = "Presentation", patterns = ["*.Api"] },
  { name = "Domain",       patterns = ["*.Domain", "*.Core"] },
]
dependency_rules = [
  { from = "Presentation", to = "Domain", allowed = true },
]
package_policies = [
  { layer = "Domain", forbidden = ["EntityFramework"] },
]
"#;
        let cfg: ArchitectureConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.layers.len(), 2);
        assert_eq!(cfg.layers[0].name, "Presentation");
        assert_eq!(cfg.dependency_rules.len(), 1);
        assert!(cfg.dependency_rules[0].allowed);
        assert_eq!(cfg.package_policies[0].forbidden, vec!["EntityFramework"]);
    }

    #[test]
    fn package_policies_defaults_to_empty_when_absent() {
        let toml = r#"
layers = [{ name = "A", patterns = ["*"] }]
dependency_rules = []
"#;
        let cfg: ArchitectureConfig = toml::from_str(toml).unwrap();
        assert!(cfg.package_policies.is_empty());
    }

    #[test]
    fn dependency_rule_allowed_field_deserialized_correctly() {
        let toml = r#"
layers = []
dependency_rules = [
  { from = "A", to = "B", allowed = false },
  { from = "C", to = "D", allowed = true  },
]
"#;
        let cfg: ArchitectureConfig = toml::from_str(toml).unwrap();
        assert!(!cfg.dependency_rules[0].allowed);
        assert!(cfg.dependency_rules[1].allowed);
    }
}
