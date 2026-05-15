# ark improvements implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement four improvements: `ark explain`, baseline suppression, accurate violation spans, and `ignorePatterns` exclusions.

**Architecture:** Extract shared layer-resolution logic to `src/rules.rs`. Build each feature on top of the existing command/config/parser/scanner structure. Baseline suppression lives in `src/baseline.rs` and integrates into `commands/check.rs` via a new `collect()` function.

**Tech Stack:** Rust, clap 4, serde_json, quick-xml, tree-sitter, miette, rayon, tempfile (tests)

---

## File map

| File | Action | Purpose |
|------|--------|---------|
| `src/rules.rs` | Create | `resolve_layer`, `resolve_layer_by_namespace`, `is_ignored` |
| `src/baseline.rs` | Create | `BaselineEntry`, `try_load`, `save`, `apply_baseline` |
| `src/commands/explain.rs` | Create | `explain` subcommand |
| `src/commands/baseline.rs` | Create | `baseline` subcommand |
| `src/config/model.rs` | Modify | Add `ignore_patterns` field |
| `src/parser/csproj.rs` | Modify | Add `include_span` to `ProjectRef`, `name_span` to `PackageRef` |
| `src/scanner/cs_scanner.rs` | Modify | Add `UsingDirective` struct, change `usings` to `Vec<UsingDirective>` |
| `src/report.rs` | Modify | Add `violation_keys: Vec<BaselineEntry>` to `CheckReport` |
| `src/commands/check.rs` | Modify | Use rules module, accurate spans, baseline filtering, ignore_patterns |
| `src/commands/init.rs` | Modify | Add `ignorePatterns` to template |
| `src/commands/mod.rs` | Modify | Declare `explain` and `baseline` modules |
| `src/main.rs` | Modify | Add `Explain`, `Baseline` commands; register `baseline` and `rules` mods |

---

## Task 1: Create src/rules.rs

Move `resolve_layer` and `resolve_layer_by_namespace` out of `check.rs` so both `check` and `explain` can share them.

**Files:**
- Create: `src/rules.rs`
- Modify: `src/commands/check.rs`

- [ ] **Step 1: Write failing tests in src/rules.rs**

```rust
// src/rules.rs
use crate::config::model::Layer;

pub fn resolve_layer<'a>(project_name: &str, layers: &'a [Layer]) -> Option<&'a Layer> {
    todo!()
}

pub fn resolve_layer_by_namespace<'a>(ns: &str, layers: &'a [Layer]) -> Option<&'a Layer> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(name: &str, patterns: &[&str]) -> Layer {
        Layer {
            name: name.to_string(),
            patterns: patterns.iter().map(|s| s.to_string()).collect(),
            namespace_patterns: vec![],
        }
    }

    #[test]
    fn resolve_layer_glob() {
        let layers = vec![layer("Domain", &["*.Domain"])];
        assert_eq!(resolve_layer("MyApp.Domain", &layers).unwrap().name, "Domain");
        assert!(resolve_layer("MyApp.Api", &layers).is_none());
    }

    #[test]
    fn resolve_layer_by_namespace_wildcard() {
        let layers = vec![Layer {
            name: "Domain".to_string(),
            patterns: vec![],
            namespace_patterns: vec!["MyApp.Domain.*".to_string()],
        }];
        assert!(resolve_layer_by_namespace("MyApp.Domain.Entities", &layers).is_some());
        assert!(resolve_layer_by_namespace("MyApp.Application.Foo", &layers).is_none());
    }
}
```

- [ ] **Step 2: Run tests — expect compile error / todo panic**

```
cargo test rules::tests
```

- [ ] **Step 3: Implement both functions**

```rust
// src/rules.rs
use crate::config::model::Layer;

pub fn resolve_layer<'a>(project_name: &str, layers: &'a [Layer]) -> Option<&'a Layer> {
    layers.iter().find(|l| {
        l.patterns.iter().any(|pat| {
            glob::Pattern::new(pat)
                .map(|p| p.matches(project_name))
                .unwrap_or(false)
        })
    })
}

pub fn resolve_layer_by_namespace<'a>(ns: &str, layers: &'a [Layer]) -> Option<&'a Layer> {
    layers.iter().find(|l| {
        l.namespace_patterns.iter().any(|pat| {
            glob::Pattern::new(pat)
                .map(|p| p.matches(ns))
                .unwrap_or(false)
        })
    })
}
```

- [ ] **Step 4: Register module in main.rs**

Add `mod rules;` to the module list in `src/main.rs` (after `mod report;`).

- [ ] **Step 5: Update check.rs to use rules module**

In `src/commands/check.rs`, remove the two private `resolve_layer*` functions and replace with:

```rust
use crate::rules::{resolve_layer, resolve_layer_by_namespace};
```

Update all call sites — the function signatures are identical so no other changes needed.

In `src/commands/check.rs`, also update the test imports (the tests for `resolve_layer` and `resolve_layer_by_namespace` in check.rs now test the rules module versions — keep them as integration tests, they'll still pass through the re-export path via `use super::*`).

- [ ] **Step 6: Run all tests**

```
cargo test
```

Expected: all existing tests pass.

- [ ] **Step 7: Commit**

```
git add src/rules.rs src/commands/check.rs src/main.rs
git commit -m "refactor: extract resolve_layer helpers to src/rules.rs"
```

---

## Task 2: Add ignore_patterns to config + is_ignored

**Files:**
- Modify: `src/config/model.rs`
- Modify: `src/rules.rs`
- Modify: `src/commands/init.rs`

- [ ] **Step 1: Write failing test for is_ignored**

Add to `src/rules.rs` tests:

```rust
#[test]
fn is_ignored_matches_glob() {
    let patterns = vec!["*.Tests".to_string(), "*.Specs".to_string()];
    assert!(is_ignored("MyApp.Tests", &patterns));
    assert!(is_ignored("MyApp.Specs", &patterns));
    assert!(!is_ignored("MyApp.Domain", &patterns));
}

#[test]
fn is_ignored_empty_patterns_never_ignores() {
    assert!(!is_ignored("MyApp.Domain", &[]));
}
```

- [ ] **Step 2: Run — expect compile error (function undefined)**

```
cargo test rules::tests::is_ignored
```

- [ ] **Step 3: Add ignore_patterns to ArchitectureConfig**

In `src/config/model.rs`, add the field:

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureConfig {
    pub layers: Vec<Layer>,
    pub dependency_rules: Vec<DependencyRule>,
    #[serde(default)]
    pub package_policies: Vec<PackagePolicy>,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}
```

- [ ] **Step 4: Implement is_ignored in rules.rs**

```rust
pub fn is_ignored(project_name: &str, ignore_patterns: &[String]) -> bool {
    ignore_patterns.iter().any(|pat| {
        glob::Pattern::new(pat)
            .map(|p| p.matches(project_name))
            .unwrap_or(false)
    })
}
```

- [ ] **Step 5: Run tests**

```
cargo test rules::tests
```

Expected: PASS.

- [ ] **Step 6: Apply is_ignored in check_dependency_rules**

In `src/commands/check.rs`, add import:

```rust
use crate::rules::{resolve_layer, resolve_layer_by_namespace, is_ignored};
```

In `check_dependency_rules`, at the start of the project loop (before `resolve_layer`):

```rust
for project in projects {
    if is_ignored(&project.name, &config.ignore_patterns) {
        continue;
    }
    // ... existing code unchanged
```

In `check_package_policies`, same guard at the start of the project loop:

```rust
for project in projects {
    if is_ignored(&project.name, &config.ignore_patterns) {
        continue;
    }
    // ... existing code unchanged
```

- [ ] **Step 7: Write test that ignored project produces no violation and no warning**

Add to `src/commands/check.rs` tests (inside the existing `mod tests`):

```rust
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
```

- [ ] **Step 8: Run tests**

```
cargo test commands::check::tests::ignored_project_skipped_entirely
```

Expected: PASS.

- [ ] **Step 9: Update init template**

In `src/commands/init.rs`, add `ignorePatterns` to the `TEMPLATE` const after `packagePolicies`:

```rust
const TEMPLATE: &str = r#"/// Ark Architecture Configuration
/// Generated by `ark init`

amends "pkl/ark.pkl"

layers {
  new {
    name = "Presentation"
    patterns { "*.Api"; "*.Web"; "*.Host" }
  }
  new {
    name = "Application"
    patterns { "*.Application"; "*.UseCases" }
  }
  new {
    name = "Domain"
    patterns { "*.Domain"; "*.Core" }
  }
  new {
    name = "Infrastructure"
    patterns { "*.Infrastructure"; "*.Persistence"; "*.Adapters" }
  }
}

dependencyRules {
  /// Presentation may call Application
  new { from = "Presentation"; to = "Application"; allowed = true }
  /// Application may call Domain
  new { from = "Application"; to = "Domain"; allowed = true }
  /// Infrastructure implements Domain interfaces — allowed
  new { from = "Infrastructure"; to = "Domain"; allowed = true }
  /// Domain must NOT depend on anything else
  new { from = "Domain"; to = "Application"; allowed = false }
  new { from = "Domain"; to = "Presentation"; allowed = false }
  new { from = "Domain"; to = "Infrastructure"; allowed = false }
}

packagePolicies {
  new {
    layer = "Domain"
    /// Domain should stay pure — no EF Core, no HTTP clients
    forbidden { "Microsoft.EntityFrameworkCore"; "System.Net.Http" }
  }
}

ignorePatterns { "*.Tests"; "*.Specs"; "*.IntegrationTests" }
"#;
```

- [ ] **Step 10: Run all tests**

```
cargo test
```

Expected: all pass.

- [ ] **Step 11: Commit**

```
git add src/rules.rs src/config/model.rs src/commands/check.rs src/commands/init.rs
git commit -m "feat: add ignorePatterns to config; skip ignored projects in all checks"
```

---

## Task 3: Span accuracy for .csproj violations

**Files:**
- Modify: `src/parser/csproj.rs`
- Modify: `src/commands/check.rs`
- Modify: `src/graph/mod.rs`

- [ ] **Step 1: Write failing test for span**

Add to `src/parser/csproj.rs` tests:

```rust
#[test]
fn project_ref_span_points_to_include_value() {
    let dir = tempfile::tempdir().unwrap();
    let xml = r#"<Project>
  <ItemGroup>
    <ProjectReference Include="..\MyApp.Domain\MyApp.Domain.csproj" />
  </ItemGroup>
</Project>"#;
    let path = write_csproj(dir.path(), "MyApp.Api", xml);
    let pf = ProjectFile::parse(&path).unwrap();
    let (start, len) = pf.project_refs[0].include_span;
    let src = std::fs::read_to_string(&path).unwrap();
    assert_eq!(&src[start..start + len], r"..\MyApp.Domain\MyApp.Domain.csproj");
}

#[test]
fn package_ref_span_points_to_name_value() {
    let dir = tempfile::tempdir().unwrap();
    let xml = r#"<Project><ItemGroup><PackageReference Include="Newtonsoft.Json" Version="13.0.3" /></ItemGroup></Project>"#;
    let path = write_csproj(dir.path(), "MyApp.Api", xml);
    let pf = ProjectFile::parse(&path).unwrap();
    let (start, len) = pf.package_refs[0].name_span;
    let src = std::fs::read_to_string(&path).unwrap();
    assert_eq!(&src[start..start + len], "Newtonsoft.Json");
}
```

- [ ] **Step 2: Run — expect compile error (fields undefined)**

```
cargo test parser::csproj::tests::project_ref_span
```

- [ ] **Step 3: Add span fields and helper to csproj.rs**

Replace the struct definitions and add a helper function:

```rust
#[derive(Debug, Clone)]
pub struct ProjectRef {
    pub include: String,
    /// Byte span of the Include attribute value within the .csproj file content.
    pub include_span: (usize, usize),
    pub resolved: Option<PathBuf>,
}

impl ProjectRef {
    pub fn new(include: String, resolved: Option<PathBuf>) -> Self {
        ProjectRef { include, include_span: (0, 0), resolved }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PackageRef {
    pub name: String,
    /// Byte span of the Include attribute value within the .csproj file content.
    pub name_span: (usize, usize),
    pub version: String,
}

impl PackageRef {
    pub fn new(name: String, version: String) -> Self {
        PackageRef { name, name_span: (0, 0), version }
    }
}
```

Add the helper:

```rust
/// Find the byte span of an XML attribute value in the raw file content.
/// Searches for `attr="value"` (double or single quotes).
fn find_attr_span(content: &str, attr: &str, value: &str) -> (usize, usize) {
    let double = format!(r#"{}="{}""#, attr, value);
    if let Some(pos) = content.find(&double) {
        return (pos + attr.len() + 2, value.len());
    }
    let single = format!("{}='{}'", attr, value);
    if let Some(pos) = content.find(&single) {
        return (pos + attr.len() + 2, value.len());
    }
    (content.find(value).unwrap_or(0), value.len())
}
```

Update `ProjectFile::parse` to compute spans:

```rust
b"ProjectReference" => {
    if let Some(include) = attr_value(&e, b"Include") {
        let include_span = find_attr_span(&content, "Include", &include);
        let resolved = path
            .parent()
            .map(|p| p.join(&include))
            .map(|p| p.canonicalize().unwrap_or(p));
        project_refs.push(ProjectRef { include, include_span, resolved });
    }
}
b"PackageReference" => {
    if let Some(name) = attr_value(&e, b"Include") {
        let name_span = find_attr_span(&content, "Include", &name);
        let version = attr_value(&e, b"Version").unwrap_or_default();
        package_refs.push(PackageRef { name, name_span, version });
    }
}
```

- [ ] **Step 4: Fix struct literal construction in existing tests**

In `src/parser/csproj.rs` tests, the `project_reference_self_closing_parsed` test uses no struct literals — it only reads fields, so it's fine. The `multiple_project_and_package_refs` test also only reads fields — fine.

In `src/commands/check.rs`, the `make_project` helper constructs `ProjectRef` directly. Update it:

```rust
fn make_project(name: &str, refs: &[&str], packages: &[(&str, &str)]) -> ProjectFile {
    ProjectFile {
        path: std::path::PathBuf::from(format!("{name}.csproj")),
        name: name.to_string(),
        project_refs: refs
            .iter()
            .map(|r| ProjectRef::new(r.to_string(), None))
            .collect(),
        package_refs: packages
            .iter()
            .map(|(n, v)| PackageRef::new(n.to_string(), v.to_string()))
            .collect(),
    }
}
```

In `src/graph/mod.rs`, the test `proj` helper:

```rust
fn proj(name: &str, refs: Vec<&str>) -> ProjectFile {
    ProjectFile {
        path: PathBuf::from(format!("{name}.csproj")),
        name: name.to_string(),
        project_refs: refs
            .into_iter()
            .map(|r| ProjectRef::new(format!("{r}.csproj"), None))
            .collect(),
        package_refs: vec![],
    }
}
```

- [ ] **Step 5: Use accurate spans in check.rs violations**

In `check_dependency_rules`, replace the `src.find` span calculation:

```rust
// Before:
let span_start = src.find(&pref.include).unwrap_or(0);
report.violations.push(Violation {
    ...
    span: (span_start, pref.include.len()).into(),
});

// After:
report.violations.push(Violation {
    ...
    span: pref.include_span.into(),
});
```

In `check_package_policies`, same change:

```rust
// Before:
let span_start = src.find(&pkg.name).unwrap_or(0);
report.violations.push(Violation {
    ...
    span: (span_start, pkg.name.len()).into(),
});

// After:
report.violations.push(Violation {
    ...
    span: pkg.name_span.into(),
});
```

Note: `src` is still read (`std::fs::read_to_string`) for `NamedSource` — keep that line.

- [ ] **Step 6: Run all tests**

```
cargo test
```

Expected: all pass including the two new span tests.

- [ ] **Step 7: Commit**

```
git add src/parser/csproj.rs src/commands/check.rs src/graph/mod.rs
git commit -m "fix: accurate violation spans for .csproj project and package references"
```

---

## Task 4: Span accuracy for C# source violations

**Files:**
- Modify: `src/scanner/cs_scanner.rs`
- Modify: `src/commands/check.rs`

- [ ] **Step 1: Write failing test for UsingDirective byte range**

Add to `src/scanner/cs_scanner.rs` tests:

```rust
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
```

- [ ] **Step 2: Run — expect compile error**

```
cargo test scanner::cs_scanner::tests::using_directive_has_correct_byte_range
```

- [ ] **Step 3: Add UsingDirective and update CsHeader**

In `src/scanner/cs_scanner.rs`, add the struct and update `CsHeader`:

```rust
#[derive(Debug, Clone)]
pub struct UsingDirective {
    pub namespace: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Debug, Clone)]
pub struct CsHeader {
    pub path: PathBuf,
    pub namespace: Option<String>,
    pub usings: Vec<UsingDirective>,
}
```

Replace `extract_using_name` with `extract_using_directive`:

```rust
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
```

Update the match arm in `scan_file`:

```rust
"using_directive" => {
    if let Some(ud) = extract_using_directive(child, &source) {
        usings.push(ud);
    }
}
```

- [ ] **Step 4: Fix existing tests that assert on usings as Vec<String>**

In `src/scanner/cs_scanner.rs` tests, add a helper and update assertions:

```rust
fn ns(usings: &[UsingDirective]) -> Vec<&str> {
    usings.iter().map(|u| u.namespace.as_str()).collect()
}
```

Update each assertion:

```rust
// extracts_namespace_and_usings
assert_eq!(ns(&h.usings), vec!["System", "System.Collections.Generic", "MyApp.Domain.Entities"]);

// file_scoped_namespace
assert_eq!(ns(&h.usings), vec!["MyApp.Domain.Interfaces"]);

// no_namespace_still_collects_usings
assert_eq!(ns(&h.usings), vec!["System"]);

// skips_using_static — was assert!(h.usings.is_empty())
assert!(h.usings.is_empty());

// skips_using_alias — same
assert!(h.usings.is_empty());
```

- [ ] **Step 5: Update check_source_rules to use directive byte offsets**

In `src/commands/check.rs`, `check_source_rules`, update the inner loop:

```rust
for using in &header.usings {
    let Some(to_layer) = resolve_layer_by_namespace(&using.namespace, &config.layers) else {
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
        report.violations.push(Violation {
            message: format!(
                "Source: layer '{}' must not use '{}' from layer '{}'",
                from_layer.name, using.namespace, to_layer.name,
            ),
            src: miette::NamedSource::new(header.path.to_string_lossy(), src),
            span: (using.start_byte, using.end_byte - using.start_byte).into(),
        });
    }
}
```

- [ ] **Step 6: Run all tests**

```
cargo test
```

Expected: all pass including the new byte-range test.

- [ ] **Step 7: Commit**

```
git add src/scanner/cs_scanner.rs src/commands/check.rs
git commit -m "fix: accurate violation spans for C# using directives via tree-sitter byte ranges"
```

---

## Task 5: ark explain command

**Files:**
- Create: `src/commands/explain.rs`
- Modify: `src/commands/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing test for explain logic**

Create `src/commands/explain.rs` with tests only:

```rust
use crate::config::model::{ArchitectureConfig, DependencyRule, Layer, PackagePolicy};
use crate::rules::resolve_layer;

pub async fn run(_root: &str, _config_path: &str, _project: &str) -> miette::Result<()> {
    todo!()
}

pub fn describe_project(
    project: &str,
    config: &ArchitectureConfig,
    known_projects: &[&str],
) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(
        layers: &[(&str, &[&str])],
        rules: &[(&str, &str, bool)],
    ) -> ArchitectureConfig {
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
        let config = cfg(&[("Domain", &["*.Domain"]), ("Application", &["*.Application"])], &[
            ("Application", "Domain", true),
        ]);
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
```

- [ ] **Step 2: Run — expect todo panic**

```
cargo test commands::explain::tests
```

- [ ] **Step 3: Implement describe_project and run**

Replace the file content:

```rust
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
```

- [ ] **Step 4: Register module**

In `src/commands/mod.rs`:

```rust
pub mod baseline;
pub mod check;
pub mod explain;
pub mod graph;
pub mod init;
```

- [ ] **Step 5: Add Explain variant to main.rs**

In `src/main.rs`, add to the `Commands` enum:

```rust
/// Show which layer a project belongs to and what it can depend on
Explain {
    /// Project name to look up (e.g. MyApp.Domain)
    project: String,
},
```

Add to the match:

```rust
Commands::Explain { project } => {
    commands::explain::run(&cli.root, &cli.config, &project).await
}
```

- [ ] **Step 6: Run all tests**

```
cargo test
```

Expected: all pass.

- [ ] **Step 7: Commit**

```
git add src/commands/explain.rs src/commands/mod.rs src/main.rs
git commit -m "feat: add ark explain command"
```

---

## Task 6: Baseline data model

**Files:**
- Create: `src/baseline.rs`
- Modify: `src/report.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing tests for BaselineEntry**

Create `src/baseline.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::path::Path;
use miette::{IntoDiagnostic, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub kind: String,
    pub from: String,
    pub to: String,
}

pub fn try_load(_path: &Path) -> Option<Vec<BaselineEntry>> {
    todo!()
}

pub fn save(_path: &Path, _entries: &[BaselineEntry]) -> Result<()> {
    todo!()
}

// apply_baseline lives in commands/check.rs to avoid a circular dependency
// (report.rs imports BaselineEntry; baseline.rs must not import Violation)

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, from: &str, to: &str) -> BaselineEntry {
        BaselineEntry { kind: kind.to_string(), from: from.to_string(), to: to.to_string() }
    }

    #[test]
    fn round_trips_through_json() {
        let entries = vec![
            entry("project_ref", "MyApp.Domain", "MyApp.Infrastructure"),
            entry("package", "MyApp.Domain", "Microsoft.EntityFrameworkCore"),
        ];
        let json = serde_json::to_string(&entries).unwrap();
        let loaded: Vec<BaselineEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(entries, loaded);
    }

    #[test]
    fn save_and_try_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ark-baseline.json");
        let entries = vec![entry("project_ref", "A", "B")];
        save(&path, &entries).unwrap();
        let loaded = try_load(&path).unwrap();
        assert_eq!(loaded, entries);
    }

    #[test]
    fn try_load_returns_none_for_missing_file() {
        let result = try_load(Path::new("/nonexistent/ark-baseline.json"));
        assert!(result.is_none());
    }

    #[test]
    fn try_load_returns_none_for_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ark-baseline.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(try_load(&path).is_none());
    }
}
```

- [ ] **Step 2: Run — expect todo panic / compile error**

```
cargo test baseline::tests::round_trips_through_json
```

- [ ] **Step 3: Implement try_load and save**

```rust
pub fn try_load(path: &Path) -> Option<Vec<BaselineEntry>> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text)
        .map_err(|e| tracing::warn!("Malformed baseline {:?}: {e}", path))
        .ok()
}

pub fn save(path: &Path, entries: &[BaselineEntry]) -> Result<()> {
    let text = serde_json::to_string_pretty(entries).into_diagnostic()?;
    std::fs::write(path, text).into_diagnostic()?;
    Ok(())
}
```

- [ ] **Step 4: Register module in main.rs**

Add `mod baseline;` to `src/main.rs` (after `mod report;`).

- [ ] **Step 5: Add violation_keys to CheckReport**

In `src/report.rs`, add the import and field:

```rust
use crate::baseline::BaselineEntry;

pub struct CheckReport {
    pub violations: Vec<Violation>,
    pub violation_keys: Vec<BaselineEntry>,
    pub warnings: Vec<String>,
}

impl CheckReport {
    pub fn new() -> Self {
        CheckReport {
            violations: Vec::new(),
            violation_keys: Vec::new(),
            warnings: Vec::new(),
        }
    }
    // ... rest unchanged
}
```

- [ ] **Step 6: Run all tests**

```
cargo test
```

Expected: all pass (violation_keys is unused for now — no compile error because it's a public field on a struct we construct ourselves).

- [ ] **Step 7: Run all tests**

```
cargo test
```

Expected: all pass.

- [ ] **Step 9: Commit**

```
git add src/baseline.rs src/report.rs src/main.rs
git commit -m "feat: add BaselineEntry data model with try_load and save"
```

---

## Task 7: Wire violation_keys into check.rs + refactor collect()

**Files:**
- Modify: `src/commands/check.rs`

- [ ] **Step 1: Add apply_baseline to check.rs with tests**

Add this private function and its tests to `src/commands/check.rs`:

```rust
fn apply_baseline(
    violations: Vec<Violation>,
    violation_keys: Vec<crate::baseline::BaselineEntry>,
    baseline: &[crate::baseline::BaselineEntry],
    warnings: &mut Vec<String>,
) -> (Vec<Violation>, Vec<crate::baseline::BaselineEntry>) {
    for entry in baseline {
        if !violation_keys.contains(entry) {
            warnings.push(format!(
                "Stale baseline entry ({} {} → {}) — violation no longer exists",
                entry.kind, entry.from, entry.to
            ));
        }
    }
    violations
        .into_iter()
        .zip(violation_keys.into_iter())
        .filter(|(_, key)| !baseline.contains(key))
        .unzip()
}

#[cfg(test)]
mod baseline_tests {
    use super::*;
    use crate::baseline::BaselineEntry;

    fn entry(kind: &str, from: &str, to: &str) -> BaselineEntry {
        BaselineEntry { kind: kind.to_string(), from: from.to_string(), to: to.to_string() }
    }

    fn make_violation() -> Violation {
        Violation {
            message: "test".to_string(),
            src: miette::NamedSource::new("test.csproj", "content".to_string()),
            span: (0, 1).into(),
        }
    }

    #[test]
    fn filters_matching_violations() {
        let baseline = vec![entry("project_ref", "A", "B")];
        let violations = vec![make_violation(), make_violation()];
        let keys = vec![
            entry("project_ref", "A", "B"),
            entry("project_ref", "C", "D"),
        ];
        let mut warnings = vec![];
        let (remaining, _) = apply_baseline(violations, keys, &baseline, &mut warnings);
        assert_eq!(remaining.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn warns_on_stale_entry() {
        let baseline = vec![entry("project_ref", "A", "B")];
        let mut warnings = vec![];
        let (remaining, _) = apply_baseline(vec![], vec![], &baseline, &mut warnings);
        assert!(remaining.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Stale"));
    }
}
```

Run:
```
cargo test commands::check::baseline_tests
```

Expected: PASS.

- [ ] **Step 2: Add violation_keys push to check_dependency_rules**

In `check_dependency_rules`, after `report.violations.push(Violation { ... })`, add:

```rust
report.violation_keys.push(crate::baseline::BaselineEntry {
    kind: "project_ref".to_string(),
    from: project.name.clone(),
    to: dep_name.clone(),
});
```

In `check_package_policies`, after pushing the violation:

```rust
report.violation_keys.push(crate::baseline::BaselineEntry {
    kind: "package".to_string(),
    from: project.name.clone(),
    to: pkg.name.clone(),
});
```

In `check_source_rules`, after pushing the violation:

```rust
report.violation_keys.push(crate::baseline::BaselineEntry {
    kind: "source".to_string(),
    from: ns.to_string(),
    to: using.namespace.clone(),
});
```

- [ ] **Step 2: Extract collect() from run()**

Refactor `src/commands/check.rs` so `run` calls a new `collect`:

```rust
pub async fn collect(root: &Path, config: &ArchitectureConfig) -> Result<CheckReport> {
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
    check_dependency_rules(&projects, config, &mut report);
    check_package_policies(&projects, config, &mut report);
    check_source_rules(root, config, &mut report)?;
    Ok(report)
}

pub async fn run(root: &str, config_path: &str, strict: bool, no_baseline: bool) -> Result<()> {
    let root_path = Path::new(root);
    let config = load_config(Path::new(config_path)).await?;
    let mut report = collect(root_path, &config).await?;

    if !no_baseline {
        let baseline_path = root_path.join("ark-baseline.json");
        if let Some(baseline) = crate::baseline::try_load(&baseline_path) {
            let (filtered_violations, filtered_keys) =
                crate::baseline::apply_baseline(
                    report.violations,
                    report.violation_keys,
                    &baseline,
                    &mut report.warnings,
                );
            report.violations = filtered_violations;
            report.violation_keys = filtered_keys;
        }
    }

    report.print_summary();

    if !report.violations.is_empty() || (strict && !report.warnings.is_empty()) {
        for v in report.violations {
            eprintln!("{:?}", miette::Report::new_boxed(Box::new(v)));
        }
        std::process::exit(1);
    }

    Ok(())
}
```

- [ ] **Step 3: Update Check variant in main.rs to add no_baseline flag**

```rust
Check {
    #[arg(long)]
    strict: bool,
    /// Ignore ark-baseline.json even if present
    #[arg(long)]
    no_baseline: bool,
},
```

Update the match arm:

```rust
Commands::Check { strict, no_baseline } => {
    commands::check::run(&cli.root, &cli.config, strict, no_baseline).await
}
```

- [ ] **Step 4: Run all tests**

```
cargo test
```

Expected: all pass.

- [ ] **Step 5: Commit**

```
git add src/commands/check.rs src/main.rs
git commit -m "feat: wire violation_keys into check; extract collect(); integrate baseline filtering"
```

---

## Task 8: ark baseline command

**Files:**
- Create: `src/commands/baseline.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Implement baseline command**

Create `src/commands/baseline.rs`:

```rust
use std::path::Path;
use miette::Result;

use crate::baseline;
use crate::config::load_config;
use super::check::collect;

pub async fn run(root: &str, config_path: &str) -> Result<()> {
    let root_path = Path::new(root);
    let config = load_config(Path::new(config_path)).await?;
    let report = collect(root_path, &config).await?;

    let baseline_path = root_path.join("ark-baseline.json");
    baseline::save(&baseline_path, &report.violation_keys)?;

    println!(
        "Wrote {} suppressed violation(s) to {:?}",
        report.violation_keys.len(),
        baseline_path
    );
    Ok(())
}
```

- [ ] **Step 2: Add Baseline variant to main.rs**

Add to `Commands` enum:

```rust
/// Snapshot current violations into ark-baseline.json for suppression
Baseline,
```

Add to the match:

```rust
Commands::Baseline => {
    commands::baseline::run(&cli.root, &cli.config).await
}
```

- [ ] **Step 3: Run all tests**

```
cargo test
```

Expected: all pass.

- [ ] **Step 4: Commit**

```
git add src/commands/baseline.rs src/main.rs
git commit -m "feat: add ark baseline command"
```

---

## Done

At this point all four features are implemented:

- `ark explain MyApp.Domain` — shows layer, rules, siblings
- `ark baseline` — snapshots current violations to `ark-baseline.json`
- `ark check` — auto-reads baseline, filters suppressed violations, warns on stale entries
- `ark check --no-baseline` — ignores baseline
- Violation spans in `.csproj` files point precisely to the `Include` attribute value
- Violation spans in `.cs` files use tree-sitter byte ranges
- `ignorePatterns` in config excludes projects from all checks and warnings
- `ark init` generates a template with `ignorePatterns` pre-populated
