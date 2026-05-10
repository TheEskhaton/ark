use std::collections::HashMap;
use std::path::PathBuf;
use petgraph::graph::{DiGraph, NodeIndex};
use crate::parser::ProjectFile;

#[allow(dead_code)]
pub struct SolutionGraph {
    pub graph: DiGraph<String, ()>,
    pub name_to_idx: HashMap<String, NodeIndex>,
}

impl SolutionGraph {
    pub fn build(projects: &[ProjectFile]) -> Self {
        let mut graph: DiGraph<String, ()> = DiGraph::new();
        let mut name_to_idx: HashMap<String, NodeIndex> = HashMap::new();

        // Add all nodes first
        for p in projects {
            let idx = graph.add_node(p.name.clone());
            name_to_idx.insert(p.name.clone(), idx);
        }

        // Add edges
        for p in projects {
            let Some(&from) = name_to_idx.get(&p.name) else { continue };

            for pref in &p.project_refs {
                // Derive the target project name from the resolved path stem
                // (same approach as check.rs — avoids Windows canonicalization quirks)
                let target_name = pref
                    .resolved
                    .as_ref()
                    .and_then(|r| r.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| {
                        PathBuf::from(&pref.include)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| pref.include.clone())
                    });

                let to = name_to_idx
                    .entry(target_name.clone())
                    .or_insert_with(|| graph.add_node(target_name));
                graph.add_edge(from, *to, ());
            }
        }

        SolutionGraph { graph, name_to_idx }
    }

    pub fn to_mermaid(&self) -> String {
        let mut out = String::from("graph TD\n");
        for edge in self.graph.edge_indices() {
            let (a, b) = self.graph.edge_endpoints(edge).unwrap();
            out.push_str(&format!(
                "    {} --> {}\n",
                sanitize_id(&self.graph[a]),
                sanitize_id(&self.graph[b])
            ));
        }
        out
    }

    pub fn to_dot(&self) -> String {
        format!("{:?}", petgraph::dot::Dot::with_config(
            &self.graph,
            &[petgraph::dot::Config::EdgeNoLabel],
        ))
    }
}

fn sanitize_id(s: &str) -> String {
    s.replace(['.', '-', ' '], "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::csproj::ProjectRef;
    use crate::parser::ProjectFile;

    fn proj(name: &str, refs: Vec<&str>) -> ProjectFile {
        ProjectFile {
            path: PathBuf::from(format!("{name}.csproj")),
            name: name.to_string(),
            project_refs: refs
                .into_iter()
                .map(|r| ProjectRef {
                    // Use ".csproj" suffix so file_stem() strips it cleanly
                    include: format!("{r}.csproj"),
                    resolved: None,
                })
                .collect(),
            package_refs: vec![],
        }
    }

    #[test]
    fn empty_input_builds_empty_graph() {
        let sg = SolutionGraph::build(&[]);
        assert_eq!(sg.graph.node_count(), 0);
        assert_eq!(sg.graph.edge_count(), 0);
    }

    #[test]
    fn nodes_added_for_each_project() {
        let sg = SolutionGraph::build(&[proj("App.Api", vec![]), proj("App.Domain", vec![])]);
        assert_eq!(sg.graph.node_count(), 2);
        assert_eq!(sg.graph.edge_count(), 0);
    }

    #[test]
    fn edge_added_for_project_reference() {
        let sg = SolutionGraph::build(&[
            proj("App.Api", vec!["App.Domain"]),
            proj("App.Domain", vec![]),
        ]);
        assert_eq!(sg.graph.edge_count(), 1);
    }

    #[test]
    fn ref_to_unknown_project_creates_phantom_node() {
        let sg = SolutionGraph::build(&[proj("App.Api", vec!["External.Lib"])]);
        assert_eq!(sg.graph.node_count(), 2);
        assert_eq!(sg.graph.edge_count(), 1);
    }

    #[test]
    fn to_mermaid_starts_with_header() {
        let sg = SolutionGraph::build(&[]);
        assert!(sg.to_mermaid().starts_with("graph TD\n"));
    }

    #[test]
    fn to_mermaid_empty_graph_has_no_edges() {
        let sg = SolutionGraph::build(&[proj("App.Api", vec![])]);
        let m = sg.to_mermaid();
        assert!(!m.contains("-->"));
    }

    #[test]
    fn to_mermaid_edge_uses_sanitized_ids() {
        let sg = SolutionGraph::build(&[
            proj("App.Api", vec!["App.Domain"]),
            proj("App.Domain", vec![]),
        ]);
        let m = sg.to_mermaid();
        assert!(m.contains("-->"));
        assert!(m.contains("App_Api"));
        assert!(m.contains("App_Domain"));
    }

    #[test]
    fn to_dot_contains_digraph_keyword() {
        let sg = SolutionGraph::build(&[]);
        assert!(sg.to_dot().contains("digraph"));
    }

    #[test]
    fn sanitize_replaces_dots_dashes_spaces() {
        assert_eq!(sanitize_id("My.App-Web Service"), "My_App_Web_Service");
    }

    #[test]
    fn sanitize_leaves_plain_names_unchanged() {
        assert_eq!(sanitize_id("MyApp"), "MyApp");
    }
}
