use std::path::{Path, PathBuf};
use quick_xml::events::Event;
use quick_xml::Reader;
use miette::{IntoDiagnostic, Result, miette};

#[derive(Debug, Clone)]
pub struct ProjectFile {
    pub path: PathBuf,
    pub name: String,
    pub project_refs: Vec<ProjectRef>,
    pub package_refs: Vec<PackageRef>,
}

#[derive(Debug, Clone)]
pub struct ProjectRef {
    pub include: String,
    /// Resolved absolute path (best-effort)
    pub resolved: Option<PathBuf>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PackageRef {
    pub name: String,
    pub version: String,
}

impl ProjectFile {
    pub fn parse(path: &Path) -> Result<Self> {
        let name = path
            .file_stem()
            .ok_or_else(|| miette!("Cannot determine project name from {:?}", path))?
            .to_string_lossy()
            .into_owned();

        let content = std::fs::read_to_string(path)
            .into_diagnostic()?;

        let mut project_refs = Vec::new();
        let mut package_refs = Vec::new();

        let mut reader = Reader::from_str(&content);
        reader.config_mut().trim_text(true);

        loop {
            match reader.read_event().into_diagnostic()? {
                Event::Empty(e) | Event::Start(e) => {
                    match e.name().as_ref() {
                        b"ProjectReference" => {
                            if let Some(include) = attr_value(&e, b"Include") {
                                let resolved = path
                                    .parent()
                                    .map(|p| p.join(&include))
                                    .map(|p| p.canonicalize().unwrap_or(p));
                                project_refs.push(ProjectRef { include, resolved });
                            }
                        }
                        b"PackageReference" => {
                            if let Some(name) = attr_value(&e, b"Include") {
                                let version = attr_value(&e, b"Version").unwrap_or_default();
                                package_refs.push(PackageRef { name, version });
                            }
                        }
                        _ => {}
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }

        Ok(ProjectFile {
            path: path.to_path_buf(),
            name,
            project_refs,
            package_refs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_csproj(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(format!("{name}.csproj"));
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn name_extracted_from_file_stem() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_csproj(dir.path(), "My.Cool.Project", "<Project/>");
        let pf = ProjectFile::parse(&path).unwrap();
        assert_eq!(pf.name, "My.Cool.Project");
    }

    #[test]
    fn empty_csproj_has_no_refs() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_csproj(
            dir.path(),
            "Empty",
            r#"<Project Sdk="Microsoft.NET.Sdk"></Project>"#,
        );
        let pf = ProjectFile::parse(&path).unwrap();
        assert!(pf.project_refs.is_empty());
        assert!(pf.package_refs.is_empty());
    }

    #[test]
    fn project_reference_self_closing_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let xml = r#"<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <ProjectReference Include="..\MyApp.Domain\MyApp.Domain.csproj" />
  </ItemGroup>
</Project>"#;
        let path = write_csproj(dir.path(), "MyApp.Api", xml);
        let pf = ProjectFile::parse(&path).unwrap();
        assert_eq!(pf.project_refs.len(), 1);
        assert_eq!(
            pf.project_refs[0].include,
            r"..\MyApp.Domain\MyApp.Domain.csproj"
        );
    }

    #[test]
    fn package_references_parsed_with_version() {
        let dir = tempfile::tempdir().unwrap();
        let xml = r#"<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.3" />
    <PackageReference Include="Serilog" Version="3.0.1" />
  </ItemGroup>
</Project>"#;
        let path = write_csproj(dir.path(), "MyApp.Infrastructure", xml);
        let pf = ProjectFile::parse(&path).unwrap();
        assert_eq!(pf.package_refs.len(), 2);
        assert_eq!(pf.package_refs[0].name, "Newtonsoft.Json");
        assert_eq!(pf.package_refs[0].version, "13.0.3");
        assert_eq!(pf.package_refs[1].name, "Serilog");
        assert_eq!(pf.package_refs[1].version, "3.0.1");
    }

    #[test]
    fn package_reference_missing_version_defaults_empty() {
        let dir = tempfile::tempdir().unwrap();
        let xml = r#"<Project><ItemGroup><PackageReference Include="SomePkg" /></ItemGroup></Project>"#;
        let path = write_csproj(dir.path(), "NoVersion", xml);
        let pf = ProjectFile::parse(&path).unwrap();
        assert_eq!(pf.package_refs[0].version, "");
    }

    #[test]
    fn multiple_project_and_package_refs() {
        let dir = tempfile::tempdir().unwrap();
        let xml = r#"<Project>
  <ItemGroup>
    <ProjectReference Include="..\A\A.csproj" />
    <ProjectReference Include="..\B\B.csproj" />
    <PackageReference Include="FluentValidation" Version="11.0.0" />
  </ItemGroup>
</Project>"#;
        let path = write_csproj(dir.path(), "Multi", xml);
        let pf = ProjectFile::parse(&path).unwrap();
        assert_eq!(pf.project_refs.len(), 2);
        assert_eq!(pf.package_refs.len(), 1);
    }

    #[test]
    fn missing_file_returns_error() {
        let result = ProjectFile::parse(Path::new("/nonexistent/path/Fake.csproj"));
        assert!(result.is_err());
    }
}

fn attr_value(element: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    element
        .attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| std::str::from_utf8(&a.value).ok().map(str::to_owned))
}
