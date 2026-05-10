use std::path::Path;
use miette::{IntoDiagnostic, Result, miette};
use pklrust::{EvaluatorManager, EvaluatorOptions, ModuleSource};
use super::model::ArchitectureConfig;

/// Load and evaluate a Pkl configuration file, returning the parsed config.
///
/// Falls back to a pre-evaluated JSON sidecar (`architecture.json`) when
/// the Pkl binary is not available (handy in CI without Pkl installed).
pub async fn load_config(config_path: &Path) -> Result<ArchitectureConfig> {
    match try_pkl(config_path) {
        Ok(cfg) => return Ok(cfg),
        Err(e) => {
            tracing::debug!("pkl evaluation failed ({e}), trying JSON sidecar");
        }
    }

    let json_path = config_path.with_extension("json");
    if json_path.exists() {
        let text = std::fs::read_to_string(&json_path).into_diagnostic()?;
        let cfg: ArchitectureConfig = serde_json::from_str(&text).into_diagnostic()?;
        return Ok(cfg);
    }

    Err(miette!(
        "Could not load configuration from {:?}. \
        Ensure the Pkl CLI is on PATH or provide a pre-evaluated architecture.json.",
        config_path
    ))
}

fn try_pkl(config_path: &Path) -> Result<ArchitectureConfig> {
    let abs = config_path
        .canonicalize()
        .into_diagnostic()?;

    let mut manager = EvaluatorManager::new().into_diagnostic()?;
    let evaluator = manager
        .new_evaluator(EvaluatorOptions::preconfigured())
        .into_diagnostic()?;

    let source = ModuleSource::file(abs.to_string_lossy().as_ref());
    let cfg: ArchitectureConfig = manager
        .evaluate_module_typed(&evaluator, source)
        .into_diagnostic()?;

    manager.close_evaluator(&evaluator).into_diagnostic()?;
    Ok(cfg)
}
