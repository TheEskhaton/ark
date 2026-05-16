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
        for p in tier_projects {
            println!("  {}", p);
        }
        println!();

        let refs: Vec<&str> = tier_projects.iter().map(|s| s.as_str()).collect();
        let suggestion = suggest_layer_name(&refs);
        let name: String = Input::new()
            .with_prompt(format!("Layer name [{}]", suggestion))
            .default(suggestion.to_string())
            .interact_text()
            .into_diagnostic()?;

        let mut layer_projects = tier_projects.clone();

        if Confirm::new()
            .with_prompt(if confirmed.is_empty() {
                "Move any projects out of this tier? (will be assigned after all tiers are processed)"
            } else {
                "Move any projects to an already-confirmed layer below?"
            })
            .default(false)
            .interact()
            .into_diagnostic()?
        {
            let selections = MultiSelect::new()
                .with_prompt("Select projects to move")
                .items(tier_projects)
                .interact()
                .into_diagnostic()?;
            let to_move: Vec<String> = selections
                .iter()
                .map(|&i| tier_projects[i].clone())
                .collect();

            if !to_move.is_empty() {
                if confirmed.is_empty() {
                    pending.extend(to_move.iter().cloned());
                } else {
                    let layer_names: Vec<String> =
                        confirmed.iter().map(|l| l.name.clone()).collect();
                    for proj in &to_move {
                        let idx = Select::new()
                            .with_prompt(format!("Move '{}' to", proj))
                            .items(&layer_names)
                            .interact()
                            .into_diagnostic()?;
                        confirmed[idx].projects.push(proj.clone());
                    }
                }
                layer_projects.retain(|p| !to_move.contains(p));
            }
        }

        confirmed.push(LayerDef {
            name,
            projects: layer_projects,
        });
    }

    // Isolated projects
    if !scan.isolated.is_empty() {
        println!("\nThese projects have no project references at all:\n");
        for p in &scan.isolated {
            println!("  {}", p);
        }
        println!();
        let mut choices: Vec<String> = confirmed.iter().map(|l| l.name.clone()).collect();
        choices.push("ignore".to_string());
        for proj in &scan.isolated {
            let idx = Select::new()
                .with_prompt(format!("Assign '{}' to a layer or ignore?", proj))
                .items(&choices)
                .default(choices.len() - 1)
                .interact()
                .into_diagnostic()?;
            if idx < confirmed.len() {
                confirmed[idx].projects.push(proj.clone());
            }
        }
    }

    // Pending reassignments from tier 0 before any layer existed
    if !pending.is_empty() {
        let layer_names: Vec<String> = confirmed.iter().map(|l| l.name.clone()).collect();
        for proj in &pending {
            let idx = Select::new()
                .with_prompt(format!("Assign '{}' to a layer?", proj))
                .items(&layer_names)
                .interact()
                .into_diagnostic()?;
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
        println!(
            "No inter-layer dependencies detected. All cross-layer rules will be forbidden by default."
        );
        return Ok(vec![]);
    }

    let mut rules = Vec::new();
    for edge in edges {
        let unusual = if edge.unusual { "  ← unusual" } else { "" };
        let ref_word = if edge.ref_count == 1 {
            "reference"
        } else {
            "references"
        };
        println!(
            "  {:20} → {:20} ({} {}){}",
            edge.from, edge.to, edge.ref_count, ref_word, unusual
        );
        let allowed = Confirm::new()
            .with_prompt("  Allow?")
            .default(!edge.unusual)
            .interact()
            .into_diagnostic()?;
        rules.push((edge.from.clone(), edge.to.clone(), allowed));
        println!();
    }
    Ok(rules)
}

pub fn run_finish_wizard(
    test_projects: &[String],
    layers: &[LayerDef],
) -> Result<(Vec<String>, Vec<(String, String)>)> {
    println!("\n─── Finishing up ─────────────────────────────────────────");

    let mut ignore_patterns = Vec::new();
    if !test_projects.is_empty() {
        println!("\nDetected test/spec projects (suggested for ignore_patterns):\n");
        for p in test_projects {
            println!("  {}", p);
        }
        println!();
        if Confirm::new()
            .with_prompt("Add these to ignore_patterns?")
            .default(true)
            .interact()
            .into_diagnostic()?
        {
            ignore_patterns.extend(test_projects.iter().cloned());
        }
    }

    let mut package_policies = Vec::new();
    if Confirm::new()
        .with_prompt("\nAdd package policies? (e.g. forbid EF Core in Domain layer)")
        .default(false)
        .interact()
        .into_diagnostic()?
    {
        let layer_names: Vec<&str> = layers.iter().map(|l| l.name.as_str()).collect();
        loop {
            let idx = Select::new()
                .with_prompt("Which layer?")
                .items(&layer_names)
                .interact()
                .into_diagnostic()?;
            let pkg: String = Input::new()
                .with_prompt("Package name to forbid")
                .interact_text()
                .into_diagnostic()?;
            package_policies.push((layers[idx].name.clone(), pkg));
            if !Confirm::new()
                .with_prompt("Add another?")
                .default(false)
                .interact()
                .into_diagnostic()?
            {
                break;
            }
        }
    }

    Ok((ignore_patterns, package_policies))
}
