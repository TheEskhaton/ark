use crate::parser::csproj::ProjectRef;
use crate::parser::ProjectFile;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct ScanResult {
    /// Tiers bottom-to-top: index 0 = leaves (most foundational)
    pub tiers: Vec<Vec<String>>,
    /// Projects with no incoming or outgoing refs in the solution
    pub isolated: Vec<String>,
    /// Names of projects filtered out as test/spec projects
    pub test_projects: Vec<String>,
    /// Groups of projects in mutual cycles (each inner Vec has >1 member)
    pub cycles: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct LayerDef {
    pub name: String,
    pub projects: Vec<String>,
}

#[derive(Debug)]
pub struct InterLayerEdge {
    pub from: String,
    pub to: String,
    pub ref_count: usize,
    /// True when a lower-indexed (more foundational) layer depends on a higher-indexed one
    pub unusual: bool,
}

pub fn is_test_project(name: &str) -> bool {
    [".Tests", ".Specs", ".IntegrationTests", ".UnitTests"]
        .iter()
        .any(|s| name.ends_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_projects_detected() {
        assert!(is_test_project("MyApp.Tests"));
        assert!(is_test_project("MyApp.Domain.Tests"));
        assert!(is_test_project("MyApp.Specs"));
        assert!(is_test_project("MyApp.IntegrationTests"));
        assert!(is_test_project("MyApp.UnitTests"));
    }

    #[test]
    fn non_test_projects_not_detected() {
        assert!(!is_test_project("MyApp.Domain"));
        assert!(!is_test_project("MyApp.Api"));
        assert!(!is_test_project("MyApp.TestHelpers"));
    }
}
