<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Interface Design: Explorer, LSP, and MCP Server

This document defines three interfaces for ferroclass, each serving a
different interaction model. TUI and Web are the same interface with
different renderers — they share features, API requirements, and data
models. The LSP and MCP serve fundamentally different use cases.

## The Killer Feature: Merge Replay

All three interfaces share one feature that makes ferroclass genuinely
useful beyond what the CLI provides: **merge replay** — the ability to
select a node, see its inheritance chain, and click through it step by
step while watching parameters evolve.

This is how you answer "why does this node have this value?" You walk
the chain: "after `base`, port was 80. After `webserver`, port became
8080. After `production`, the `${env}` reference resolved to `prod`."
Each step shows the complete parameter state at that point in the merge.

The library needs a `merge_node_with_replay()` API that returns not just
the final merged node, but every intermediate `MergeStep` — the class
that was merged, and the full parameter state after it was applied.
See `docs/api-review.md` Phase 3 for the proposed data structures.

This feature is so central that it should be designed and built first,
before the interface shells.

## Common Requirements

All three interfaces share these needs:

| Need                                  | Current Gap                              | API Change Required            |
|---------------------------------------|------------------------------------------|--------------------------------|
| Load inventory once, query many times | Adapters re-load from disk on every call | Decouple adapters from loading |
| Search and filter nodes               | Only `get_node(name)` and `node_names()` | Query API with indexing        |
| Thread-safe inventory handle          | ~~`Rc<Value>` is `!Send + !Sync`~~ ✅ Done: `Arc<Value>` is `Send + Sync` | Phase 1 complete |
| Progress reporting during load        | No callback or event system              | Tracing spans or callback hook |
| Unified error handling                | Fragmented error types                   | Single `ferroclass::Error`     |
| Error collection (not abort on first) | ✅ Domain errors → `Ok(Failed)`; ⬜ file loading still aborts | Collect-and-continue for merge ✅; for load ⬜ |
| Warnings and informational messages   | Only `tracing::warn!` logs; no structured | Diagnostic severity levels ✅ |

## Diagnostics and Error Collection

All three interfaces need to report problems to the user. Currently,
ferroclass aborts on the first error at virtually every level: file
loading, class resolution, merging, and interpolation. This is the right
behavior for a CLI tool that produces one output, but wrong for an
interactive system that should show *all* problems at once.

### Current Behavior: Collect-and-Continue (partially implemented)

The merge pipeline now uses a collect-and-continue model for domain errors.
`merge_node()` and `merge_class()` return `Ok(Node)` or `Ok(Class)` even
when a domain error occurs (missing class, interpolation failure, type
conflict). The returned entity has `state: EntityState::Failed` and
diagnostics explaining what went wrong. Only implementation errors (I/O
failures, bugs) still return `Err`.

The `EntityState` enum tracks processing progress:
- **Source** — parsed from YAML, no merging applied
- **Merged** — class inheritance resolved, not yet interpolated
- **Interpolated** — fully processed (default for `merge_node()`)
- **Failed** — processing failed; data is NOT trustworthy

`Inventory::state()` returns the aggregate minimum across all nodes and
classes. Per-entity diagnostic summaries are stored in a
`HashMap<String, Diagnostic>` keyed by entity name, ensuring a 0-or-1
invariant.

**Still aborts on first error** (not yet collect-and-continue):
- **File loading**: First invalid YAML file → abort. Other files are
  never loaded. (Planned for Phase 2a.4/2a.5)
- **Node loading**: Duplicate node name → abort. (Will become a Failed
  node diagnostic.)

**Now collect-and-continue:**
- ✅ **Missing class reference** → node with `state: Failed` and `INV-001` diagnostic
- ✅ **Interpolation error** → node with `state: Failed` and `REF-001` diagnostic
- ✅ **Type conflict during merge** → node with `state: Failed` and `MERGE-001` diagnostic
- ✅ **Class name resolution failure** → node with `state: Failed` and `INV-002` diagnostic
- ✅ **File-level parse errors** → skipped with `PARSE-002`/`PARSE-003` diagnostic
- ✅ **Duplicate node names** → skipped with `INV-003` warning diagnostic

### Target Behavior: Collect and Continue (partially implemented)

For interactive interfaces, we need to collect as many diagnostics as
possible and report them all at once. A user editing a YAML file should
see all problems in one pass, not fix one error, re-run, fix the next,
re-run, etc.

**Implemented (Phase 2a):**
- ✅ Domain errors in merge produce `EntityState::Failed` nodes with
  diagnostics instead of `Err`. Callers check `node.is_usable()`.
- ✅ `EntityState` enum with `Source`, `Merged`, `Interpolated`, `Failed`.
- ✅ `Inventory::state()` returns aggregate minimum across all entities.
- ✅ Per-entity diagnostic summaries with 0-or-1 invariant.
- ✅ `Node::state()`, `Node::diagnostics()`, `Node::has_errors()`, etc.
- ✅ `Class::state()`, `Class::diagnostics()`, `Class::has_errors()`, etc.
- ✅ Diagnostic codes: `INV-001`, `INV-002`, `REF-001`, `MERGE-001`.

**Not yet implemented:**
- ⬜ Source location tracking: `Class` and `Node` should carry file/line
  info from the parser for LSP go-to-definition.
- ⬜ Warning-level diagnostics: duplicate class references, overridden
  constants, unused classes, etc.
- ⬜ Info/hint diagnostics: class mapping matches, deep inheritance chains,
  unused classes.
- ⬜ `load()` returns `Result<Inventory, Error>` for backward compatibility;
  `load_with_diagnostics()` returns `Result<LoadResult, Error>` with full
  diagnostics. Consider making `load()` collect-and-continue by default
  in a future version.

### Error Conditions (should be collected, not abort)

| Condition                                | Current Behavior         | Target Behavior                          |
|------------------------------------------|--------------------------|------------------------------------------|
| YAML parse error in a class/node file    | Abort                    | Collect; continue loading other files     |
| Missing class reference                  | Abort (unless ignored)   | Error diagnostic on the reference         |
| Missing node reference                   | Abort                    | Error diagnostic on the reference         |
| Circular reference in interpolation      | Abort                    | Error diagnostic on the circular path     |
| Type conflict during merge               | Abort                    | Error diagnostic on the conflicting key    |
| Unresolved `${...}` reference            | Abort (unless grouped)  | Error diagnostic on the reference          |
| Invalid `~`/`=` usage                    | Abort                    | Error diagnostic on the key                 |
| Invalid YAML structure (wrong types)     | Abort                    | Error diagnostic on the file                |
| Duplicate node name                      | Abort                    | Error diagnostic on both files              |

### Warning Conditions (currently silent or log-only)

These are currently not surfaced at all — they happen silently during
merge or are only visible via `tracing::warn!`:

| Condition                                | Current Behavior         | Target Behavior                          |
|------------------------------------------|--------------------------|------------------------------------------|
| Duplicate class in `classes:` list       | Silently deduplicated    | Warning: "class X listed twice"           |
| Override of `=` (constant) parameter    | Abort if `strict`        | Warning (non-strict): "constant X overridden" |
| Value overridden by later class          | Silent                   | Info: "key X set by class A, overridden by class B" |
| `~` (override) marker used              | Silent                   | Hint: "override marker on key X"          |
| Class mapping pattern matched            | `tracing::debug!`        | Info: "class mapping 'web*' matched node 'web01'" |
| Ignored missing class (`ignore_class_notfound`) | `tracing::warn!` | Warning: "class X not found, ignored"    |
| Overwritten missing reference            | Silent (default on)      | Info: "reference ${X} not found but overwritten by later class" |

### Informational Conditions (new)

These don't exist today but would be valuable:

| Condition                                | Target Behavior          |
|------------------------------------------|--------------------------|
| Unused class (defined but not referenced by any node) | Hint: "class X is not used by any node" |
| Parameter that could use a reference     | Hint: "value 'web01' matches node name, consider ${name}" |
| Deep inheritance chain                   | Info: "class X inherits from 7 levels deep" |
| Class referenced only once              | Hint: "class X is only used by node Y, consider inlining" |

### Library Changes Needed

#### 1. Diagnostic Type ✅ Done

```rust,ignore
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<SourceLocation>,  // file, line, column
    pub code: Option<String>,             // e.g., "INV-001", "REF-001"
    pub subject: Option<String>,          // entity name this diagnostic is about
}

pub enum EntityState {
    Source,        // Parsed from YAML, no merging applied
    Merged,        // Class inheritance resolved, not yet interpolated
    Interpolated,  // Fully processed (default)
    Failed,        // Processing failed; data is NOT trustworthy
}
```

`Diagnostic` is attached to `Node`, `Class`, and `Inventory`. Per-entity
summaries are stored in `Inventory.entity_diagnostics` (a `HashMap<String,
Diagnostic>` keyed by entity name, ensuring 0-or-1 invariant).

#### 2. Error Collection in `load()` ⬜ Not yet

Currently `load()` returns `Result<Inventory, Error>`. It should return
`Result<DiagnosticReport, Error>` where `DiagnosticReport` contains both
the collected diagnostics and the (possibly partial) inventory. Fatal
errors (can't read directory, invalid config) still return `Err`. All
per-file, per-class, and per-node errors become diagnostics in the report.

```rust,ignore
// Before:
pub fn load(options: &StorageOptions) -> Result<Inventory, Error>;

// After:
pub fn load(options: &StorageOptions) -> Result<DiagnosticReport, Error>;
// DiagnosticReport { diagnostics: Vec<Diagnostic>, inventory: Option<Inventory> }
```

#### 3. Error Collection in Merge ✅ Done

`merge_node()` now returns `Result<Node, Error>` where domain errors produce
`Ok(Node)` with `node.state() == EntityState::Failed` and diagnostics.
Only implementation errors (I/O, bugs) still return `Err`.

```rust,ignore
let node = inventory.merge_node("web01")?;
if !node.is_usable() {
    for diag in node.diagnostics() {
        eprintln!("{}: {} [{}]", diag.severity, diag.message, diag.code.unwrap_or(""));
    }
    // Handle failed node
}
```

#### 4. Source Locations

The parser currently discards file/line information after parsing. The
LSP needs this for go-to-definition and diagnostic locations. The library
needs it for meaningful error messages.

```rust,ignore
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
}
```

This must be stored on `Class`, `Node`, and `Value` during parsing.
The YAML parser (`yaml_rust2`) provides `Mark` objects with line/column
info that we currently ignore.

#### 5. Merge Provenance (for Warnings)

To produce "key X overridden by class Y" warnings, we need to track which
class contributed each key. This overlaps with the merge replay feature
(`MergeStep`), but provenance tracking can be lighter weight — just a
`HashMap<Key, String>` mapping parameter keys to source class names.

### Phasing

| Phase | Change                                                    | Effort | Status |
|-------|-----------------------------------------------------------|--------|--------|
| 0     | Add `Diagnostic` and `DiagnosticSeverity` types           | Low    | ✅ Done |
| 0     | Add `EntityState` enum (Source/Merged/Interpolated/Failed) | Low    | ✅ Done |
| 0     | Add `SourceLocation` to `Class` and `Node` during parse   | Medium | ⬜ Not yet |
| 1     | Change `merge_node()` to return `Ok(Failed)` for domain errors | Medium | ✅ Done |
| 2a    | Add per-entity diagnostic summaries with 0-or-1 invariant | Medium | ✅ Done |
| 2a    | Change `load()` to collect per-file errors and continue   | Medium | ✅ Done |
| 2a    | Add `LoadResult` type with diagnostics                   | Medium | ✅ Done |
| 2a    | Change `add_node()` to accept duplicates as warnings       | Low    | ✅ Done |
| 2     | Extend `group_errors` to cover all interpolation errors   | Medium | ⬜ Not yet |
| 2     | Add warning-level diagnostics (duplicates, overrides)    | Medium | ⬜ Not yet |
| 3     | Add info/hint diagnostics (unused classes, deep chains)   | Low    | ⬜ Not yet |

Phase 0 and 1 changes are prerequisites for the LSP. Phase 2 and 3 can
come later.

---

## 1. Explorer (TUI + Web UI)

The Explorer is the interactive inventory browser. It has two frontends —
a terminal UI and a web UI — sharing the same features and data model.
The TUI is for operators who live in the terminal. The Web UI adds
shareable URLs, collaborative access, and richer rendering.

### Target Users

- Operations engineers debugging why a node has certain parameters
- Developers checking class inheritance chains
- Teams sharing inventory exploration via browser
- Anyone wanting quick inventory exploration without remembering CLI flags

### Core Features

#### Merge Replay (the main feature)

- Select a node → see its full inheritance chain (class by class)
- Click on any step in the chain → see the complete parameter state at
  that point in the merge
- See which class contributed which keys (new keys highlighted, changed
  keys marked)
- Toggle between pre-interpolation view (raw `${...}` references visible)
  and post-interpolation view (all references resolved)
- Step forward/backward through the merge like a debugger
- Diff any two steps to see exactly what changed

```
┌─ Merge Replay: web01 ──────────────────────────────────────────┐
│ Step 1/5: ClassMapping "default"                               │
│ Step 2/5: AutoParameters                                       │
│ ▸ Step 3/5: Class "base"                    ◄── selected       │
│   Step 4/5: Class "webserver"                                   │
│   Step 5/5: Node fields                                        │
├────────────────────────────────────────────────────────────────┤
│ Parameters after step 3 (class "base"):                        │
│   hostname: web01                                             │
│   role: base                                                  │
│ + base_setting: true           ◄── new key from this step     │
│ + log_level: INFO              ◄── new key from this step     │
│                                                                │
│ [Pre-interpolation] [Post-interpolation] [Diff from step 2]  │
└────────────────────────────────────────────────────────────────┘
```

#### Node Browser

- List all nodes with optional filtering (by name pattern, class, environment)
- Select a node → see merged parameters, classes, applications, exports
- Expand/collapse nested parameter values
- Show the merge order: which class contributed which parameter

#### Class Browser

- List all classes
- Select a class → see its parameters, applications, environment
- Show which nodes include a given class
- Trace class inheritance (class A includes class B includes class C)

#### Search

- Full-text search across all node parameters
- Find nodes where a specific key is set (or unset)
- Find nodes matching a parameter value pattern
- Regex and glob support

#### Configuration View

- Show current `Options` (storage type, inventory base URI, merge config)
- Show class mappings (glob/regex patterns and their target classes)
- Allow toggling `automatic_parameters`, `class_mappings_match_path`

### TUI Specifics

The TUI runs an event loop (e.g., ratatui/crossterm). Inventory is loaded
once at startup (or on user command), then queried repeatedly:

```
┌─ Node List ─────────────────┐ ┌─ Node Detail: web01 ─────────────┐
│ ▸ web01                      │ │ Step 3/5: Class "webserver"       │
│   web02                      │ │ + port: 8080                     │
│   db01                       │ │ + role: webserver                │
│ ▸ db02                       │ │                                   │
│   monitor01                  │ │ [◀ Step 2] [Step 4 ▶]            │
│                              │ │ [Pre-interp] [Post-interp] [Diff] │
│ [Search: role=web]           │ └──────────────────────────────────┘
└──────────────────────────────┘
```

**Dependencies**: `ratatui` + `crossterm`, thread-safe inventory (Phase 1),
query API (Phase 2), merge replay (Phase 3).

### Web UI Specifics

The web version of merge replay is especially powerful because clicking
through steps is a natural browser interaction. Each step in the chain
is a clickable element; the parameter panel updates live.

- Step-by-step navigation with prev/next buttons and clickable step list
- Diff view: highlight keys added, changed, or overridden at each step
- Pre/post interpolation toggle: show raw `${...}` references or resolved
  values with a single click
- Shareable URLs: `/nodes/web01/replay?step=3` links directly to a
  specific merge step
- Side-by-side diff of any two steps

#### REST API

The web UI is backed by a JSON API that can also be used programmatically:

| Endpoint                                   | Description                                         |
|--------------------------------------------|-----------------------------------------------------|
| `GET /api/v1/nodes`                        | List all nodes (with optional filters)              |
| `GET /api/v1/nodes/:name`                  | Get merged node detail                              |
| `GET /api/v1/nodes/:name/parameters`       | Get merged parameters only                          |
| `GET /api/v1/nodes/:name/classes`          | Get resolved class list                             |
| `GET /api/v1/nodes/:name/replay`           | Full merge replay: all steps with intermediate state |
| `GET /api/v1/nodes/:name/replay/:step`     | Single merge step (for step-by-step navigation)     |
| `GET /api/v1/nodes/:name/raw`              | Get node before interpolation/merge                 |
| `GET /api/v1/classes`                       | List all classes                                    |
| `GET /api/v1/classes/:name`                | Get class detail                                    |
| `GET /api/v1/classes/:name/usage`           | Which nodes use this class                          |
| `GET /api/v1/search?q=pattern`             | Full-text search across nodes and parameters        |
| `GET /api/v1/inventory/config`              | Current inventory configuration                     |
| `POST /api/v1/inventory/reload`             | Reload inventory from disk                          |
| `GET /api/v1/formats/ansible?node=name`    | Ansible-formatted output                            |
| `GET /api/v1/formats/salt?node=name`        | Salt-formatted output                               |

#### WebSocket Updates

- Notify connected clients when inventory files change on disk (via inotify/polling)
- Push incremental merge results as they complete
- Show merge progress bar for large inventories

#### Multi-User Concerns

- Read-only by default (inventory is files on disk, not edited through the API)
- Optional write endpoint for creating/modifying node and class YAML files (future scope)
- Authentication via API key or HTTP basic auth (simple)

### API Requirements (Explorer)

| Explorer Feature                | Library API Needed                                                                    |
|---------------------------------|---------------------------------------------------------------------------------------|
| Node list with filters          | `find_nodes(pattern)`, `find_nodes_by_class(name)`, `find_nodes_by_environment(env)`  |
| Merged node parameters          | `inventory.merge_node(name)` (already exists)                                         |
| Merge replay                    | `inventory.merge_node_with_replay(name)` → `MergeReplay` with `Vec<MergeStep>`        |
| Class list and lookup           | `inventory.class_names()`, `inventory.get_class(name)` (already exists)               |
| Class inverse lookup            | `find_nodes_by_class(name)`                                                           |
| Unresolved interpolation view   | Pre-interpolation state from `MergeReplay::pre_interpolation_parameters`              |
| Circular reference detection    | Already detected during merge; expose as structured data                              |
| Full-text parameter search      | `search_parameters(query)` returning node names + matching keys                       |
| Reload inventory from disk      | `inventory.reload()` or `load()` again (needs decoupled adapters)                     |
| JSON serialization of all types | `Serialize` already exists for most types; verify completeness                       |
| Streaming merge progress        | Callback or channel-based progress reporting                                           |
| File watch + reload             | `inventory.reload()` or `load()` again; decouple from adapter                          |
| Concurrent read access          | `Arc<Inventory>` ✅ (Phase 1 complete: Send + Sync)                                   |
| Ansible/Salt format endpoints   | Decoupled adapter functions ✅ (Phase 0 complete)                                      |

### Technology Choices

**TUI**: `ratatui` + `crossterm` for rendering. Single binary, no network.

**Web UI**: `axum` or `actix-web` for the backend. Start with
server-rendered HTML (askama/templates) for simplicity, add JSON API
endpoints from day one. File watching via `notify` crate.
`Arc<Inventory>` behind `RwLock` for concurrent reads.

---

## 2. LSP (Language Server Protocol)

An LSP meets users where they already work — their editor. Inventory
YAML files reference other YAML files (classes, nodes). The LSP turns
those references into navigable, validated, auto-completable links,
exactly like an LSP does for code.

### Target Users

- Operations engineers editing inventory YAML in VS Code, Neovim, or any LSP-aware editor
- Developers writing new classes or nodes who need autocompletion
- Anyone wanting real-time validation of class references and interpolation

### Core Features

#### Tier 1: Immediately Useful

| Feature          | LSP Method                      | What it does                                                                                                    |
| ---------------- | ------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Diagnostics      | `textDocument/publishDiagnostics` | All errors, warnings, and hints from the diagnostic system: missing classes, circular references, unresolved `${...}`, invalid `~`/`=` usage, YAML errors, duplicate class refs, overridden constants, unused classes |
| Go-to-definition | `textDocument/definition`         | Click a class name in a node file → jump to the class YAML file. Click a node name → jump to the node file          |
| Find references  | `textDocument/references`         | Right-click a class → find all nodes that include it. Right-click a node → find all places it's referenced          |
| Hover            | `textDocument/hover`              | Hover over a class name in a node → see the merged parameters that class contributes. Hover over `${foo:bar}` → see the resolved value |

#### Tier 2: Very Useful

| Feature           | LSP Method                      | What it does                                                                                                                          |
| ----------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Autocomplete      | `textDocument/completion`         | Type a class name → get suggestions from all classes in the inventory. Type `${` → suggest parameter paths. Type `~` or `=` → suggest override/constant markers |
| Document symbols  | `textDocument/documentSymbol`     | Outline view showing classes, parameters, applications, exports in the current file                                                    |
| Workspace symbols | `workspace/symbol`                | Fuzzy search for class or node names across the entire inventory                                                                       |
| Merge replay      | CodeLens or custom notification   | Click a "show merge steps" lens on a node → open a panel showing step-by-step merge (the killer feature, surfaced via LSP)           |

#### Tier 3: Nice to Have

| Feature          | LSP Method                       | What it does                                                                                                    |
| ---------------- | -------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Rename           | `textDocument/rename`              | Rename a class → update all nodes that reference it                                                            |
| Code actions     | `textDocument/codeAction`          | "Add missing class" quick fix, "Extract parameters to new class"                                                |
| Folding          | `textDocument/foldingRange`        | Fold classes, parameters, exports sections                                                                     |
| Semantic tokens  | `textDocument/semanticTokens/full` | Color class references differently from strings, highlight `${...}` interpolation differently from plain strings |
| File watching    | `workspace/didChangeWatchedFiles`  | Re-merge inventory when any file changes, push updated diagnostics                                             |

### How the LSP Maps to the Library

```
Diagnostics ──────→ inventory.load() + inventory.merge_node()
                     catch Error variants, map to LSP DiagnosticSeverity

Go-to-definition ──→ storage layer: find which file defines a class/node
                     (needs: file path → line number mapping from parser)

Find references ───→ query API: find_nodes_by_class(name)
                     + text search for class name in node files

Hover ─────────────→ merge_node_with_replay(name) → show step parameters
                     or merge_node(name) → show final merged parameters

Autocomplete ──────→ inventory.class_names() / inventory.node_names()
                     + parameter key names from merged node

Merge replay ──────→ merge_node_with_replay(name) → CodeLens / custom notification
```

### What the LSP Needs from the Library

| Need                          | Current state                                | What's needed                                                                                     |
| ----------------------------- | -------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| Diagnostic collection        | Aborts on first error; warnings are log-only | `DiagnosticReport` type that collects errors/warnings/hints per file, per node, per key          |
| Source location mapping       | Parser discards file/line info after parsing | Store `SourceLocation { file, line, col }` on each `Class`, `Node`, and `Value` during parsing    |
| File path to entity mapping   | `Inventory` stores entities by name, not file | A `HashMap<String, PathBuf>` mapping class/node names to their YAML files                          |
| Incremental re-parse          | `load()` re-reads everything from disk        | Watch for file changes, re-parse only affected files, re-merge affected nodes                      |
| Query API                     | Only exact name lookup and full iteration     | `find_nodes_by_class()`, `find_nodes_by_environment()`, `search_nodes()`                           |
| Merge replay                  | Not yet implemented                          | `merge_node_with_replay()` from Phase 3 in `api-review.md`                                         |
| Thread-safe inventory         | ~~`Rc<Value>` is `!Send + !Sync`~~ ✅ `Arc<Value>` is `Send + Sync`               | ~~Phase 1~~ Done                                                                                    |

**Source location tracking** and **diagnostic collection** are the two
things not yet in the roadmap that the LSP specifically needs. Both are
also valuable for the CLI — better error messages with file/line numbers
and collecting all errors instead of aborting on the first one.

**Diagnostic collection** is the biggest change: it requires converting
the entire pipeline from `Result<T, Error>` abort-on-first-error to a
collect-and-continue model where `load()` returns a `DiagnosticReport`
with both the collected problems and the (possibly partial) inventory.
This affects every layer: file loading, class resolution, merging, and
interpolation.

### Architecture

```
┌─────────────────────────────────────────────────────┐
│            Editor (VS Code, Neovim, etc.)            │
│            ↕ LSP protocol (JSON-RPC over stdio)      │
├─────────────────────────────────────────────────────┤
│              ferroclass-lsp (binary)                  │
│                                                      │
│  ┌──────────────────┐  ┌──────────────────────────┐  │
│  │  LSP Protocol     │  │  Inventory Manager        │  │
│  │  Handler          │  │  (owns Arc<Inventory>)     │  │
│  │  (tower-lsp)      │  │  - file watching (notify)  │  │
│  │                   │  │  - incremental re-parse    │  │
│  │                   │  │  - merge on change          │  │
│  └──────────────────┘  └──────────────────────────┘  │
│           │                        │                  │
│           │     ┌──────────────────┘                  │
│           ▼     ▼                                     │
│  ┌────────────────────────┐                          │
│  │  ferroclass (library)    │                          │
│  │  - load()               │                          │
│  │  - merge_node()         │                          │
│  │  - merge_node_with_     │                          │
│  │    replay()             │                          │
│  │  - query API            │                          │
│  │  - source locations     │                          │
│  └────────────────────────┘                          │
└─────────────────────────────────────────────────────┘
```

### Technology Choices

- **LSP framework**: `tower-lsp` — standard Rust LSP framework, handles JSON-RPC transport, protocol types, request routing
- **Protocol types**: `lsp-types` crate — all LSP protocol definitions
- **File watching**: `notify` crate — cross-platform inotify/FSEvents/kqueue
- **Transport**: stdio (standard for LSP servers)
- **Concurrency**: `Arc<Inventory>` behind `RwLock`; background re-merge on file change

### Effort Estimate

| Component                                              | Effort    | Notes                                         |
| ------------------------------------------------------ | --------- | --------------------------------------------- |
| LSP skeleton (tower-lsp, JSON-RPC, basic lifecycle)    | 1-2 weeks | Boilerplate, well-documented                  |
| Diagnostics (push errors on file open/save)             | 1 week    | Map existing error types to LSP diagnostics   |
| Go-to-definition                                       | 2-3 weeks | Requires source location tracking in parser   |
| Find references                                        | 1 week    | Requires query API                            |
| Hover                                                  | 1 week    | Requires merge replay or at least merge_node  |
| Autocomplete                                           | 1-2 weeks | Needs query API + parameter key extraction    |
| Merge replay via CodeLens                              | 2-3 weeks | Requires full merge replay API                |
| Incremental re-parse                                   | 2-3 weeks | Hard; can start with full re-parse + debounce |
| File watching + auto-diagnose                          | 1 week    | `notify` crate                                  |

**Total: ~12-16 weeks** for a usable LSP with diagnostics, go-to-definition,
references, hover, and autocomplete. Merge replay and incremental parsing
add another 4-6 weeks.

### Dependencies on Library Phases

| LSP Feature          | Depends on (from api-review.md)                         | Status |
| -------------------- | ------------------------------------------------------- | ------ |
| Diagnostics          | Phase 0: diagnostic types ✅ + collection API ✅ (merge only, load not yet) | ✅ Partial |
| Go-to-definition     | Phase 0: source location tracking in parser (not yet)   | ⬜ Not yet |
| Find references      | Phase 2: query API ✅                                   | ✅ Done |
| Hover / merge replay | Phase 3: merge replay API                               | ⬜ Not yet |
| Autocomplete         | Phase 0: public `class_names()`/`node_names()` ✅       | ✅ Done |
| Thread safety        | Phase 1: `Arc<Value>` ✅                                | ✅ Done |
| File watching        | Phase 2: incremental load/reload                        | ⬜ Not yet |

---

## 3. MCP (Model Context Protocol) Server

An MCP server exposes inventory data to AI agents, allowing them to
answer questions like "which nodes are web servers?" or "what parameters
does db01 inherit from the postgres class?" without loading raw YAML files.

### Target Users

- AI coding assistants (via MCP client integration)
- Automation tools that query inventory programmatically
- ChatOps bots

### Core Features

#### MCP Tools

The server exposes these tools following the MCP specification:

| Tool Name                          | Parameters                        | Returns                                    |
|------------------------------------|------------------------------------|--------------------------------------------|
| `list_nodes`                       | `filter?: string`, `class?: string`, `environment?: string` | List of node names with summary |
| `get_node`                         | `name: string`                     | Full merged node (parameters, classes, applications, exports) |
| `get_node_replay`                  | `name: string`                     | Merge replay: all steps with intermediate parameter state |
| `get_node_parameters`              | `name: string`                     | Merged parameters only (as dict)            |
| `get_node_classes`                  | `name: string`                     | Resolved class list with merge order        |
| `get_node_merge_provenance`        | `name: string`, `key?: string`     | Which class contributed each parameter value |
| `list_classes`                     | `filter?: string`                  | List of class names                         |
| `get_class`                        | `name: string`                     | Class detail (parameters, applications)     |
| `find_class_usage`                 | `name: string`                     | Nodes that include this class               |
| `search_parameters`                | `query: string`, `value?: string`  | Nodes matching key/value pattern            |
| `get_inventory_config`             | —                                  | Current options, storage type, class mappings |
| `reload_inventory`                 | —                                  | Reload from disk                            |
| `get_interpolation_sources`       | `node: string`, `key: string`      | Show reference chain for an interpolated value |
| `explain_merge`                    | `node: string`                     | Human-readable explanation of merge order and conflicts |

#### MCP Resources

Static data exposed as URI-addressable resources:

| URI Pattern                            | Description                              |
|----------------------------------------|------------------------------------------|
| `ferroclass://nodes`                   | All node names                            |
| `ferroclass://nodes/{name}`            | Merged node detail                        |
| `ferroclass://nodes/{name}/replay`      | Merge replay: step-by-step merge history  |
| `ferroclass://nodes/{name}/raw`         | Node before merge/interpolation           |
| `ferroclass://classes`                  | All class names                           |
| `ferroclass://classes/{name}`           | Class detail                              |
| `ferroclass://config`                   | Inventory configuration                   |

#### MCP Prompts

Pre-configured prompt templates:

| Prompt Name                    | Description                                              |
|--------------------------------|----------------------------------------------------------|
| `debug_node`                   | "Investigate why node X has parameter Y set to Z"        |
| `trace_merge`                  | "Walk the merge chain for node X step by step"           |
| `compare_nodes`                | "Compare the parameters of node A and node B"           |
| `trace_class`                  | "Show the full class inheritance chain for class X"      |
| `find_unused_classes`          | "Find classes not included by any node"                 |

### API Requirements (MCP)

| MCP Feature                     | Library API Needed                                          |
|----------------------------------|-------------------------------------------------------------|
| JSON-serializable all types      | `Serialize` on `Node`, `Class`, `Inventory`, `Value`       |
| Merge replay                     | `merge_node_with_replay(name)` → `MergeReplay` with `Vec<MergeStep>` |
| Human-readable merge explanation | `explain_merge(name)` → formatted string                     |
| Reference chain tracing          | `get_interpolation_sources(node, key)` → list of references |
| Lightweight startup              | Lazy loading or incremental inventory construction          |
| stdio transport                  | MCP servers communicate over stdin/stdout (no HTTP needed) |

### Technology Choices

- **MCP SDK**: `rmcp` crate (Rust MCP implementation) or `mcp-rust-sdk`
- **Transport**: stdio (for local agent integration) or HTTP/SSE (for remote)
- **No web framework needed**: MCP is a JSON-RPC protocol over stdio
- **Inventory as `Arc<Inventory>`**: loaded once, queried many times
- **Schema**: Define MCP tool schemas as JSON Schema matching the parameter
  types above

---

## API Changes Needed (Summary)

### Must Have (for all three interfaces)

| Change                                        | Phase  | Effort | Impact                                      | Status |
|-----------------------------------------------|--------|--------|---------------------------------------------|--------|
| Decouple adapters from loading                | Phase 0 | Medium | Enables all interfaces                       | ✅ Done |
| Unify error types                              | Phase 0 | Low    | Cleaner public API                           | ✅ Done |
| Diagnostic collection (`DiagnosticReport`)   | 0 (types added, collection not yet) | High   | Collect errors instead of aborting — required for LSP, useful for all | ✅ Types + Collection |
| Source location mapping                       | 0 (types added, tracking not yet) | Medium | Required for LSP go-to-def, better error messages everywhere | ⬜ Not yet |
| Make `add_node`/`add_class` public            | Phase 0 | Trivial| Enables programmatic construction            | ✅ Done |
| Switch `Rc` → `Arc` in `Value`                | Phase 1 | Medium | Enables thread safety                        | ✅ Done |
| Query API (filter, search)                    | Phase 2 | Medium | Enables all interfaces                       | ✅ Done |
| `EntityState` and entity diagnostics          | Phase 2a| Medium | Track processing progress, collect domain errors | ✅ Done |
| `merge_node()` returns `Ok(Failed)` for domain errors | Phase 2a | Medium | Collect-and-continue for merge errors | ✅ Done |
| `load_with_diagnostics()` collects per-file errors | Phase 2a | Medium | Collect-and-continue for loading errors | ✅ Done |
| `LoadResult` type                             | Phase 2a | Medium | Structured return with inventory + diagnostics | ✅ Done |
| `add_node()` accepts duplicates as warnings   | Phase 2a | Low    | Don't abort on duplicate node names | ✅ Done |
| `load()` returns `Result<Inventory, Error>` for backward compat | Phase 2a | Low | Existing callers don't need changes | ✅ Done |

### Should Have (the core differentiator)

| Change                                   | Interface      | Effort | Impact                                          |
|------------------------------------------|----------------|--------|-------------------------------------------------|
| Merge replay API                         | All            | Medium | Step-by-step merge inspection — the killer feature |
| Pre/post interpolation toggle            | All            | Low    | Show raw `${...}` refs vs resolved values        |
| Warning-level diagnostics                | All            | Medium | Duplicate classes, overridden constants, unused classes |
| Interpolation source tracing             | Explorer, MCP  | Medium | Debug reference chains                          |
| `explain_merge()` human-readable output  | MCP            | Low    | Agent-friendly explanations                     |
| Lazy/incremental inventory loading        | Explorer, MCP  | High   | Fast startup for large inventories              |

### Nice to Have

| Change                                   | Interface | Effort | Impact                     |
|------------------------------------------|-----------|--------|----------------------------|
| File watching + auto-reload              | Explorer  | Medium | Live updates               |
| Write endpoints (create/modify nodes)    | Explorer  | Very High | Full lifecycle management |
| Requirements/prerequisites validation    | All       | High   | Policy enforcement         |
| Rename refactoring                        | LSP       | Medium | Rename class → update all refs |
| Code actions (extract class, add missing) | LSP       | High   | Quick fixes and refactors  |

---

## Sequencing: LSP First, MCP Second, Explorer Third

The three interfaces depend on the same library improvements, but the
implementation order matters for impact and effort:

```
Phase 0 (API cleanup + diagnostics) ✅ Done ──┐
Phase 1 (thread safety) ✅ Done                 ├── LSP v1
Phase 2a (collect-and-continue) ✅ Done ────────┘
Phase 2b (source location tracking) ────────────── LSP v1 (go-to-def)
Phase 3 (merge replay) ────────────────────────── MCP v1
Phase 4 (Explorer) ────────────────────────────── TUI + Web UI
```

### Why LSP first

- **No UI to build.** The editor IS the UI. We implement a protocol, not
  a rendering layer. This is dramatically less work than a TUI or web app.
- **Highest user impact.** Users spend their time in editors. Showing
  diagnostics, go-to-definition, and hover in the exact context where they
  edit YAML files is transformative.
- **Smallest surface area.** An LSP server is a JSON-RPC process over stdio.
  No HTTP server, no authentication, no multi-user concerns. Just load
  inventory, answer questions.
- **Forces library improvements.** The LSP requires error collection
  (Phase 0-2), source locations (Phase 0), and query API (Phase 2) — all
  things that benefit every other interface too.
- **~12-16 weeks** for a usable v1 with diagnostics, go-to-definition,
  references, hover, and autocomplete.

### Why MCP second

- **Even smaller surface area than LSP.** JSON-RPC over stdio, no rendering.
- **Lower impact than LSP** — agents are useful but humans are the primary
  audience. An MCP server is a nice-to-have, not the game-changer that an
  LSP is.
- **Depends on merge replay** (Phase 3) for the `get_node_replay` and
  `explain_merge` tools, which are the most useful MCP features.
- **~4-6 weeks** after Phase 3 is complete.

### Why Explorer last

- **Most work.** Two renderers (TUI + Web), REST API, WebSocket, file
  watching, authentication, merge replay UI. This is a full application,
  not a protocol adapter.
- **Merge replay UI** is the killer feature but requires significant
  frontend investment — step navigation, diff views, pre/post
  interpolation toggles.
- **~20-30 weeks** for a usable v1 with both TUI and Web frontends.

---

## Open Questions

1. **Merge replay: snapshot granularity** — Should each step snapshot the
   full `MergeAccumulator` (parameters, classes, applications, environment,
   exports), or just parameters? Full snapshots cost more memory but enable
   "show me which classes were resolved at step 3." Parameter-only snapshots
   are lighter but lose that context.

2. **Merge replay: depth-first visibility** — When class A inherits from B
   which inherits from C, the depth-first resolution merges C → B → A. Should
   the replay show each parent class as a separate step (C step, then B step,
   then A step), or collapse the entire inheritance resolution into one step?
   Showing each parent class is more useful for debugging but produces more
   steps.

3. **Interpolation before/after** — Should we expose both the unresolved
   (references visible) and resolved (interpolated) views of a node?
   The `MergeReplay` design captures `pre_interpolation_parameters` which
   contains `Value::Reference`, `Value::DeferredMerge`, etc. The interface
   needs to render these meaningfully — e.g., `${host:name}` shown as
   "reference to host:name → resolved to 'web01'".

4. **Incremental loading** — For large inventories, should we support loading
   nodes on demand (lazy) rather than all at once? This would require
   restructuring `Inventory::load()` significantly.

5. **Explorer: read-only or read-write?** — A read-only explorer is
   straightforward. A read-write explorer needs YAML file generation,
   validation, and conflict resolution. Start read-only.

6. **MCP: local or remote?** — Local (stdio) is simpler and matches how
   agents currently use MCP. Remote (HTTP/SSE) enables shared instances.
   Start local.

7. **Authentication** — The Explorer (web frontend) needs at least API key
   auth. The MCP server (stdio) inherits the agent's local access. The LSP
   inherits the editor's local access. How much auth infrastructure do we
   build?

8. **Binary layout** — Should each interface be a separate binary
   (`ferroclass-tui`, `ferroclass-web`, `ferroclass-lsp`, `ferroclass-mcp`)
   or subcommands of the `reclass` binary? Separate binaries align with the
   architecture rule that binaries are thin wrappers over the library.

9. **Diff computation** — For the "diff any two steps" feature in merge
   replay, do we need a structural diff on `ParametersType` (showing added,
   removed, and changed keys with old/new values)? This is similar to
   `serde_json` diffing. Could use a simple key-by-key comparison since
   `ParametersType` is a `LinkedHashMap<Key, Value>`.

10. **LSP incremental re-parse** — For large inventories, re-parsing
    everything on every keystroke is too slow. Options: (a) full re-parse
    with 500ms debounce, (b) incremental re-parse of only changed files,
    (c) background re-merge thread. Start with (a), evolve to (b).

11. **LSP source locations** — The parser currently discards file/line info.
    Adding `SourceLocation { file: PathBuf, line: usize, col: usize }` to
    `Class`, `Node`, and key `Value` entries is needed for go-to-definition
    and better error messages. How much of the parser needs to change?