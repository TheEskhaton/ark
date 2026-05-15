use std::path::Path;
use miette::{IntoDiagnostic, Result};
use super::model::ArchitectureConfig;

pub fn load_config(config_path: &Path) -> Result<ArchitectureConfig> {
    let text = std::fs::read_to_string(config_path).into_diagnostic()?;
    toml::from_str(&text).into_diagnostic()
}
