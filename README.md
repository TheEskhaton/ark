# ark

Architectural boundary enforcer for .NET solutions. Parses `.csproj` project graphs and C# source files to catch layer violations before they reach CI.

```
ark check          # report all architectural violations
ark graph          # print dependency graph (Mermaid)
ark graph --format dot -o graph.dot   # export DOT file
ark init           # generate starter architecture.pkl
```

## How it works

ark reads an `architecture.pkl` (or `architecture.json`) config that defines your layers and the rules between them, then checks three things:

| Check | What it catches |
|---|---|
| **Project references** | `.csproj` `<ProjectReference>` that cross forbidden layer boundaries |
| **Package policies** | `<PackageReference>` packages banned from specific layers (e.g. EF Core in Domain) |
| **Source imports** | `using` directives in `.cs` files that reference a forbidden layer's namespace (tree-sitter) |

Violations are reported with miette source spans pointing directly to the offending line.

## Configuration

### Pkl (recommended)

```pkl
amends "pkl/ark.pkl"

layers {
  new { name = "Domain";         patterns { "*.Domain"; "*.Core" }         }
  new { name = "Application";    patterns { "*.Application"; "*.UseCases" } }
  new { name = "Infrastructure"; patterns { "*.Infrastructure" }            }
  new { name = "Presentation";   patterns { "*.Api"; "*.Web"; "*.Host" }    }
}

dependencyRules {
  new { from = "Presentation";   to = "Application";   allowed = true  }
  new { from = "Application";    to = "Domain";         allowed = true  }
  new { from = "Infrastructure"; to = "Domain";         allowed = true  }
  new { from = "Domain";         to = "Application";    allowed = false }
  new { from = "Domain";         to = "Infrastructure"; allowed = false }
  new { from = "Presentation";   to = "Infrastructure"; allowed = false }
}

packagePolicies {
  new {
    layer = "Domain"
    forbidden { "Microsoft.EntityFrameworkCore"; "Microsoft.AspNetCore" }
  }
}
```

Run `ark init` to generate a starter file.

### JSON sidecar (no Pkl CLI required)

Name the file `architecture.json` alongside `architecture.pkl`. ark falls back to it automatically if the Pkl CLI is not installed.

```json
{
  "layers": [
    {
      "name": "Domain",
      "patterns": ["*.Domain", "*.Core"],
      "namespacePatterns": ["MyApp.Domain.*", "MyApp.Domain"]
    }
  ],
  "dependencyRules": [
    { "from": "Domain", "to": "Application", "allowed": false }
  ],
  "packagePolicies": [
    { "layer": "Domain", "forbidden": ["Microsoft.EntityFrameworkCore"] }
  ]
}
```

`namespacePatterns` activates the source-level `using` scan. Omit it to skip tree-sitter parsing.

## Installation

```bash
cargo install ark-cli
```

Or build from source:

```bash
git clone https://github.com/TheEskhaton/ark
cd ark
cargo build --release
# binary: target/release/ark
```

## Performance targets

| Operation | Target | Typical (ABP Framework ~500 projects) |
|---|---|---|
| Project graph scan | < 50 ms | ~30 ms |
| Full source scan (tree-sitter) | < 500 ms | ~200 ms |

ark uses `rayon` for parallel project and file parsing.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | No violations |
| `1` | One or more violations found |

Use `--strict` to also exit `1` on warnings (unmatched projects).

## License

MIT
