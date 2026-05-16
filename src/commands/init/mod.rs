mod generator;
mod scan;
mod wizard;

use generator::{WizardAnswers, build_toml};
use miette::{IntoDiagnostic, Result, miette};
use scan::compute_inter_layer_edges;
use std::path::Path;
use wizard::{run_finish_wizard, run_layer_wizard, run_rules_wizard};

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
    let projects: Vec<_> = project_paths
        .iter()
        .map(|p| crate::parser::csproj::ProjectFile::parse(p))
        .collect::<Result<Vec<_>>>()?;
    println!("          ✓  {} projects found", projects.len());

    println!("Step 2/4  Ranking by dependency graph...");
    let scan_result = scan::scan(&projects);
    println!(
        "          ✓  {} tiers detected, {} test projects filtered",
        scan_result.tiers.len(),
        scan_result.test_projects.len()
    );

    let layers = run_layer_wizard(&scan_result)?;
    let edges = compute_inter_layer_edges(&layers, &projects);
    let rules = run_rules_wizard(&edges)?;
    let (ignore_patterns, package_policies) =
        run_finish_wizard(&scan_result.test_projects, &layers)?;

    let answers = WizardAnswers {
        layers,
        rules,
        ignore_patterns,
        package_policies,
    };
    let toml_content = build_toml(&answers)?;

    println!("\n─── Preview ───────────────────────────────────────────────");
    println!("{}", toml_content);

    if dialoguer::Confirm::new()
        .with_prompt("Write architecture.toml?")
        .default(true)
        .interact()
        .into_diagnostic()?
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
