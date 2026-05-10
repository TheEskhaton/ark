use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

/// A single architectural violation found during the check.
#[derive(Debug, Error, Diagnostic)]
#[error("Architectural violation: {message}")]
pub struct Violation {
    pub message: String,

    #[source_code]
    pub src: NamedSource<String>,

    #[label("violation here")]
    pub span: SourceSpan,
}

/// Aggregate result of an `ark check` run.
pub struct CheckReport {
    pub violations: Vec<Violation>,
    pub warnings: Vec<String>,
}

impl CheckReport {
    pub fn new() -> Self {
        CheckReport {
            violations: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[allow(dead_code)]
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    pub fn print_summary(&self) {
        if self.violations.is_empty() {
            println!("\x1b[32m✓ No architectural violations found.\x1b[0m");
        } else {
            eprintln!(
                "\x1b[31m✗ {} violation(s) found.\x1b[0m",
                self.violations.len()
            );
        }
        for w in &self.warnings {
            eprintln!("\x1b[33m⚠ {w}\x1b[0m");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_report_is_clean() {
        let report = CheckReport::new();
        assert!(report.is_clean());
        assert!(report.violations.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn report_with_violation_is_not_clean() {
        let mut report = CheckReport::new();
        report.violations.push(Violation {
            message: "test violation".to_string(),
            src: miette::NamedSource::new("test.csproj", "content".to_string()),
            span: (0, 1).into(),
        });
        assert!(!report.is_clean());
        assert_eq!(report.violations.len(), 1);
    }

    #[test]
    fn warnings_do_not_affect_is_clean() {
        let mut report = CheckReport::new();
        report.warnings.push("unmatched project".to_string());
        assert!(report.is_clean());
    }
}
