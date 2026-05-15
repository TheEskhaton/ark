use ignore::WalkBuilder;
use miette::{IntoDiagnostic, Result};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use tree_sitter::Parser;

/// A single `using` directive with its byte range in the source file.
#[derive(Debug, Clone)]
pub struct UsingDirective {
    pub namespace: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Header-level information extracted from a single .cs file via tree-sitter.
#[derive(Debug, Clone)]
pub struct CsHeader {
    pub path: PathBuf,
    /// Declared namespace (`namespace MyApp.Domain` / file-scoped variant).
    pub namespace: Option<String>,
    /// Regular `using` directives (aliases and `using static` are skipped).
    pub usings: Vec<UsingDirective>,
}

/// Parse a single .cs file and extract its namespace + using directives.
pub fn scan_file(path: &Path) -> Result<CsHeader> {
    let source = std::fs::read(path).into_diagnostic()?;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .map_err(|e| miette::miette!("tree-sitter language error: {e}"))?;

    let tree = parser
        .parse(&source, None)
        .ok_or_else(|| miette::miette!("tree-sitter failed to parse {:?}", path))?;

    let root = tree.root_node();
    let mut walk = root.walk();
    let mut namespace: Option<String> = None;
    let mut usings: Vec<UsingDirective> = Vec::new();

    for child in root.children(&mut walk) {
        match child.kind() {
            "using_directive" => {
                if let Some(ud) = extract_using_directive(child, &source) {
                    usings.push(ud);
                }
            }
            "namespace_declaration" | "file_scoped_namespace_declaration" => {
                if namespace.is_none() {
                    namespace = extract_namespace_name(child, &source);
                }
                // Everything after the namespace declaration is implementation —
                // no more header nodes to collect.
                break;
            }
            _ => {}
        }
    }

    Ok(CsHeader {
        path: path.to_path_buf(),
        namespace,
        usings,
    })
}

/// Recursively scan a directory for .cs files and extract their headers in parallel.
pub fn scan_directory(root: &Path) -> Result<Vec<CsHeader>> {
    let files: Vec<PathBuf> = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .require_git(false)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext.eq_ignore_ascii_case("cs"))
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    tracing::debug!("Found {} .cs files under {:?}", files.len(), root);

    let headers = files
        .par_iter()
        .filter_map(|p| {
            scan_file(p)
                .map_err(|e| tracing::warn!("Skipping {:?}: {e}", p))
                .ok()
        })
        .collect();

    Ok(headers)
}

/// Extract the imported namespace from a `using_directive` node, including its byte range.
/// Returns `None` for `using static …` and `using Alias = …`.
fn extract_using_directive(node: tree_sitter::Node, source: &[u8]) -> Option<UsingDirective> {
    let raw = node.utf8_text(source).ok()?;
    let inner = raw.trim().strip_prefix("using")?.trim();

    if inner.starts_with("static ") || inner.contains('=') {
        return None;
    }

    let name = inner.trim_end_matches(';').trim();
    if name.is_empty() {
        return None;
    }

    let name_bytes = name.as_bytes();
    let node_src = &source[node.start_byte()..node.end_byte()];
    let offset = node_src
        .windows(name_bytes.len())
        .position(|w| w == name_bytes)
        .unwrap_or(0);

    Some(UsingDirective {
        namespace: name.to_string(),
        start_byte: node.start_byte() + offset,
        end_byte: node.start_byte() + offset + name.len(),
    })
}

/// Extract the declared namespace name from a `namespace_declaration` or
/// `file_scoped_namespace_declaration` node.
fn extract_namespace_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "identifier" | "qualified_name") {
            return child.utf8_text(source).ok().map(str::to_owned);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_cs(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, content).unwrap();
        p
    }

    fn ns(usings: &[UsingDirective]) -> Vec<&str> {
        usings.iter().map(|u| u.namespace.as_str()).collect()
    }

    #[test]
    fn extracts_namespace_and_usings() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_cs(
            dir.path(),
            "OrderService.cs",
            r#"
using System;
using System.Collections.Generic;
using MyApp.Domain.Entities;

namespace MyApp.Application.Services
{
    public class OrderService {}
}
"#,
        );
        let h = scan_file(&path).unwrap();
        assert_eq!(h.namespace.as_deref(), Some("MyApp.Application.Services"));
        assert_eq!(
            ns(&h.usings),
            vec![
                "System",
                "System.Collections.Generic",
                "MyApp.Domain.Entities"
            ]
        );
    }

    #[test]
    fn file_scoped_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_cs(
            dir.path(),
            "Repo.cs",
            r#"
using MyApp.Domain.Interfaces;

namespace MyApp.Infrastructure.Persistence;

public class OrderRepository {}
"#,
        );
        let h = scan_file(&path).unwrap();
        assert_eq!(
            h.namespace.as_deref(),
            Some("MyApp.Infrastructure.Persistence")
        );
        assert_eq!(ns(&h.usings), vec!["MyApp.Domain.Interfaces"]);
    }

    #[test]
    fn no_namespace_still_collects_usings() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_cs(
            dir.path(),
            "Script.cs",
            "using System;\npublic class Foo {}",
        );
        let h = scan_file(&path).unwrap();
        assert!(h.namespace.is_none());
        assert_eq!(ns(&h.usings), vec!["System"]);
    }

    #[test]
    fn skips_using_static() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_cs(
            dir.path(),
            "Calc.cs",
            "using static System.Math;\nnamespace A;\npublic class X {}",
        );
        let h = scan_file(&path).unwrap();
        assert!(h.usings.is_empty());
    }

    #[test]
    fn skips_using_alias() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_cs(
            dir.path(),
            "Alias.cs",
            "using Col = System.Collections.Generic;\nnamespace A;\npublic class X {}",
        );
        let h = scan_file(&path).unwrap();
        assert!(h.usings.is_empty());
    }

    #[test]
    fn scan_directory_finds_cs_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("A.cs"), "namespace X;").unwrap();
        fs::write(dir.path().join("B.cs"), "namespace Y;").unwrap();
        fs::write(dir.path().join("C.txt"), "not cs").unwrap();

        let headers = scan_directory(dir.path()).unwrap();
        assert_eq!(headers.len(), 2);
    }

    #[test]
    fn missing_file_returns_error() {
        let result = scan_file(Path::new("/nonexistent/file.cs"));
        assert!(result.is_err());
    }

    #[test]
    fn using_directive_has_correct_byte_range() {
        let dir = tempfile::tempdir().unwrap();
        let src = "using MyApp.Domain.Entities;\nnamespace MyApp.Application;\npublic class X {}";
        let path = write_cs(dir.path(), "Test.cs", src);
        let h = scan_file(&path).unwrap();
        assert_eq!(h.usings.len(), 1);
        let u = &h.usings[0];
        assert_eq!(u.namespace, "MyApp.Domain.Entities");
        assert_eq!(&src[u.start_byte..u.end_byte], "MyApp.Domain.Entities");
    }
}
