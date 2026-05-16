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

pub fn scan(projects: &[ProjectFile]) -> ScanResult {
    let (test_projects, non_test): (Vec<_>, Vec<_>) =
        projects.iter().partition(|p| is_test_project(&p.name));
    let test_names: Vec<String> = test_projects.iter().map(|p| p.name.clone()).collect();
    let non_test_set: std::collections::HashSet<String> =
        non_test.iter().map(|p| p.name.clone()).collect();

    // Build graph
    let mut graph: DiGraph<String, ()> = DiGraph::new();
    let mut name_to_idx: HashMap<String, NodeIndex> = HashMap::new();
    for p in &non_test {
        let idx = graph.add_node(p.name.clone());
        name_to_idx.insert(p.name.clone(), idx);
    }
    for p in &non_test {
        let from = name_to_idx[&p.name];
        for pref in &p.project_refs {
            let target = resolve_ref_name(pref);
            if non_test_set.contains(&target) {
                if let Some(&to) = name_to_idx.get(&target) {
                    graph.add_edge(from, to, ());
                }
            }
        }
    }

    // Detect cycles via SCC
    let sccs = tarjan_scc(&graph);
    let cycles: Vec<Vec<String>> = sccs.iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| scc.iter().map(|&i| graph[i].clone()).collect())
        .collect();
    let in_cycle: std::collections::HashSet<String> =
        cycles.iter().flat_map(|g| g.iter().cloned()).collect();

    // Assign tiers iteratively (longest path from leaves)
    let mut tier_map: HashMap<String, usize> = HashMap::new();
    let mut changed = true;
    while changed {
        changed = false;
        for p in &non_test {
            if tier_map.contains_key(&p.name) || in_cycle.contains(&p.name) {
                continue;
            }
            let deps: Vec<String> = p.project_refs.iter()
                .map(|r| resolve_ref_name(r))
                .filter(|n| non_test_set.contains(n) && !in_cycle.contains(n))
                .collect();
            if deps.iter().all(|d| tier_map.contains_key(d)) {
                let t = deps.iter().filter_map(|d| tier_map.get(d)).copied().max()
                    .map(|m| m + 1).unwrap_or(0);
                tier_map.insert(p.name.clone(), t);
                changed = true;
            }
        }
    }
    // Place cycle members at max-dep-tier + 1, or above all non-cycle tiers
    let max_non_cycle_tier = tier_map.values().copied().max().unwrap_or(0);
    for group in &cycles {
        let t = group.iter()
            .flat_map(|name| non_test.iter().find(|p| &p.name == name))
            .flat_map(|p| p.project_refs.iter().map(|r| resolve_ref_name(r)))
            .filter_map(|d| tier_map.get(&d).copied())
            .max().map(|m| m + 1)
            .unwrap_or(max_non_cycle_tier + 1);
        for name in group { tier_map.insert(name.clone(), t); }
    }

    // Build tier buckets
    let max_tier = tier_map.values().copied().max().unwrap_or(0);
    let mut tiers: Vec<Vec<String>> = vec![vec![]; max_tier + 1];
    for (name, &t) in &tier_map { tiers[t].push(name.clone()); }
    for bucket in &mut tiers { bucket.sort(); }

    // Detect isolated (no in or out edges within non-test solution)
    let has_incoming: std::collections::HashSet<String> = non_test.iter()
        .flat_map(|p| p.project_refs.iter().map(|r| resolve_ref_name(r)))
        .filter(|n| non_test_set.contains(n))
        .collect();
    let isolated: Vec<String> = non_test.iter()
        .filter(|p| {
            let out_count = p.project_refs.iter()
                .map(|r| resolve_ref_name(r))
                .filter(|n| non_test_set.contains(n))
                .count();
            out_count == 0 && !has_incoming.contains(&p.name)
        })
        .map(|p| p.name.clone())
        .collect();

    ScanResult { tiers, isolated, test_projects: test_names, cycles }
}

fn resolve_ref_name(pref: &ProjectRef) -> String {
    pref.resolved.as_ref()
        .and_then(|r| r.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            PathBuf::from(&pref.include).file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| pref.include.clone())
        })
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

    use crate::parser::csproj::ProjectRef;

    fn proj(name: &str, deps: Vec<&str>) -> ProjectFile {
        ProjectFile {
            path: PathBuf::from(format!("{name}.csproj")),
            name: name.to_string(),
            project_refs: deps.into_iter().map(|r| ProjectRef {
                include: format!("..\\{r}\\{r}.csproj"),
                include_span: (0, 0),
                resolved: Some(PathBuf::from(format!("{r}.csproj"))),
            }).collect(),
            package_refs: vec![],
        }
    }

    #[test]
    fn leaf_is_tier_0() {
        let result = scan(&[proj("MyApp.Domain", vec![])]);
        assert_eq!(result.tiers.len(), 1);
        assert!(result.tiers[0].contains(&"MyApp.Domain".to_string()));
    }

    #[test]
    fn two_tier_chain() {
        let result = scan(&[proj("MyApp.Api", vec!["MyApp.Domain"]), proj("MyApp.Domain", vec![])]);
        assert_eq!(result.tiers.len(), 2);
        assert!(result.tiers[0].contains(&"MyApp.Domain".to_string()));
        assert!(result.tiers[1].contains(&"MyApp.Api".to_string()));
    }

    #[test]
    fn test_projects_removed_from_tiers() {
        let result = scan(&[proj("MyApp.Domain", vec![]), proj("MyApp.Tests", vec!["MyApp.Domain"])]);
        assert_eq!(result.test_projects, vec!["MyApp.Tests"]);
        assert_eq!(result.tiers.len(), 1);
    }

    #[test]
    fn isolated_project_flagged() {
        let result = scan(&[
            proj("MyApp.Api", vec!["MyApp.Domain"]),
            proj("MyApp.Domain", vec![]),
            proj("MyApp.BuildTools", vec![]),
        ]);
        assert!(result.isolated.contains(&"MyApp.BuildTools".to_string()));
        assert!(!result.isolated.contains(&"MyApp.Domain".to_string()));
    }

    #[test]
    fn cycle_detected() {
        let result = scan(&[proj("A", vec!["B"]), proj("B", vec!["A"])]);
        assert_eq!(result.cycles.len(), 1);
        let cycle = &result.cycles[0];
        assert!(cycle.contains(&"A".to_string()));
        assert!(cycle.contains(&"B".to_string()));
    }

    #[test]
    fn pure_cycle_placed_above_non_cycle_projects() {
        // A ↔ B cycle with no external deps — should NOT land in tier 0
        let projects = vec![
            proj("MyApp.Domain", vec![]),      // tier 0 (leaf)
            proj("A", vec!["B"]),              // cycle
            proj("B", vec!["A"]),              // cycle
        ];
        let result = scan(&projects);
        assert_eq!(result.cycles.len(), 1);
        // Domain should be at tier 0; A and B should be above it
        let domain_tier = result.tiers.iter().position(|t| t.contains(&"MyApp.Domain".to_string())).unwrap();
        let a_tier = result.tiers.iter().position(|t| t.contains(&"A".to_string())).unwrap();
        assert!(a_tier > domain_tier, "cycle members should be placed above genuine leaf projects");
    }
}
