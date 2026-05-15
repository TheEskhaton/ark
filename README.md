# ark

[![Crates.io](https://img.shields.io/crates/v/ark-cli.svg)](https://crates.io/crates/ark-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Architectural boundary enforcer for .NET solutions. Parses `.csproj` project graphs and C# source files to catch layer violations before they reach CI.

```
$ ark check

  × Domain → Infrastructure dependency forbidden
   ╭─[MyApp.Domain/Repositories/UserRepo.cs:3:7]
 3 │ using MyApp.Infrastructure.Data;
   ·       ────────────────────────
   ╰─
```

---

## What it checks

| Check | What it catches |
|---|---|
| **Project references** | `.csproj` `<ProjectReference>` that cross forbidden layer boundaries |
| **Package policies** | `<PackageReference>` packages banned from specific layers (e.g. EF Core in Domain) |
| **Source imports** | `using` directives in `.cs` files referencing a forbidden layer's namespace (tree-sitter) |

Violations are reported with source spans pointing directly to the offending line.

---

## Quick start

```bash
# 1. Generate a starter config in your solution root
ark init

# 2. Edit architecture.pkl to match your layers, then:
ark check
```

---

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

---

## Configuration

ark reads `architecture.toml` from the solution root. Run `ark init` to generate a starter file, or write one by hand:

```toml
[[layers]]
name = "Domain"
patterns = ["*.Domain", "*.Core"]
# Optional: enables C# using-directive checks for this layer
namespace_patterns = ["MyApp.Domain.*"]

[[layers]]
name = "Application"
patterns = ["*.Application", "*.UseCases"]

[[layers]]
name = "Infrastructure"
patterns = ["*.Infrastructure"]

[[layers]]
name = "Presentation"
patterns = ["*.Api", "*.Web", "*.Host"]

[[dependency_rules]]
from = "Presentation"
to = "Application"
allowed = true

[[dependency_rules]]
from = "Application"
to = "Domain"
allowed = true

[[dependency_rules]]
from = "Infrastructure"
to = "Domain"
allowed = true

[[dependency_rules]]
from = "Domain"
to = "Infrastructure"
allowed = false

[[package_policies]]
layer = "Domain"
forbidden = ["Microsoft.EntityFrameworkCore", "Microsoft.AspNetCore"]

ignore_patterns = ["*.Tests", "*.Specs", "*.IntegrationTests"]
```

Layer patterns use glob syntax (`*` matches any sequence of non-separator characters). Any dependency not listed in `dependency_rules` is **forbidden by default**.

`namespace_patterns` activates the C# source scan for a layer — omit it to skip tree-sitter parsing for that layer.


---

## Commands

### `ark check`

Run all architectural checks and report violations.

```bash
ark check                  # check; exit 1 if any violations
ark check --strict         # also exit 1 on warnings (unmatched projects)
ark check --no-baseline    # ignore ark-baseline.json even if present
```

### `ark baseline`

Snapshot current violations so `ark check` only fails on *new* ones. Useful for gradual adoption in brownfield solutions — lock in existing debt without silencing the tool.

```bash
ark baseline               # write all current violations → ark-baseline.json
ark check                  # now reports only violations introduced since the snapshot
```

`ark-baseline.json` should be committed alongside your config. Stale entries (violations that no longer exist) are reported as warnings so you can clean them up over time.

### `ark explain`

Show which layer a project belongs to and what it can and cannot depend on.

```bash
ark explain MyApp.Domain
```

```
Project: MyApp.Domain
Layer:   Domain

Dependency rules:
  → Application              forbidden  [default]
  → Infrastructure           forbidden  [explicit]
  → Presentation             forbidden  [explicit]

Other projects in this layer:
  MyApp.Core
```

Useful when a project shows up as unmatched, or when onboarding to an unfamiliar solution.

### `ark graph`

Export the project dependency graph.

```bash
ark graph                               # Mermaid to stdout
ark graph --format dot -o graph.dot     # Graphviz DOT file
```

### `ark init`

Generate a starter `architecture.pkl` in the solution root.

```bash
cd /path/to/solution
ark init
```

---

## CI integration

### GitHub Actions

```yaml
- name: Check architecture
  run: |
    cargo install ark-cli --quiet
    ark check
```

ark exits `0` on clean, `1` on violations — no external dependencies required.

---

## Global flags

```
ark --root <path>     # solution root (default: current directory)
ark --config <path>   # config file (default: architecture.pkl)
```

These apply to all subcommands and are useful when running ark from a different directory:

```bash
ark --root /path/to/solution check
```

---

## Exit codes

| Code | Meaning |
|---|---|
| `0` | No violations |
| `1` | One or more violations found |

Use `--strict` to also exit `1` on warnings (e.g. projects that match no layer).

---

## Performance

ark uses `rayon` for parallel project and file parsing.

| Operation | Target | Typical (ABP Framework, ~500 projects) |
|---|---|---|
| Project graph scan | < 50 ms | ~30 ms |
| Full source scan (tree-sitter) | < 500 ms | ~200 ms |

---

## License

MIT
