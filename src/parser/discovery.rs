use std::path::{Path, PathBuf};
use ignore::WalkBuilder;
use miette::{IntoDiagnostic, Result};

/// Recursively find all .csproj files under `root`, respecting .gitignore.
pub fn discover_projects(root: &Path) -> Result<Vec<PathBuf>> {
    let mut projects = Vec::new();

    for entry in WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .require_git(false)
        .build()
    {
        let entry = entry.into_diagnostic()?;
        let path = entry.path();
        if path
            .extension()
            .map_or(false, |e| e.eq_ignore_ascii_case("csproj"))
        {
            projects.push(path.to_path_buf());
        }
    }

    Ok(projects)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_csproj_files_at_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Foo.csproj"), "").unwrap();
        fs::write(dir.path().join("Bar.csproj"), "").unwrap();
        fs::write(dir.path().join("README.md"), "").unwrap();

        let found = discover_projects(dir.path()).unwrap();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|p| p.extension().unwrap() == "csproj"));
    }

    #[test]
    fn finds_nested_csproj() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("src").join("MyApp.Api");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("MyApp.Api.csproj"), "").unwrap();

        let found = discover_projects(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn ignores_non_csproj_extensions() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("solution.sln"), "").unwrap();
        fs::write(dir.path().join("app.vbproj"), "").unwrap();
        fs::write(dir.path().join("notes.txt"), "").unwrap();

        let found = discover_projects(dir.path()).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Big.CSPROJ"), "").unwrap();

        let found = discover_projects(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn respects_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "bin/\n").unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("Hidden.csproj"), "").unwrap();
        fs::write(dir.path().join("Visible.csproj"), "").unwrap();

        let found = discover_projects(dir.path()).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file_name().unwrap(), "Visible.csproj");
    }
}
