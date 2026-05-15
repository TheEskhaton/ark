use super::model::ArchitectureConfig;
use miette::{IntoDiagnostic, Result};
use std::path::Path;

pub fn load_config(config_path: &Path) -> Result<ArchitectureConfig> {
    let text = std::fs::read_to_string(config_path).into_diagnostic()?;
    toml::from_str(&text).into_diagnostic()
}
