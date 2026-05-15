use std::path::Path;
use miette::Result;
use rayon::prelude::*;

use crate::config::{load_config, ArchitectureConfig, Layer};
use crate::parser::{discover_projects, ProjectFile};
use crate::report::{CheckReport, Violation};
use crate::rules::{resolve_layer, resolve_layer_by_namespace, is_ignored};
use crate::scanner;

pub async fn run(root: &str, config_path: &str, strict: bool) -> Result<()> {
    let root = Path::new(root);
    let config = load_config(Path::new(config_path)).await?;

    let project_paths = discover_projects(root)?;
    tracing::info!("Discovered {} projects", project_paths.len());

    let projects: Vec<ProjectFile> = project_paths
        .par_iter()
        .filter_map(|p| match ProjectFile::parse(p) {
            Ok(pf) => Some(pf),
            Err(e) => {
                tracing::warn!("Skipping {:?}: {e}", p);
                None
            }
        })
        .collect();

    let mut report = CheckReport::new();
    check_dependency_rules(&projects, &config, &mut report);
    check_package_policies(&projects, &config, &mut report);
    check_source_rules(root, &config, &mut report)?;
    report.print_summary();

    if !report.violations.is_empty() || (strict && !report.warnings.is_empty()) {
        for v in report.violations {
            eprintln!("{:?}", miette::Report::new_boxed(Box::new(v)));
        }
        std::process::exit(1);
    }

    Ok(())
}

fn check_dependency_rules(
    projects: &[ProjectFile],
    config: &ArchitectureConfig,
    report: &mut CheckReport,
) {
    for project in projects {
        if is_ignored(&project.name, &config.ignore_patterns) {
            continue;
        }
        let Some(from_layer) = resolve_layer(&project.name, &config.layers) else {
            report.warnings.push(format!(
                "Project '{}' does not match any layer pattern",
                project.name
            ));
            continue;
        };

        for pref in &project.project_refs {
            let dep_name = pref
                .resolved
                .as_ref()
                .and_then(|r| r.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| pref.include.clone());

            let Some(to_layer) = resolve_layer(&dep_name, &config.layers) else {
                continue;
            };

            if from_layer.name == to_layer.name {
                continue;
            }

            let allowed = config
                .dependency_rules
                .iter()
                .find(|r| r.from == from_layer.name && r.to == to_layer.name)
                .map(|r| r.allowed)
                .unwrap_or(false);

            if !allowed {
                let src = std::fs::read_to_string(&project.path).unwrap_or_default();
                let span_start = src.find(&pref.include).unwrap_or(0);
                report.violations.push(Violation {
                    message: format!(
                        "Layer '{}' ({}) must not depend on layer '{}' ({})",
                        from_layer.name, project.name, to_layer.name, dep_name
                    ),
                    src: miette::NamedSource::new(project.path.to_string_lossy(), src),
                    span: (span_start, pref.include.len()).into(),
                });
            }
        }
    }
}

/// Scan every .cs file under `root` and report source-level layer violations.
/// Only runs when at least one layer defines `namespace_patterns`.
fn check_source_rules(
    root: &Path,
    config: &ArchitectureConfig,
    report: &mut CheckReport,
) -> Result<()> {
    if config.layers.iter().all(|l| l.namespace_patterns.is_empty()) {
        tracing::debug!("No namespace_patterns defined — skipping source scan");
        return Ok(());
    }

    let headers = scanner::scan_directory(root)?;
    tracing::info!("Source scan: {} .cs files", headers.len());

    for header in &headers {
        let Some(ns) = &header.namespace else { continue };

        let Some(from_layer) = resolve_layer_by_namespace(ns, &config.layers) else {
            continue;
        };

        for using in &header.usings {
            let Some(to_layer) = resolve_layer_by_namespace(using, &config.layers) else {
                continue;
            };

            if from_layer.name == to_layer.name {
                continue;
            }

            let allowed = config
                .dependency_rules
                .iter()
                .find(|r| r.from == from_layer.name && r.to == to_layer.name)
                .map(|r| r.allowed)
                .unwrap_or(false);

            if !allowed {
                let src = std::fs::read_to_string(&header.path).unwrap_or_default();
                let needle = format!("using {using}");
                let span_start = src.find(&needle).unwrap_or(0);
                report.violations.push(Violation {
                    message: format!(
                        "Source: layer '{}' must not use '{}' from layer '{}'",
                        from_layer.name, using, to_layer.name,
                    ),
                    src: miette::NamedSource::new(header.path.to_string_lossy(), src),
                    span: (span_start, needle.len()).into(),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{ArchitectureConfig, DependencyRule, PackagePolicy};
    use crate::parser::csproj::{PackageRef, ProjectRef};

    fn make_config(
        layers: &[(&str, &[&str])],
        rules: &[(&str, &str, bool)],
        policies: &[(&str, &[&str])],
    ) -> ArchitectureConfig {
        ArchitectureConfig {
            layers: layers
                .iter()
                .map(|(name, pats)| Layer {
                    name: name.to_string(),
                    patterns: pats.iter().map(|s| s.to_string()).collect(),
                    namespace_patterns: vec![],
                })
                .collect(),
            dependency_rules: rules
                .iter()
                .map(|(from, to, allowed)| DependencyRule {
                    from: from.to_string(),
                    to: to.to_string(),
                    allowed: *allowed,
                })
                .collect(),
            package_policies: policies
                .iter()
                .map(|(layer, pkgs)| PackagePolicy {
                    layer: layer.to_string(),
                    forbidden: pkgs.iter().map(|s| s.to_string()).collect(),
                })
                .collect(),
            ignore_patterns: vec![],
        }
    }

    fn make_project(name: &str, refs: &[&str], packages: &[(&str, &str)]) -> ProjectFile {
        ProjectFile {
            path: std::path::PathBuf::from(format!("{name}.csproj")),
            name: name.to_string(),
            project_refs: refs
                .iter()
                .map(|r| ProjectRef { include: r.to_string(), resolved: None })
                .collect(),
            package_refs: packages
                .iter()
                .map(|(n, v)| PackageRef { name: n.to_string(), version: v.to_string() })
                .collect(),
        }
    }

    // ── resolve_layer ──────────────────────────────────────────────────────────

    #[test]
    fn resolve_layer_exact_pattern() {
        let layers = vec![Layer {
            name: "Api".to_string(),
            patterns: vec!["MyApp.Api".to_string()],
            namespace_patterns: vec![],
        }];
        assert_eq!(resolve_layer("MyApp.Api", &layers).unwrap().name, "Api");
    }

    #[test]
    fn resolve_layer_glob_wildcard() {
        let layers = vec![Layer {
            name: "Domain".to_string(),
            patterns: vec!["*.Domain".to_string()],
            namespace_patterns: vec![],
        }];
        assert!(resolve_layer("MyApp.Domain", &layers).is_some());
        assert!(resolve_layer("OtherApp.Domain", &layers).is_some());
        assert!(resolve_layer("MyApp.Api", &layers).is_none());
    }

    #[test]
    fn resolve_layer_returns_none_when_unmatched() {
        let layers = vec![Layer {
            name: "Api".to_string(),
            patterns: vec!["*.Api".to_string()],
            namespace_patterns: vec![],
        }];
        assert!(resolve_layer("MyApp.Infrastructure", &layers).is_none());
    }

    #[test]
    fn resolve_layer_returns_first_matching_layer() {
        let layers = vec![
            Layer { name: "First".to_string(),  patterns: vec!["*.Shared".to_string()], namespace_patterns: vec![] },
            Layer { name: "Second".to_string(), patterns: vec!["*.Shared".to_string()], namespace_patterns: vec![] },
        ];
        assert_eq!(resolve_layer("MyApp.Shared", &layers).unwrap().name, "First");
    }

    // ── check_dependency_rules ─────────────────────────────────────────────────

    #[test]
    fn allowed_dependency_no_violation() {
        let config = make_config(
            &[("Presentation", &["*.Api"]), ("Domain", &["*.Domain"])],
            &[("Presentation", "Domain", true)],
            &[],
        );
        let projects = [make_project("MyApp.Api", &["MyApp.Domain"], &[])];
        let mut report = CheckReport::new();
        check_dependency_rules(&projects, &config, &mut report);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn forbidden_dependency_produces_violation() {
        let config = make_config(
            &[("Domain", &["*.Domain"]), ("Infrastructure", &["*.Infrastructure"])],
            &[("Domain", "Infrastructure", false)],
            &[],
        );
        let projects = [make_project("MyApp.Domain", &["MyApp.Infrastructure"], &[])];
        let mut report = CheckReport::new();
        check_dependency_rules(&projects, &config, &mut report);
        assert_eq!(report.violations.len(), 1);
        assert!(report.violations[0].message.contains("Domain"));
        assert!(report.violations[0].message.contains("Infrastructure"));
    }

    #[test]
    fn no_matching_rule_defaults_to_forbidden() {
        let config = make_config(
            &[("Presentation", &["*.Api"]), ("Domain", &["*.Domain"])],
            &[], // no rules
            &[],
        );
        let projects = [make_project("MyApp.Api", &["MyApp.Domain"], &[])];
        let mut report = CheckReport::new();
        check_dependency_rules(&projects, &config, &mut report);
        assert_eq!(report.violations.len(), 1);
    }

    #[test]
    fn unmatched_project_adds_warning_not_violation() {
        let config = make_config(&[("Domain", &["*.Domain"])], &[], &[]);
        let projects = [make_project("MyApp.Utilities", &[], &[])];
        let mut report = CheckReport::new();
        check_dependency_rules(&projects, &config, &mut report);
        assert!(report.violations.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("MyApp.Utilities"));
    }

    #[test]
    fn multiple_forbidden_refs_all_reported() {
        let config = make_config(
            &[("Domain", &["*.Domain"]), ("Infrastructure", &["*.Infrastructure"])],
            &[("Domain", "Infrastructure", false)],
            &[],
        );
        let projects = [make_project(
            "MyApp.Domain",
            &["MyApp.Infrastructure", "Other.Infrastructure"],
            &[],
        )];
        let mut report = CheckReport::new();
        check_dependency_rules(&projects, &config, &mut report);
        assert_eq!(report.violations.len(), 2);
    }

    #[test]
    fn dep_to_unmatched_layer_skipped() {
        // Ref target matches no layer — should produce no violation, no warning
        let config = make_config(&[("Api", &["*.Api"])], &[], &[]);
        let projects = [make_project("MyApp.Api", &["Some.ExternalLib"], &[])];
        let mut report = CheckReport::new();
        check_dependency_rules(&projects, &config, &mut report);
        assert!(report.violations.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn intra_layer_dependency_never_a_violation() {
        // Domain -> Domain.Shared — both match *.Domain or *.Domain.* → same layer, always allowed
        let config = make_config(
            &[("Domain", &["*.Domain", "*.Domain.*"])],
            &[], // no rules at all — defaults to forbidden, but intra-layer should skip
            &[],
        );
        let projects = [make_project(
            "MyCompany.Domain",
            &["MyCompany.Domain.Shared"],
            &[],
        )];
        let mut report = CheckReport::new();
        check_dependency_rules(&projects, &config, &mut report);
        assert!(report.violations.is_empty(), "intra-layer ref should not be a violation");
    }

    // ── check_package_policies ─────────────────────────────────────────────────

    #[test]
    fn forbidden_package_produces_violation() {
        let config = make_config(
            &[("Domain", &["*.Domain"])],
            &[],
            &[("Domain", &["Microsoft.EntityFrameworkCore"])],
        );
        let projects =
            [make_project("MyApp.Domain", &[], &[("Microsoft.EntityFrameworkCore", "7.0.0")])];
        let mut report = CheckReport::new();
        check_package_policies(&projects, &config, &mut report);
        assert_eq!(report.violations.len(), 1);
        assert!(report.violations[0].message.contains("Microsoft.EntityFrameworkCore"));
        assert!(report.violations[0].message.contains("Domain"));
    }

    #[test]
    fn allowed_package_no_violation() {
        let config = make_config(
            &[("Domain", &["*.Domain"])],
            &[],
            &[("Domain", &["Microsoft.EntityFrameworkCore"])],
        );
        let projects = [make_project("MyApp.Domain", &[], &[("FluentValidation", "11.0.0")])];
        let mut report = CheckReport::new();
        check_package_policies(&projects, &config, &mut report);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn package_policy_match_is_case_insensitive() {
        let config = make_config(
            &[("Domain", &["*.Domain"])],
            &[],
            &[("Domain", &["microsoft.entityframeworkcore"])],
        );
        let projects =
            [make_project("MyApp.Domain", &[], &[("Microsoft.EntityFrameworkCore", "7.0.0")])];
        let mut report = CheckReport::new();
        check_package_policies(&projects, &config, &mut report);
        assert_eq!(report.violations.len(), 1);
    }

    #[test]
    fn no_policy_for_layer_allows_any_package() {
        let config = make_config(&[("Domain", &["*.Domain"])], &[], &[]);
        let projects = [make_project("MyApp.Domain", &[], &[("Anything", "1.0.0")])];
        let mut report = CheckReport::new();
        check_package_policies(&projects, &config, &mut report);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn ignored_project_skipped_entirely() {
        let mut config = make_config(&[("Domain", &["*.Domain"])], &[], &[]);
        config.ignore_patterns = vec!["*.Tests".to_string()];
        let projects = [make_project("MyApp.Tests", &["MyApp.Domain"], &[])];
        let mut report = CheckReport::new();
        check_dependency_rules(&projects, &config, &mut report);
        assert!(report.violations.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn ignored_project_skipped_in_package_policies() {
        let mut config = make_config(
            &[("Domain", &["*.Domain"])],
            &[],
            &[("Domain", &["Microsoft.EntityFrameworkCore"])],
        );
        config.ignore_patterns = vec!["*.Tests".to_string()];
        let projects = [make_project(
            "MyApp.Tests",
            &[],
            &[("Microsoft.EntityFrameworkCore", "7.0.0")],
        )];
        let mut report = CheckReport::new();
        check_package_policies(&projects, &config, &mut report);
        assert!(report.violations.is_empty());
    }
}

fn check_package_policies(
    projects: &[ProjectFile],
    config: &ArchitectureConfig,
    report: &mut CheckReport,
) {
    for project in projects {
        if is_ignored(&project.name, &config.ignore_patterns) {
            continue;
        }
        let Some(layer) = resolve_layer(&project.name, &config.layers) else {
            continue;
        };
        let Some(policy) = config.package_policies.iter().find(|p| p.layer == layer.name) else {
            continue;
        };

        for pkg in &project.package_refs {
            if policy.forbidden.iter().any(|f| f.eq_ignore_ascii_case(&pkg.name)) {
                let src = std::fs::read_to_string(&project.path).unwrap_or_default();
                let span_start = src.find(&pkg.name).unwrap_or(0);
                report.violations.push(Violation {
                    message: format!(
                        "Package '{}' is forbidden in layer '{}'",
                        pkg.name, layer.name
                    ),
                    src: miette::NamedSource::new(project.path.to_string_lossy(), src),
                    span: (span_start, pkg.name.len()).into(),
                });
            }
        }
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use crate::config::model::DependencyRule;

    fn make_ns_config(
        layers: &[(&str, &[&str], &[&str])],
        rules: &[(&str, &str, bool)],
    ) -> ArchitectureConfig {
        ArchitectureConfig {
            layers: layers
                .iter()
                .map(|(name, pats, ns_pats)| Layer {
                    name: name.to_string(),
                    patterns: pats.iter().map(|s| s.to_string()).collect(),
                    namespace_patterns: ns_pats.iter().map(|s| s.to_string()).collect(),
                })
                .collect(),
            dependency_rules: rules
                .iter()
                .map(|(from, to, allowed)| DependencyRule {
                    from: from.to_string(),
                    to: to.to_string(),
                    allowed: *allowed,
                })
                .collect(),
            package_policies: vec![],
            ignore_patterns: vec![],
        }
    }

    // ── resolve_layer_by_namespace ─────────────────────────────────────────────

    #[test]
    fn resolve_ns_matches_wildcard_pattern() {
        let layers = vec![Layer {
            name: "Domain".to_string(),
            patterns: vec![],
            namespace_patterns: vec!["MyApp.Domain.*".to_string()],
        }];
        assert!(resolve_layer_by_namespace("MyApp.Domain.Entities", &layers).is_some());
        assert!(resolve_layer_by_namespace("MyApp.Application.Services", &layers).is_none());
    }

    #[test]
    fn resolve_ns_matches_deep_namespace() {
        let layers = vec![Layer {
            name: "Domain".to_string(),
            patterns: vec![],
            namespace_patterns: vec!["MyApp.Domain.*".to_string()],
        }];
        assert!(resolve_layer_by_namespace("MyApp.Domain.Services.Commands", &layers).is_some());
    }

    // ── check_source_rules ─────────────────────────────────────────────────────

    #[test]
    fn source_forbidden_using_produces_violation() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("OrderService.cs"),
            "using MyApp.Infrastructure.Db;\nnamespace MyApp.Domain.Services;\npublic class X {}",
        )
        .unwrap();

        let config = make_ns_config(
            &[
                ("Domain",         &["*.Domain"],         &["MyApp.Domain.*"]),
                ("Infrastructure", &["*.Infrastructure"], &["MyApp.Infrastructure.*"]),
            ],
            &[("Domain", "Infrastructure", false)],
        );

        let mut report = CheckReport::new();
        check_source_rules(dir.path(), &config, &mut report).unwrap();
        assert_eq!(report.violations.len(), 1);
        assert!(report.violations[0].message.contains("Infrastructure"));
    }

    #[test]
    fn source_allowed_using_no_violation() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Handler.cs"),
            "using MyApp.Domain.Entities;\nnamespace MyApp.Application.Handlers;\npublic class X {}",
        )
        .unwrap();

        let config = make_ns_config(
            &[
                ("Application", &["*.Application"], &["MyApp.Application.*"]),
                ("Domain",      &["*.Domain"],      &["MyApp.Domain.*"]),
            ],
            &[("Application", "Domain", true)],
        );

        let mut report = CheckReport::new();
        check_source_rules(dir.path(), &config, &mut report).unwrap();
        assert!(report.violations.is_empty());
    }

    #[test]
    fn source_scan_skipped_when_no_namespace_patterns() {
        let dir = tempfile::tempdir().unwrap();
        let config = make_ns_config(&[("Domain", &["*.Domain"], &[])], &[]);
        let mut report = CheckReport::new();
        check_source_rules(dir.path(), &config, &mut report).unwrap();
        assert!(report.violations.is_empty());
    }

    #[test]
    fn intra_layer_using_no_violation() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Svc.cs"),
            "using MyApp.Domain.ValueObjects;\nnamespace MyApp.Domain.Services;\npublic class X {}",
        )
        .unwrap();

        let config = make_ns_config(
            &[("Domain", &["*.Domain"], &["MyApp.Domain.*"])],
            &[],
        );

        let mut report = CheckReport::new();
        check_source_rules(dir.path(), &config, &mut report).unwrap();
        assert!(report.violations.is_empty());
    }
}
