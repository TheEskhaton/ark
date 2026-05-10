use std::path::Path;
use miette::{IntoDiagnostic, Result};
use rayon::prelude::*;

use crate::config::load_config;
use crate::graph::SolutionGraph;
use crate::parser::{discover_projects, ProjectFile};

pub async fn run(root: &str, config_path: &str, format: &str, output: Option<&str>) -> Result<()> {
    let root = Path::new(root);

    // Config is optional for the graph command
    let _config = load_config(Path::new(config_path)).await.ok();

    let project_paths = discover_projects(root)?;

    let projects: Vec<ProjectFile> = project_paths
        .par_iter()
        .filter_map(|p| ProjectFile::parse(p).ok())
        .collect();

    let solution = SolutionGraph::build(&projects);

    let rendered = match format {
        "dot" => solution.to_dot(),
        _ => solution.to_mermaid(),
    };

    match output {
        Some(path) => {
            std::fs::write(path, &rendered).into_diagnostic()?;
            eprintln!("Graph written to {path}");
        }
        None => print!("{rendered}"),
    }

    Ok(())
}
