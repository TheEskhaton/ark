# ark init — Intelligent Architecture Scaffold

**Date:** 2026-05-16  
**Status:** Approved

---

## Overview

Replace the current static `ark init` template with a smart, interactive wizard that scans the real codebase, detects architectural boundaries from the dependency graph, and guides the user through confirming layers and rules — producing a grounded `architecture.toml` rather than a generic boilerplate.

Philosophy: **prescriptive**. The generated config reflects the intended architecture (clean layered rules), not a permissive snapshot of the current mess. Violations are expected on day one for legacy codebases; `ark baseline` handles that.

---

## What Changes

The existing `ark init` command is **replaced entirely**. The static template is removed. No new subcommand is added — `init` becomes the scaffold.

No `--non-interactive` flag. Running `init` in CI makes no sense; the command is inherently a human setup step.

---

## Wizard Flow

The wizard narrates every phase clearly so the user always knows what step they're on and why.

```
ark init — intelligent architecture scaffold
───────────────────────────────────────────
Scanning your solution to detect architectural layers.

Step 1/4  Scanning projects...
Step 2/4  Ranking by dependency graph...
Step 3/4  Confirming layers with you...
Step 4/4  Reviewing dependency rules...
```

Each phase header is printed before prompts appear. No silent transitions.

---

## Phase 1 — Scan

1. Discover all `.csproj` files under root using existing `discovery.rs`.
2. Parse each with `csproj.rs` to extract `ProjectReference` and `PackageReference` entries.
3. **Auto-filter test projects** — names matching `*.Tests`, `*.Specs`, `*.IntegrationTests`, `*.UnitTests` are set aside for `ignore_patterns`. They are not included in layer analysis.
4. Build a directed dependency graph from the remaining projects.
5. **Detect cycles** — find strongly connected components with more than one node. Warn the user and collapse cycling projects into the same tier (they cannot be ranked against each other).
6. **Assign tiers** — leaf nodes (no outgoing edges to solution projects) = tier 0. All other nodes: tier = max(tier of dependencies) + 1.
7. Within each tier, scan name suffixes to **suggest** a layer name as a hint only:
   - `.Domain`, `.Core`, `.Entities` → "Domain"
   - `.Application`, `.UseCases`, `.Services` → "Application"
   - `.Infrastructure`, `.Persistence`, `.Adapters` → "Infrastructure"
   - `.Api`, `.Web`, `.Host` → "Presentation"
   - No match → "Layer{N}"

---

## Phase 2 — Layer Wizard

Projects are presented **bottom-to-top** (tier 0 first). For each tier:

```
─── Step 3/4: Confirming layers ───────────────────────────────────
These projects have no dependencies on other solution projects.
They are likely your innermost layer (Domain, Core, etc.)

  MyApp.Domain
  MyApp.Core
  MyApp.Entities

Suggested name: Domain
Layer name [Domain]: _

Move any projects to a different layer? [y/N]: _
```

- If the user moves a project, they are prompted which (already-named) layer to assign it to.
- After all tiers are processed, any unassigned projects get a final pass.

**Isolated projects** (no refs in or out) are handled separately:

```
These projects have no project references at all:
  MyApp.BuildTools

Assign to a layer or ignore? [ignore]: _
```

**Cycle warning** (shown before the wizard if cycles were detected):

```
⚠  Circular dependencies detected:
   MyApp.Domain ↔ MyApp.Application (2 references)

   These projects cannot be ranked by topology and will be grouped together.
   Consider resolving these cycles — they indicate layer boundary violations.
```

---

## Phase 3 — Rules Wizard

After all layers are confirmed, every detected **inter-layer edge** is presented one at a time, with reference counts and topology anomaly hints:

```
─── Step 4/4: Reviewing dependency rules ──────────────────────────
For each dependency between layers, choose whether to allow or forbid it.
Rules not listed here are forbidden by default.

  Presentation → Application   (12 references)   Allow? [Y/n]: _
  Presentation → Domain        (1 reference)    ← unusual   Allow? [y/N]: _
  Application  → Domain        (8 references)   Allow? [Y/n]: _
  Infrastructure → Domain      (5 references)   Allow? [Y/n]: _
  Infrastructure → Application (2 references)   Allow? [Y/n]: _
```

- **Reference count** is shown so the user understands how load-bearing each edge is.
- **"← unusual"** is flagged when an edge skips layers or runs against topological order (bottom-layer depending on top-layer, or layer skipping intermediate tiers). Defaults to `[y/N]` (deny) rather than `[Y/n]`.
- Inter-layer pairs with **zero** references are not shown — they default to forbidden silently.

---

## Phase 4 — Finish

### Test/ignore patterns

```
─── Finishing up ──────────────────────────────────────────────────

Detected test/spec projects (suggested for ignore_patterns):
  MyApp.Tests
  MyApp.Domain.Tests
  MyApp.Integration.Specs

Add these to ignore_patterns? [Y/n]: _
```

### Package policies

```
Add package policies? (e.g. forbid EF Core in Domain layer) [y/N]: _
```

If yes, a simple loop: "Which layer? Which package?" — repeats until the user is done.

### Preview and write

```
─── Preview ───────────────────────────────────────────────────────
layers = [
  { name = "Domain", patterns = ["*.Domain", "*.Core", "*.Entities"] },
  ...
]
...

Write architecture.toml? [Y/n]: _

✓ Created architecture.toml
  Run `ark check` to see your current violations.
  Run `ark baseline` to snapshot them if you want a clean starting point.
```

---

## Architecture

### New module layout

```
src/commands/init/
  mod.rs          — orchestrates wizard phases, top-level run() entry point
  scan.rs         — tier computation (topology ranking, cycle detection, test filtering)
  wizard.rs       — all interactive prompts (wraps dialoguer)
  generator.rs    — builds ArchitectureConfig from wizard answers, serializes to TOML
```

The existing `src/commands/init.rs` is deleted and replaced by this module.

### Reused modules

- `src/parser/discovery.rs` — project discovery (unchanged)
- `src/parser/csproj.rs` — project file parsing (unchanged)
- `src/graph/mod.rs` — dependency graph construction (unchanged or lightly extended)

### New dependency

`dialoguer` crate — provides `Input`, `Confirm`, `Select`, and `MultiSelect` prompt types for the interactive wizard.

### Tier computation

```
fn assign_tiers(projects: &[ProjectFile]) -> HashMap<String, usize>
```

- BFS/DFS from leaves upward
- Cycles collapsed via SCC detection (petgraph already provides this)
- Returns project name → tier number

### TOML generation

`generator.rs` takes the confirmed `Vec<(layer_name, Vec<project_name>)>` and `Vec<(from, to, allowed)>` and produces the final `ArchitectureConfig`, then serializes via `toml::to_string_pretty`.

---

## Out of Scope

- `namespace_patterns` — not generated by scaffold (requires knowing the codebase's namespace prefix; user adds manually after init)
- Multi-solution support (multiple `.sln` files) — single root assumption retained
- Editing an existing `architecture.toml` — init still refuses if the file already exists
