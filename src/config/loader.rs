use std::path::Path;
use miette::{IntoDiagnostic, Result, miette};
use super::model::ArchitectureConfig;

pub fn load_config(config_path: &Path) -> Result<ArchitectureConfig> {
    let ext = config_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    if ext == "toml" {
        let text = std::fs::read_to_string(config_path).into_diagnostic()?;
        return toml::from_str(&text).into_diagnostic();
    }

    if ext == "json" {
        let text = std::fs::read_to_string(config_path).into_diagnostic()?;
        return serde_json::from_str(&text).into_diagnostic();
    }

    // No recognised extension — probe for .toml then .json
    let toml_path = config_path.with_extension("toml");
    if toml_path.exists() {
        let text = std::fs::read_to_string(&toml_path).into_diagnostic()?;
        return toml::from_str(&text).into_diagnostic();
    }

    let json_path = config_path.with_extension("json");
    if json_path.exists() {
        let text = std::fs::read_to_string(&json_path).into_diagnostic()?;
        return serde_json::from_str(&text).into_diagnostic();
    }

    Err(miette!(
        "No configuration found at {:?}. Run `ark init` to generate a starter config.",
        config_path
    ))
}
