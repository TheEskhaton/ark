use std::path::Path;
use miette::Result;

use crate::baseline;
use crate::config::load_config;
use super::check::collect;

pub fn run(root: &str, config_path: &str) -> Result<()> {
    let root_path = Path::new(root);
    let config = load_config(Path::new(config_path))?;
    let report = collect(root_path, &config)?;

    let baseline_path = root_path.join("ark-baseline.json");
    baseline::save(&baseline_path, &report.violation_keys)?;

    println!(
        "Wrote {} suppressed violation(s) to {:?}",
        report.violation_keys.len(),
        baseline_path
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::BaselineEntry;
    use std::fs;

    #[test]
    fn writes_baseline_file_with_violation_keys() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let config_path = dir.path().join("architecture.toml");

        let toml_str = r#"
dependency_rules = []

[[layers]]
name = "Domain"
patterns = ["*.Domain"]
"#;
        fs::write(&config_path, toml_str).unwrap();

        let config_path_str = config_path.to_str().unwrap();

        let result = run(root, config_path_str);

        // Assert
        assert!(result.is_ok(), "run should succeed");
        let baseline_file = dir.path().join("ark-baseline.json");
        assert!(
            baseline_file.exists(),
            "baseline file should be created at {:?}",
            baseline_file
        );

        // Verify it's valid JSON
        let content = fs::read_to_string(&baseline_file).unwrap();
        let entries: Vec<BaselineEntry> = serde_json::from_str(&content).unwrap();
        assert!(
            entries.is_empty(),
            "baseline should be empty when no violations exist"
        );
    }
}
