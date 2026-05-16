# ark

[![Crates.io](https://img.shields.io/crates/v/ark-cli.svg)](https://crates.io/crates/ark-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Architectural boundary enforcer for .NET solutions. Parses `.csproj` project graphs and C# source files to catch layer violations before they reach CI.

```
$ ark check

  × Layer 'Domain' (MyApp.Domain) must not depend on layer 'Infrastructure' (MyApp.Infrastructure.Data)
   ╭─[MyApp.Domain/MyApp.Domain.csproj:6:27]
 6 │     <ProjectReference Include="..\MyApp.Infrastructure.Data\MyApp.Infrastructure.Data.csproj" />
   ·                           ──────────────────────────────────────────────────────────────────
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
# 1. Scaffold architecture.toml interactively
cd /path/to/your/solution
ark init

# 2. Check for violations
ark check
```

---

## Installation

**Pre-built binaries** (Linux, Windows x86/arm) are available on the [releases page](https://github.com/TheEskhaton/ark/releases).

Or install via Cargo:

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
layers = [
  { name = "Presentation",   patterns = ["*.Api", "*.Web", "*.Host"]                   },
  { name = "Application",    patterns = ["*.Application", "*.UseCases"]                },
  { name = "Domain",         patterns = ["*.Domain", "*.Core"]                         },
  { name = "Infrastructure", patterns = ["*.Infrastructure", "*.Persistence"]          },
]

# Any dependency not listed here is forbidden by default.
dependency_rules = [
  { from = "Presentation",   to = "Application",   allowed = true  },
  { from = "Application",    to = "Domain",         allowed = true  },
  { from = "Infrastructure", to = "Domain",         allowed = true  },
  { from = "Domain",         to = "Infrastructure", allowed = false },
]

package_policies = [
  { layer = "Domain", forbidden = ["Microsoft.EntityFrameworkCore", "Microsoft.AspNetCore"] },
]

ignore_patterns = ["*.Tests", "*.Specs", "*.IntegrationTests"]
```

**Layer patterns** use glob syntax — `*.Domain` matches any project name ending in `.Domain`. Any dependency not listed in `dependency_rules` is **forbidden by default**.

### C# source scanning

Add `namespace_patterns` to a layer to also check `using` directives in `.cs` files for that layer (powered by tree-sitter):

```toml
layers = [
  { name = "Domain", patterns = ["*.Domain"], namespace_patterns = ["MyApp.Domain.*"] },
]
```

Omit `namespace_patterns` to skip source scanning for that layer.

---

## Commands

### `ark check`

Run all architectural checks and report violations.

```bash
ark check                  # exit 1 if any violations
ark check --strict         # also exit 1 on warnings (unmatched projects)
ark check --no-baseline    # ignore ark-baseline.json even if present
```

### `ark baseline`

Brownfield adoption: snapshot existing violations so `ark check` only fails on *new* ones. Teams can quarantine known debt and clean it up incrementally.

```bash
ark baseline               # write current violations → ark-baseline.json
ark check                  # now only fails on violations introduced since the snapshot
```

Commit `ark-baseline.json` alongside your config. Stale entries (violations that no longer exist) surface as warnings so you know when debt has been paid off.

### `ark explain`

Show which layer a project belongs to and what it can depend on.

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

Useful when a project shows up as unmatched or when onboarding to an unfamiliar solution.

### `ark graph`

Export the project dependency graph.

```bash
ark graph                               # Mermaid to stdout
ark graph --format dot -o graph.dot     # Graphviz DOT file
```

### `ark init`

Interactively scaffold `architecture.toml` from your real solution structure.

```bash
ark init
```

The wizard:
1. Scans all `.csproj` files and builds a dependency graph
2. Groups projects into architectural tiers by topological depth
3. Walks you through naming each tier as a layer
4. Reviews each detected inter-layer dependency — allow or forbid
5. Writes `architecture.toml` tailored to your solution

For brownfield teams with existing violations, run `ark baseline` right after to snapshot the current state.

---

## CI integration

### GitHub Actions

```yaml
- name: Check architecture
  run: |
    curl -sSL https://github.com/TheEskhaton/ark/releases/latest/download/ark-latest-x86_64-unknown-linux-gnu.tar.gz | tar xz
    ./ark check
```

Or via Cargo (slower, but no binary download needed):

```yaml
- name: Check architecture
  run: |
    cargo install ark-cli --quiet
    ark check
```

ark exits `0` on clean, `1` on violations.

---

## Global flags

```
ark --root <path>     # solution root (default: current directory)
ark --config <path>   # config file (default: architecture.toml)
```

Useful when running ark from a directory other than the solution root:

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
