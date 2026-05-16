# ark init — Intelligent Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the static `ark init` template with a 4-phase interactive wizard that detects architectural tiers from the dependency graph and generates a grounded `architecture.toml`.

**Architecture:** Scan builds a DAG from `.csproj` refs, ranks projects by topological depth into tiers, then a layer wizard (bottom-to-top) and rules wizard (inter-layer edges) guide the user to a confirmed config which the generator serializes to TOML.

**Tech Stack:** Rust, petgraph (existing — used for SCC + topology), dialoguer 0.11 (new — interactive prompts), existing discovery/parser infrastructure.

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Delete | `src/commands/init.rs` | replaced by module below |
| Create | `src/commands/init/mod.rs` | orchestrates phases, `pub fn run()` |
| Create | `src/commands/init/scan.rs` | tier computation, cycle detection, test filtering, inter-layer edges |
| Create | `src/commands/init/wizard.rs` | all dialoguer prompts |
| Create | `src/commands/init/generator.rs` | `WizardAnswers`, pattern inference, TOML string building |
| Modify | `Cargo.toml` | add dialoguer |
| Modify | `README.md` | update ark init docs |

Types that flow between modules:
- `scan.rs` exports: `ScanResult`, `LayerDef`, `InterLayerEdge`, `scan()`, `compute_inter_layer_edges()`, `suggest_layer_name()`
- `generator.rs` exports: `WizardAnswers`, `build_toml()`, `infer_patterns()`
- `wizard.rs` exports: `run_layer_wizard()`, `run_rules_wizard()`, `run_finish_wizard()`
- `mod.rs` wires them together

---

### Task 1: Add dialoguer dependency

**Files:** `Cargo.toml`

- [ ] Add to `[dependencies]` after the clap entry:
```toml
# Interactive wizard prompts
dialoguer = "0.11"
```
- [ ] Run: `cargo check` — Expected: no errors
- [ ] Commit:
```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add dialoguer for interactive wizard"
```

---

### Task 2: `scan.rs` — data types and test filtering

**Files:** Create `src/commands/init/scan.rs`

- [ ] Write failing tests:
```rust
// src/commands/init/scan.rs
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
```
- [ ] Run: `cargo test` — Expected: FAIL (not defined)
- [ ] Implement:
```rust
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
```
- [ ] Run: `cargo test` — Expected: PASS
- [ ] Commit:
```bash
git add src/commands/init/scan.rs
git commit -m "feat(init): scan data types and test project detection"
```

---

### Task 3: `scan.rs` — tier computation

**Files:** Modify `src/commands/init/scan.rs`

- [ ] Add failing tests (append to `tests` mod):
```rust
    use crate::parser::csproj::{PackageRef, ProjectRef};

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
```
- [ ] Run: `cargo test` — Expected: FAIL
- [ ] Implement `scan()` and `resolve_ref_name()`:
```rust
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
    // Place cycle members at max-dep-tier + 1
    for group in &cycles {
        let t = group.iter()
            .flat_map(|name| non_test.iter().find(|p| &p.name == name))
            .flat_map(|p| p.project_refs.iter().map(|r| resolve_ref_name(r)))
            .filter_map(|d| tier_map.get(&d).copied())
            .max().map(|m| m + 1).unwrap_or(0);
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
```
- [ ] Run: `cargo test` — Expected: all PASS
- [ ] Commit:
```bash
git add src/commands/init/scan.rs
git commit -m "feat(init): topology scan and tier assignment"
```

---

### Task 4: `scan.rs` — layer name suggestion + inter-layer edges

**Files:** Modify `src/commands/init/scan.rs`

- [ ] Add failing tests:
```rust
    #[test]
    fn suggests_domain() {
        assert_eq!(suggest_layer_name(&["MyApp.Domain", "MyApp.Core"]), "Domain");
    }
    #[test]
    fn suggests_presentation() {
        assert_eq!(suggest_layer_name(&["MyApp.Api"]), "Presentation");
    }
    #[test]
    fn fallback_layer() {
        assert_eq!(suggest_layer_name(&["MyApp.Weird"]), "Layer");
    }

    #[test]
    fn inter_layer_edges_counted() {
        let projects = vec![proj("MyApp.Api", vec!["MyApp.Domain"]), proj("MyApp.Domain", vec![])];
        let layers = vec![
            LayerDef { name: "Domain".into(), projects: vec!["MyApp.Domain".into()] },
            LayerDef { name: "Presentation".into(), projects: vec!["MyApp.Api".into()] },
        ];
        let edges = compute_inter_layer_edges(&layers, &projects);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from, "Presentation");
        assert_eq!(edges[0].to, "Domain");
        assert_eq!(edges[0].ref_count, 1);
        assert!(!edges[0].unusual);
    }

    #[test]
    fn unusual_edge_flagged() {
        // Domain (idx 0) depends on Presentation (idx 1) — lower depends on higher = unusual
        let projects = vec![proj("MyApp.Domain", vec!["MyApp.Api"]), proj("MyApp.Api", vec![])];
        let layers = vec![
            LayerDef { name: "Domain".into(), projects: vec!["MyApp.Domain".into()] },
            LayerDef { name: "Presentation".into(), projects: vec!["MyApp.Api".into()] },
        ];
        let edges = compute_inter_layer_edges(&layers, &projects);
        assert_eq!(edges.len(), 1);
        assert!(edges[0].unusual);
    }

    #[test]
    fn same_layer_refs_excluded() {
        let projects = vec![proj("MyApp.Domain", vec!["MyApp.Core"]), proj("MyApp.Core", vec![])];
        let layers = vec![
            LayerDef { name: "Domain".into(), projects: vec!["MyApp.Domain".into(), "MyApp.Core".into()] },
        ];
        assert!(compute_inter_layer_edges(&layers, &projects).is_empty());
    }
```
- [ ] Run: `cargo test` — Expected: FAIL
- [ ] Implement both functions:
```rust
pub fn suggest_layer_name(projects: &[&str]) -> &'static str {
    let score = |hints: &[&str]| projects.iter().filter(|p| hints.iter().any(|h| p.ends_with(h))).count();
    [
        (score(&[".Domain", ".Core", ".Entities"]), "Domain"),
        (score(&[".Application", ".UseCases", ".Services"]), "Application"),
        (score(&[".Infrastructure", ".Persistence", ".Adapters"]), "Infrastructure"),
        (score(&[".Api", ".Web", ".Host"]), "Presentation"),
    ]
    .iter()
    .filter(|(s, _)| *s > 0)
    .max_by_key(|(s, _)| *s)
    .map(|(_, name)| *name)
    .unwrap_or("Layer")
}

pub fn compute_inter_layer_edges(layers: &[LayerDef], projects: &[ProjectFile]) -> Vec<InterLayerEdge> {
    let project_to_layer: HashMap<String, usize> = layers.iter().enumerate()
        .flat_map(|(i, l)| l.projects.iter().map(move |p| (p.clone(), i)))
        .collect();

    let mut counts: HashMap<(usize, usize), usize> = HashMap::new();
    for p in projects {
        let Some(&from_idx) = project_to_layer.get(&p.name) else { continue };
        for pref in &p.project_refs {
            let target = resolve_ref_name(pref);
            let Some(&to_idx) = project_to_layer.get(&target) else { continue };
            if from_idx != to_idx {
                *counts.entry((from_idx, to_idx)).or_insert(0) += 1;
            }
        }
    }

    let mut edges: Vec<InterLayerEdge> = counts.into_iter().map(|((fi, ti), count)| InterLayerEdge {
        from: layers[fi].name.clone(),
        to: layers[ti].name.clone(),
        ref_count: count,
        unusual: fi < ti,
    }).collect();
    edges.sort_by(|a, b| a.from.cmp(&b.from).then(a.to.cmp(&b.to)));
    edges
}
```
- [ ] Run: `cargo test` — Expected: all PASS
- [ ] Commit:
```bash
git add src/commands/init/scan.rs
git commit -m "feat(init): layer name suggestion and inter-layer edge computation"
```

---

### Task 5: `generator.rs` — pattern inference and TOML generation

**Files:** Create `src/commands/init/generator.rs`

- [ ] Write failing tests:
```rust
// src/commands/init/generator.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init::scan::LayerDef;

    #[test]
    fn infers_glob_from_suffix() {
        let patterns = infer_patterns(&["MyApp.Domain".into(), "OtherApp.Domain".into()]);
        assert_eq!(patterns, vec!["*.Domain"]);
    }

    #[test]
    fn multiple_suffixes_multiple_patterns() {
        let patterns = infer_patterns(&["MyApp.Domain".into(), "MyApp.Core".into()]);
        assert!(patterns.contains(&"*.Domain".to_string()));
        assert!(patterns.contains(&"*.Core".to_string()));
    }

    #[test]
    fn no_dot_uses_exact_name() {
        let patterns = infer_patterns(&["Shared".into()]);
        assert_eq!(patterns, vec!["Shared"]);
    }

    #[test]
    fn build_toml_contains_expected_sections() {
        let answers = WizardAnswers {
            layers: vec![
                LayerDef { name: "Domain".into(), projects: vec!["MyApp.Domain".into()] },
                LayerDef { name: "Presentation".into(), projects: vec!["MyApp.Api".into()] },
            ],
            rules: vec![("Presentation".into(), "Domain".into(), true)],
            ignore_patterns: vec!["*.Tests".into()],
            package_policies: vec![],
        };
        let toml = build_toml(&answers).unwrap();
        assert!(toml.contains("name = \"Domain\""));
        assert!(toml.contains("name = \"Presentation\""));
        assert!(toml.contains("allowed = true"));
        assert!(toml.contains("*.Tests"));
    }
}
```
- [ ] Run: `cargo test` — Expected: FAIL
- [ ] Implement:
```rust
use crate::commands::init::scan::LayerDef;
use miette::Result;

pub struct WizardAnswers {
    pub layers: Vec<LayerDef>,
    pub rules: Vec<(String, String, bool)>,
    pub ignore_patterns: Vec<String>,
    pub package_policies: Vec<(String, String)>,
}

pub fn build_toml(answers: &WizardAnswers) -> Result<String> {
    let mut out = String::from("# Generated by `ark init`\n\n");

    out.push_str("layers = [\n");
    for layer in &answers.layers {
        let pats = infer_patterns(&layer.projects).iter()
            .map(|p| format!("\"{}\"", p)).collect::<Vec<_>>().join(", ");
        out.push_str(&format!("  {{ name = \"{}\", patterns = [{}] }},\n", layer.name, pats));
    }
    out.push_str("]\n\n");

    out.push_str("# Any dependency not listed here is forbidden by default.\n");
    out.push_str("dependency_rules = [\n");
    for (from, to, allowed) in &answers.rules {
        out.push_str(&format!("  {{ from = \"{}\", to = \"{}\", allowed = {} }},\n", from, to, allowed));
    }
    out.push_str("]\n");

    if !answers.package_policies.is_empty() {
        out.push('\n');
        out.push_str("package_policies = [\n");
        for (layer, pkg) in &answers.package_policies {
            out.push_str(&format!("  {{ layer = \"{}\", forbidden = [\"{}\"] }},\n", layer, pkg));
        }
        out.push_str("]\n");
    }

    if !answers.ignore_patterns.is_empty() {
        out.push('\n');
        let pats = answers.ignore_patterns.iter()
            .map(|p| format!("\"{}\"", p)).collect::<Vec<_>>().join(", ");
        out.push_str(&format!("ignore_patterns = [{}]\n", pats));
    }

    Ok(out)
}

pub fn infer_patterns(projects: &[String]) -> Vec<String> {
    let mut patterns: Vec<String> = Vec::new();
    for project in projects {
        let pat = if let Some(pos) = project.rfind('.') {
            format!("*{}", &project[pos..])
        } else {
            project.clone()
        };
        if !patterns.contains(&pat) {
            patterns.push(pat);
        }
    }
    patterns
}
```
- [ ] Run: `cargo test` — Expected: all PASS
- [ ] Commit:
```bash
git add src/commands/init/generator.rs
git commit -m "feat(init): TOML generator with pattern inference"
```

---

### Task 6: `wizard.rs` — all interactive phases

**Files:** Create `src/commands/init/wizard.rs`

- [ ] Implement (no unit tests — dialoguer requires a TTY; verified manually):
```rust
use crate::commands::init::generator::WizardAnswers;
use crate::commands::init::scan::{suggest_layer_name, InterLayerEdge, LayerDef, ScanResult};
use dialoguer::{Confirm, Input, MultiSelect, Select};
use miette::{IntoDiagnostic, Result};

pub fn print_phase(step: usize, total: usize, label: &str) {
    println!("\n─── Step {}/{}: {} ", step, total, label);
    println!("{}", "─".repeat(60));
}

pub fn run_layer_wizard(scan: &ScanResult) -> Result<Vec<LayerDef>> {
    print_phase(3, 4, "Confirming layers");

    if !scan.cycles.is_empty() {
        println!("\n⚠  Circular dependencies detected:");
        for group in &scan.cycles {
            println!("   {}", group.join(" ↔ "));
        }
        println!("   Consider resolving these — they usually indicate layer boundary violations.\n");
    }

    let mut confirmed: Vec<LayerDef> = Vec::new();
    let mut pending: Vec<String> = Vec::new();

    for (tier_idx, tier_projects) in scan.tiers.iter().enumerate() {
        if tier_idx == 0 {
            println!("\nThese projects have no dependencies on other solution projects.");
            println!("They are likely your innermost layer (Domain, Core, etc.)\n");
        } else {
            println!("\nThese projects depend on the layer(s) below:\n");
        }
        for p in tier_projects { println!("  {}", p); }
        println!();

        let refs: Vec<&str> = tier_projects.iter().map(|s| s.as_str()).collect();
        let suggestion = suggest_layer_name(&refs);
        let name: String = Input::new()
            .with_prompt(format!("Layer name [{}]", suggestion))
            .default(suggestion.to_string())
            .interact_text().into_diagnostic()?;

        let mut layer_projects = tier_projects.clone();

        if Confirm::new().with_prompt("Move any projects to a different layer?")
            .default(false).interact().into_diagnostic()?
        {
            let selections = MultiSelect::new()
                .with_prompt("Select projects to move")
                .items(tier_projects)
                .interact().into_diagnostic()?;
            let to_move: Vec<String> = selections.iter().map(|&i| tier_projects[i].clone()).collect();

            if confirmed.is_empty() {
                pending.extend(to_move.iter().cloned());
            } else {
                let layer_names: Vec<&str> = confirmed.iter().map(|l| l.name.as_str()).collect();
                for proj in &to_move {
                    let idx = Select::new()
                        .with_prompt(format!("Move '{}' to", proj))
                        .items(&layer_names).interact().into_diagnostic()?;
                    confirmed[idx].projects.push(proj.clone());
                }
            }
            layer_projects.retain(|p| !to_move.contains(p));
        }

        confirmed.push(LayerDef { name, projects: layer_projects });
    }

    // Isolated projects
    if !scan.isolated.is_empty() {
        println!("\nThese projects have no project references at all:\n");
        for p in &scan.isolated { println!("  {}", p); }
        println!();
        let mut choices: Vec<String> = confirmed.iter().map(|l| l.name.clone()).collect();
        choices.push("ignore".to_string());
        for proj in &scan.isolated {
            let idx = Select::new()
                .with_prompt(format!("Assign '{}' to a layer or ignore?", proj))
                .items(&choices).default(choices.len() - 1)
                .interact().into_diagnostic()?;
            if idx < confirmed.len() { confirmed[idx].projects.push(proj.clone()); }
        }
    }

    // Pending reassignments from tier 0 when no layers existed yet
    if !pending.is_empty() {
        let layer_names: Vec<&str> = confirmed.iter().map(|l| l.name.as_str()).collect();
        for proj in &pending {
            let idx = Select::new()
                .with_prompt(format!("Assign '{}' to a layer?", proj))
                .items(&layer_names).interact().into_diagnostic()?;
            confirmed[idx].projects.push(proj.clone());
        }
    }

    Ok(confirmed)
}

pub fn run_rules_wizard(edges: &[InterLayerEdge]) -> Result<Vec<(String, String, bool)>> {
    print_phase(4, 4, "Reviewing dependency rules");
    println!("For each dependency between layers, choose whether to allow or forbid it.");
    println!("Rules not listed here are forbidden by default.\n");

    if edges.is_empty() {
        println!("No inter-layer dependencies detected. All cross-layer rules will be forbidden by default.");
        return Ok(vec![]);
    }

    let mut rules = Vec::new();
    for edge in edges {
        let unusual = if edge.unusual { "  ← unusual" } else { "" };
        let ref_word = if edge.ref_count == 1 { "reference" } else { "references" };
        println!("  {:20} → {:20} ({} {}){}", edge.from, edge.to, edge.ref_count, ref_word, unusual);
        let allowed = Confirm::new()
            .with_prompt("  Allow?")
            .default(!edge.unusual)
            .interact().into_diagnostic()?;
        rules.push((edge.from.clone(), edge.to.clone(), allowed));
        println!();
    }
    Ok(rules)
}

pub fn run_finish_wizard(
    test_projects: &[String],
    layers: &[LayerDef],
) -> Result<(Vec<String>, Vec<(String, String)>)> {
    println!("\n─── Finishing up ");
    println!("{}", "─".repeat(60));

    let mut ignore_patterns = Vec::new();
    if !test_projects.is_empty() {
        println!("\nDetected test/spec projects (suggested for ignore_patterns):\n");
        for p in test_projects { println!("  {}", p); }
        println!();
        if Confirm::new().with_prompt("Add these to ignore_patterns?").default(true)
            .interact().into_diagnostic()?
        {
            ignore_patterns.extend(test_projects.iter().cloned());
        }
    }

    let mut package_policies = Vec::new();
    if Confirm::new()
        .with_prompt("\nAdd package policies? (e.g. forbid EF Core in Domain layer)")
        .default(false).interact().into_diagnostic()?
    {
        let layer_names: Vec<&str> = layers.iter().map(|l| l.name.as_str()).collect();
        loop {
            let idx = Select::new().with_prompt("Which layer?")
                .items(&layer_names).interact().into_diagnostic()?;
            let pkg: String = Input::new()
                .with_prompt("Package name to forbid")
                .interact_text().into_diagnostic()?;
            package_policies.push((layers[idx].name.clone(), pkg));
            if !Confirm::new().with_prompt("Add another?").default(false)
                .interact().into_diagnostic()?
            { break; }
        }
    }

    Ok((ignore_patterns, package_policies))
}
```
- [ ] Run: `cargo check` — Expected: no errors
- [ ] Commit:
```bash
git add src/commands/init/wizard.rs
git commit -m "feat(init): interactive wizard phases (layer, rules, finish)"
```

---

### Task 7: `mod.rs` — orchestration; delete old `init.rs`

**Files:** Create `src/commands/init/mod.rs`, delete `src/commands/init.rs`

- [ ] Create `src/commands/init/mod.rs`:
```rust
mod generator;
mod scan;
mod wizard;

use generator::{build_toml, WizardAnswers};
use miette::{miette, IntoDiagnostic, Result};
use scan::compute_inter_layer_edges;
use std::path::Path;
use wizard::{print_phase, run_finish_wizard, run_layer_wizard, run_rules_wizard};

pub fn run(root: &str) -> Result<()> {
    let dest = Path::new(root).join("architecture.toml");
    if dest.exists() {
        return Err(miette!(
            "architecture.toml already exists at {:?}. Delete it first or edit it directly.",
            dest
        ));
    }

    println!("ark init — intelligent architecture scaffold");
    println!("{}", "─".repeat(44));
    println!("Scanning your solution to detect architectural layers.\n");

    println!("Step 1/4  Scanning projects...");
    let project_paths = crate::parser::discovery::discover_projects(Path::new(root))?;
    let projects: Vec<_> = project_paths.iter()
        .map(|p| crate::parser::csproj::ProjectFile::parse(p))
        .collect::<Result<Vec<_>>>()?;
    println!("          ✓  {} projects found", projects.len());

    println!("Step 2/4  Ranking by dependency graph...");
    let scan_result = scan::scan(&projects);
    println!("          ✓  {} tiers detected, {} test projects filtered",
        scan_result.tiers.len(), scan_result.test_projects.len());

    let layers = run_layer_wizard(&scan_result)?;
    let edges = compute_inter_layer_edges(&layers, &projects);
    let rules = run_rules_wizard(&edges)?;
    let (ignore_patterns, package_policies) = run_finish_wizard(&scan_result.test_projects, &layers)?;

    let answers = WizardAnswers { layers, rules, ignore_patterns, package_policies };
    let toml_content = build_toml(&answers)?;

    println!("\n─── Preview ");
    println!("{}", "─".repeat(60));
    println!("{}", toml_content);

    if dialoguer::Confirm::new().with_prompt("Write architecture.toml?")
        .default(true).interact().into_diagnostic()?
    {
        std::fs::write(&dest, &toml_content).into_diagnostic()?;
        println!("\n✓  Created {:?}", dest);
        println!("   Run `ark check` to see your current violations.");
        println!("   Run `ark baseline` to snapshot them if you want a clean starting point.");
    } else {
        println!("Aborted. No file written.");
    }

    Ok(())
}
```
- [ ] Delete `src/commands/init.rs`:
```bash
git rm src/commands/init.rs
```
- [ ] Run: `cargo check` — Rust resolves `pub mod init` to `init/mod.rs` automatically. Expected: no errors
- [ ] Run: `cargo test` — Expected: all PASS
- [ ] Commit:
```bash
git add src/commands/init/
git commit -m "feat(init): wire up scaffold wizard — replace static template"
```

---

### Task 8: Update README

**Files:** Modify `README.md`

- [ ] Replace the `### \`ark init\`` section with:
```markdown
### `ark init`

Interactively scaffold `architecture.toml` from your real solution structure.

```bash
ark init
```

The wizard:
1. Scans all `.csproj` files and builds a dependency graph
2. Groups projects into architectural tiers by topological depth
3. Walks you through naming each tier as a layer
4. Reviews each detected inter-layer dependency — allow or forbid
5. Writes `architecture.toml` tailored to your solution

For brownfield teams with existing violations, run `ark baseline` right after to snapshot the current state.
```

- [ ] Commit:
```bash
git add README.md
git commit -m "docs: update ark init docs for scaffold wizard"
```
