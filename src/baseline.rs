use miette::{IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub kind: String,
    pub from: String,
    pub to: String,
}

pub fn try_load(path: &Path) -> Option<Vec<BaselineEntry>> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text)
        .map_err(|e| tracing::warn!("Malformed baseline {:?}: {e}", path))
        .ok()
}

pub fn save(path: &Path, entries: &[BaselineEntry]) -> Result<()> {
    let text = serde_json::to_string_pretty(entries).into_diagnostic()?;
    std::fs::write(path, text).into_diagnostic()?;
    Ok(())
}

// apply_baseline lives in commands/check.rs to avoid a circular dependency
// (report.rs imports BaselineEntry; baseline.rs must not import Violation)

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, from: &str, to: &str) -> BaselineEntry {
        BaselineEntry {
            kind: kind.to_string(),
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    #[test]
    fn round_trips_through_json() {
        let entries = vec![
            entry("project_ref", "MyApp.Domain", "MyApp.Infrastructure"),
            entry("package", "MyApp.Domain", "Microsoft.EntityFrameworkCore"),
        ];
        let json = serde_json::to_string(&entries).unwrap();
        let loaded: Vec<BaselineEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(entries, loaded);
    }

    #[test]
    fn save_and_try_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ark-baseline.json");
        let entries = vec![entry("project_ref", "A", "B")];
        save(&path, &entries).unwrap();
        let loaded = try_load(&path).unwrap();
        assert_eq!(loaded, entries);
    }

    #[test]
    fn try_load_returns_none_for_missing_file() {
        let result = try_load(Path::new("/nonexistent/ark-baseline.json"));
        assert!(result.is_none());
    }

    #[test]
    fn try_load_returns_none_for_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ark-baseline.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(try_load(&path).is_none());
    }
}
