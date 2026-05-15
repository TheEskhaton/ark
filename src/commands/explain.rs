use std::path::Path;
use miette::Result;
use rayon::prelude::*;

use crate::config::load_config;
use crate::config::model::ArchitectureConfig;
use crate::parser::{discover_projects, ProjectFile};
use crate::rules::{resolve_layer, is_ignored};

pub async fn run(root: &str, config_path: &str, project: &str) -> Result<()> {
    let root_path = Path::new(root);
    let config = load_config(Path::new(config_path)).await?;

    let project_paths = discover_projects(root_path)?;
    let projects: Vec<ProjectFile> = project_paths
        .par_iter()
        .filter_map(|p| ProjectFile::parse(p).ok())
        .collect();

    let known: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
    if !known.contains(&project) {
        eprintln!("Warning: '{}' not found among discovered .csproj files", project);
    }

    print!("{}", describe_project(project, &config, &known));
    Ok(())
}

pub fn describe_project(
    project: &str,
    config: &ArchitectureConfig,
    known_projects: &[&str],
) -> String {
    let mut out = String::new();

    if is_ignored(project, &config.ignore_patterns) {
        out.push_str(&format!("Project: {project}\n"));
        out.push_str("Layer:   (ignored — matches an ignorePattern)\n");
        return out;
    }

    let Some(layer) = resolve_layer(project, &config.layers) else {
        out.push_str(&format!("Project: {project}\n"));
        out.push_str("Layer:   (unmatched — no layer pattern matches this project name)\n");
        return out;
    };

    out.push_str(&format!("Project: {project}\n"));
    out.push_str(&format!("Layer:   {}\n\n", layer.name));
    out.push_str("Dependency rules:\n");

    for other in &config.layers {
        if other.name == layer.name {
            continue;
        }
        let rule = config.dependency_rules.iter()
            .find(|r| r.from == layer.name && r.to == other.name);
        let (status, tag) = match rule {
            Some(r) if r.allowed => ("allowed  ", "[explicit]"),
            Some(_)              => ("forbidden", "[explicit]"),
            None                 => ("forbidden", "[default] "),
        };
        out.push_str(&format!("  → {:<24} {}  {}\n", other.name, status, tag));
    }

    let siblings: Vec<&str> = known_projects.iter()
        .copied()
        .filter(|&p| p != project)
        .filter(|p| resolve_layer(p, &config.layers).map(|l| l.name == layer.name).unwrap_or(false))
        .collect();

    if !siblings.is_empty() {
        out.push_str("\nOther projects in this layer:\n");
        for s in siblings {
            out.push_str(&format!("  {s}\n"));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{ArchitectureConfig, DependencyRule, Layer};

    fn cfg(layers: &[(&str, &[&str])], rules: &[(&str, &str, bool)]) -> ArchitectureConfig {
        ArchitectureConfig {
            layers: layers.iter().map(|(name, pats)| Layer {
                name: name.to_string(),
                patterns: pats.iter().map(|s| s.to_string()).collect(),
                namespace_patterns: vec![],
            }).collect(),
            dependency_rules: rules.iter().map(|(from, to, allowed)| DependencyRule {
                from: from.to_string(),
                to: to.to_string(),
                allowed: *allowed,
            }).collect(),
            package_policies: vec![],
            ignore_patterns: vec![],
        }
    }

    #[test]
    fn unmatched_project_says_unmatched() {
        let config = cfg(&[("Domain", &["*.Domain"])], &[]);
        let out = describe_project("MyApp.Unknown", &config, &[]);
        assert!(out.contains("unmatched"));
    }

    #[test]
    fn matched_project_shows_layer() {
        let config = cfg(
            &[("Domain", &["*.Domain"]), ("Application", &["*.Application"])],
            &[("Application", "Domain", true)],
        );
        let out = describe_project("MyApp.Domain", &config, &[]);
        assert!(out.contains("Domain"));
        assert!(out.contains("Application"));
        assert!(out.contains("allowed") || out.contains("forbidden"));
    }

    #[test]
    fn ignored_project_says_ignored() {
        let mut config = cfg(&[("Domain", &["*.Domain"])], &[]);
        config.ignore_patterns = vec!["*.Tests".to_string()];
        let out = describe_project("MyApp.Tests", &config, &[]);
        assert!(out.contains("ignored"));
    }

    #[test]
    fn siblings_listed_when_present() {
        let config = cfg(&[("Domain", &["*.Domain"])], &[]);
        let out = describe_project("MyApp.Domain", &config, &["MyApp.Domain", "MyApp.Core.Domain"]);
        assert!(out.contains("MyApp.Core.Domain"));
    }
}
