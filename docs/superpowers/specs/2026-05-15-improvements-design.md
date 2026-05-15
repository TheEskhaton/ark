# ark improvements — design spec

Date: 2026-05-15

## Scope

Four independent improvements to the `ark` CLI, all oriented toward the local development loop:

- **C** — `ark explain <project>` command
- **D** — Baseline / suppress workflow
- **E** — Span accuracy fix
- **F** — `ignorePatterns` project exclusions

---

## C — `ark explain <project>`

### Purpose

Answers "which layer does this project belong to, and what can it depend on?" — useful when a project is flagged as unmatched or when onboarding to an unfamiliar solution.

### CLI

```
ark explain MyApp.Domain
```

Uses the same global `--root` and `--config` flags as `check`.

### Behaviour

1. Load config and discover all `.csproj` files under root.
2. Match the given project name against layer patterns (same `resolve_layer` logic as `check`).
3. Print:
   - Layer the project belongs to (or "unmatched — no layer pattern matches")
   - Allowed outbound dependencies (layers this project may reference)
   - Forbidden outbound dependencies (layers explicitly or implicitly forbidden)
   - Other known projects in the same layer

### Implementation

- New file: `src/commands/explain.rs`
- New variant `Commands::Explain { project: String }` in `main.rs`
- Reuses `resolve_layer` from `commands/check.rs` — extract to `src/rules.rs` or make it `pub` so both commands can share it

### Error handling

- If the project name matches no discovered `.csproj`, print a clear message and exit 1.
- If config fails to load, propagate the existing miette error.

---

## D — Baseline / suppress workflow

### Purpose

Record existing violations so `ark check` only fails on *new* ones. Enables gradual adoption in brownfield solutions and lets teams quarantine known debt without silencing the tool entirely.

### CLI

```
ark baseline            # snapshot current violations → ark-baseline.json
ark check               # auto-reads ark-baseline.json if present
ark check --no-baseline # ignore baseline even if file exists
```

### Baseline file format

Stored as `ark-baseline.json` in the solution root (next to `architecture.pkl`).

Each entry is keyed by a stable 3-tuple that survives refactors and line-number changes:

```json
[
  { "kind": "project_ref", "from": "MyApp.Domain",    "to": "MyApp.Infrastructure" },
  { "kind": "package",     "from": "MyApp.Domain",    "to": "Microsoft.EntityFrameworkCore" },
  { "kind": "source",      "from": "MyApp.Domain",    "to": "MyApp.Infrastructure.Db" }
]
```

`kind` values: `project_ref` | `package` | `source`

### Behaviour

**`ark baseline`**
- Runs the full check internally (no output, no exit-code side effect).
- Serializes all violations to `ark-baseline.json` in the root directory.
- Prints the count of suppressed entries written.
- Overwrites any existing baseline.

**`ark check` (with baseline present)**
- Loads baseline; after collecting violations, filters out any that match a baseline entry.
- Reports remaining violations normally (exit 1 if any).
- Prints a warning for each baseline entry that no longer has a matching violation ("stale suppression — violation may have been fixed").
- `--no-baseline` skips loading entirely.

### Implementation

- New file: `src/baseline.rs` — `BaselineEntry` struct (serde Serialize/Deserialize), `load`, `save`, `filter_violations` functions.
- `Violation` gets a new method `to_baseline_entry() -> BaselineEntry` returning its stable key.
- `commands/check.rs` calls `baseline::load` after config load; calls `baseline::filter_violations` before printing.
- New `Commands::Baseline` variant in `main.rs`; new `src/commands/baseline.rs`.

### Error handling

- If `ark-baseline.json` is malformed, treat as missing and warn (don't hard-fail `ark check`).
- Stale entries are warnings only — never cause a non-zero exit.

---

## E — Span accuracy fix

### Purpose

Violation source spans currently use `src.find(&substring)` which points to the first occurrence of a string in the file. This is wrong when the substring appears in a comment, is referenced multiple times, or shares a prefix with another token. Both `.csproj` and `.cs` violations are affected.

### `.csproj` violations

`quick-xml`'s `Reader` exposes `buffer_position()` after reading each event. During `ProjectFile::parse`, record the byte offset of each `Include` attribute value alongside the string.

Change `ProjectRef`:

```rust
pub struct ProjectRef {
    pub include: String,
    pub include_span: (usize, usize),  // (start_byte, len) within the file content
    pub resolved: Option<PathBuf>,
}
```

`PackageRef` gets the same treatment. Violations in `check.rs` use `pref.include_span` directly instead of calling `src.find(...)`.

### C# source violations

tree-sitter nodes carry `.start_byte()` / `.end_byte()`. Change `CsHeader::usings` from `Vec<String>` to `Vec<UsingDirective>`:

```rust
pub struct UsingDirective {
    pub namespace: String,
    pub start_byte: usize,
    pub end_byte: usize,
}
```

`check_source_rules` uses `(using.start_byte, using.end_byte - using.start_byte)` as the span.

### Scope

Pure correctness fix. No config changes, no CLI changes, no behaviour changes beyond accurate span highlighting.

---

## F — `ignorePatterns` project exclusions

### Purpose

Allow projects to be excluded from all checks and "unmatched project" warnings — not just test projects, but any projects the team wants to exempt (legacy code, vendored scaffolding, migration projects, etc.).

### Config

New optional top-level field in `ArchitectureConfig`:

```rust
#[serde(default)]
pub ignore_patterns: Vec<String>,
```

**JSON:**
```json
"ignorePatterns": ["*.Tests", "*.Specs", "*.Legacy", "*.Migrations"]
```

**Pkl:**
```pkl
ignorePatterns { "*.Tests"; "*.Specs"; "*.Legacy" }
```

**`ark init` template** ships with:
```pkl
ignorePatterns { "*.Tests"; "*.Specs"; "*.IntegrationTests" }
```

### Behaviour

A new helper `is_ignored(project_name, config) -> bool` checks the project name against `ignore_patterns` using the same glob matching as layer patterns.

In `check_dependency_rules`: if the from-project is ignored, skip entirely (no violation, no warning).
In `check_package_policies`: same.
In `check_source_rules`: no change needed — files in ignored projects won't have namespace patterns matching any layer, so they're already skipped. If they do match, the from-layer check applies; this is acceptable since source-level checks operate on namespaces, not project names.

### Implementation

- `is_ignored` lives in `src/rules.rs` (or inline in `check.rs` if rules module isn't created for feature C).
- The existing `check_dependency_rules` "unmatched project" warning path checks `is_ignored` before emitting the warning.

---

## Shared implementation note

Features C and F both need `resolve_layer` to be callable from outside `commands/check.rs`. Extract it to a shared location (`src/rules.rs`) rather than duplicating it.

---

## Out of scope

- Watch mode / file-system watcher
- Machine-readable output (JSON/SARIF)
- `.sln` file parsing
- Package version constraints
