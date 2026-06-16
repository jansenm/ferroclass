<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# API Review: Current State and Improvement Plan

This document assesses the current ferroclass library API surface and
identifies changes needed to support external interfaces (TUI, web, MCP).

## Current Public API

### Crate Root Re-exports (`src/lib.rs`)

```rust,ignore
pub use inventory::Inventory;
pub use inventory::load;
pub use inventory::load_from_yaml_string;
pub use inventory::load_from_yaml_string_with_uri;
pub use inventory::options::Options;
pub use inventory::options::{
    MergeConfig, OutputFormat, OutputOptions, ParameterKeyStyle,
    StorageOptions, StorageOptionsTrait, StorageType,
    YamlFileStorageOptions, YamlFsStorageOptions,
};
```

### Core Domain Types

| Type                  | Location                          | Purpose                              |
|-----------------------|-----------------------------------|--------------------------------------|
| `Inventory`           | `src/inventory.rs:79`            | Central data structure               |
| `Value`               | `src/inventory/value.rs:140`     | 14-variant value enum               |
| `Key`                 | `src/inventory/value.rs:44`      | Map key type                         |
| `Class`               | `src/inventory/elements/class.rs`| Class domain type with builder       |
| `Node`                | `src/inventory/elements/node.rs` | Node domain type with builder        |
| `ClassMapping`        | `src/inventory/class_mapping.rs` | Glob/regex pattern to auto-include  |
| `MergeConfig`         | `src/inventory/options/`        | Merge behavior configuration        |

### Adapter Output Types

| Type                  | Location                          | Purpose                              |
|-----------------------|-----------------------------------|--------------------------------------|
| `AnsibleInventory`   | `src/output/ansible.rs`           | Ansible dynamic inventory            |
| `AnsibleNodeInfo`    | `src/output/ansible.rs`           | Per-host variables                   |
| `HostVars`           | `src/output/ansible.rs`           | Single-host parameters               |
| `TopData`            | `src/output/salt.rs`              | Salt top data                        |
| `InventoryOutput`    | `src/output/reclass.rs`           | Full inventory listing               |
| `NodeInfoOutput`     | `src/output/reclass.rs`           | Single-node detail output            |

### Python Bindings (PyO3, feature-gated)

```python
import ferroclass

inv = ferroclass.Inventory(...)       # Load + merge_node + node_names
node = ferroclass.Node(...)            # Read-only access to merged node
pillar = ferroclass.ext_pillar(...)    # Salt external pillar
top_data = ferroclass.top(...)         # Salt master tops
inv = ferroclass.load(...)             # Low-level loader
```

## Data Flow

The pipeline follows the three-layer architecture:

```
Loading → Core Inventory → Adapter → Format → String
```

1. **Loading**: `ferroclass::load(&storage_options)` reads YAML from disk,
   parses into `Class` and `Node` domain objects, assembles `Inventory`.
2. **Core Merging**: `inventory.merge_node("name")` resolves class
   inheritance, merges parameters, detects references.
3. **Adapter**: Each adapter takes `Options` + inventory, produces an
   output type with `__reclass__` metadata injected.
4. **Format**: `format_output(value, format, pretty, sorted)` dispatches
   to JSON (`Serialize`) or YAML (`to_yaml_value()`).

## Architectural Constraints

### ~~1. `Rc<Value>` blocks thread safety~~ ✅ Done

Switched all `Rc<LinkedHashMap>` and `Rc<Array>` to `Arc` counterparts in
v0.13.0. `MergeConfig::compiled_class_notfound_regexp` changed from
`Option<Regex>` to `Arc<Mutex<Option<Regex>>>` (Regex is `!Send + !Sync`).
`PyInventory` and `PyNode` no longer need `#[pyclass(unsendable)]`.
Static assertions confirm `Inventory`, `Class`, `Node`, `Value`, and
`MergeConfig` are all `Send + Sync`.

### ~~2. Adapter builders are coupled to loading~~ ✅ Done

Added `build_inventory_from()`, `build_host_vars_from()`, `build_top_from()`,
and `build_pillar_from()` variants in v0.13.0. These accept a pre-loaded
`&Inventory` instead of calling `Inventory::load()` internally. The
original loading-based functions remain as convenience wrappers that
delegate to the `_from()` variants.

### ~~3. No query or filter API~~ ✅ Done (v0.13.0)

Added `find_nodes_by_class()`, `find_nodes_by_resolved_class()`,
`find_nodes_by_environment()`, `search_nodes()`, and `class_names()`.
Reverse indexes (`class_to_nodes`, `environment_to_nodes`) are built
lazily via `build_indexes()` and cached. Query methods fall back to
linear scan if indexes aren't built.

### ~~4. Private construction APIs~~ ✅ Done

`Inventory::add_node()` and `add_class()` are now `pub` as of v0.13.0.

### ~~5. Fragmented error types~~ ✅ Done

Added `ferroclass::Error` at the crate root in v0.13.0, wrapping all
sub-module errors via `#[snafu(transparent)]`. Sub-module error types
remain accessible for granular matching.

### 6. CLI concerns mixed into library types (deferred)

`Options.verbose` is a CLI flag that doesn't belong in the library's
configuration type. `OutputOptions.no_refs` is accepted for CLI
compatibility but unused. This is low priority — the current API works
fine and breaking `Options` doesn't unlock new functionality.

## Improvement Plan

### Phase 0: API cleanup ✅ Done (v0.13.0)

- ✅ Make `add_node`/`add_class` public
- ✅ Decouple adapters from loading: `build_inventory_from()`, `build_host_vars_from()`, `build_top_from()`, `build_pillar_from()`
- ✅ Unify error types: `ferroclass::Error` at crate root with `#[snafu(transparent)]`
- ✅ Add `Diagnostic`, `DiagnosticSeverity`, `SourceLocation` types
- ✅ Change `node_names()` to return `impl Iterator<Item = &str>`
- ✅ Change `name()` on `Class` and `Node` to return `&str`
- ✅ Change `NodeBuilder`/`ClassBuilder` to consume `self` (builder pattern)
- ✅ Change setters to accept `impl Into<String>`
- ⏭️ Deferred: Split `Options` into library and CLI concerns (low priority)

### Phase 1: Thread safety ✅ Done (v0.13.0)

- ✅ Switch `Rc` → `Arc` in `Value` variants (~200 sites across 16 files)
- ✅ `MergeConfig::compiled_class_notfound_regexp` → `Arc<Mutex<Option<Regex>>>`
- ✅ Remove `#[pyclass(unsendable)]` from `PyInventory`/`PyNode`
- ✅ Verify `Inventory: Send + Sync` (static assertions pass)

### Phase 2: Error collection (collect-and-continue) ✅ In progress

- ✅ `Diagnostic`, `DiagnosticSeverity`, `SourceLocation` types (Phase 0, v0.13.0)
- ✅ `EntityState` enum: `Source`, `Merged`, `Interpolated`, `Failed` (Phase 2a)
- ✅ `Node::state()`, `Node::is_usable()`, `Node::diagnostics()`, `Node::add_diagnostic()`
- ✅ `Class::state()`, `Class::is_usable()`, `Class::diagnostics()`, `Class::add_diagnostic()`
- ✅ `Inventory::state()` — aggregate minimum across all nodes and classes
- ✅ `Inventory::diagnostics()` — inventory-level diagnostics
- ✅ `Inventory::all_diagnostics()` — combined inventory + entity summaries
- ✅ `Inventory::has_errors()` — checks both sources
- ✅ Per-entity diagnostic summaries with 0-or-1 invariant (`HashMap<String, Diagnostic>`)
- ✅ `merge_node()` returns `Ok(Failed)` for domain errors instead of `Err`
- ✅ `merge_class()` returns `Ok(Failed)` for domain errors instead of `Err`
- ✅ Diagnostic codes: `INV-001`, `INV-002`, `REF-001`, `MERGE-001`
- ⬜ `load()` collects per-file parse errors and continues (Phase 2a.4/2a.5)
- ⬜ Source location tracking in parser (Phase 0 prerequisite for LSP)
- ⬜ Warning-level diagnostics (duplicates, overrides, unused classes)
- ⬜ Incremental merge, progress callbacks, observability

### Phase 3: Merge Replay

The marquee feature for all interfaces. Expose the intermediate state of a
node at each step of the merge process, so users can click through the
inheritance chain and see how parameters evolve.

**Current state**: `merge_node_impl` in `src/inventory/merge.rs` builds a
`MergeAccumulator` step by step — one class at a time — but discards all
intermediate states. Only the final merged result is returned.

**Proposed API**:

```rust,ignore
/// A snapshot of the accumulator state after merging one step.
pub struct MergeStep {
    /// What kind of step produced this snapshot
    pub step_type: MergeStepType,
    /// The class or entity name that was merged in this step
    pub source_name: String,
    /// The accumulator state AFTER this step was applied
    pub parameters: ParametersType,
    pub exports: ParametersType,
    pub classes: Vec<String>,
    pub applications: Applications,
    pub environment: Environment,
}

pub enum MergeStepType {
    /// A class-mapping class was merged
    ClassMapping,
    /// Input data was merged
    InputData,
    /// Automatic parameters (_reclass_) were added
    AutomaticParameters,
    /// A node's declared class was merged
    NodeClass,
    /// A parent class within recurse_class was merged
    ParentClass { parent_of: String },
    /// The node's own fields were merged
    NodeFields,
}

/// Result of a merge replay, containing all intermediate steps
/// plus the final merged node.
pub struct MergeReplay {
    /// Each step in the merge, in order
    pub steps: Vec<MergeStep>,
    /// The fully merged and interpolated node
    pub final_node: Node,
    /// Pre-interpolation parameters (with ${...} references visible)
    pub pre_interpolation_parameters: ParametersType,
    /// Pre-interpolation exports (with ${...} references visible)
    pub pre_interpolation_exports: ParametersType,
}

impl Inventory {
    /// Like merge_node, but returns intermediate steps for replay.
    pub fn merge_node_with_replay(&self, name: &str)
        -> Result<MergeReplay, Error>;

    /// Like merge_node_with_inventory, but returns intermediate steps.
    pub fn merge_node_with_inventory_and_replay(&self, name: &str,
        inventory: &InventoryMap) -> Result<MergeReplay, Error>;
}
```

**Implementation**: Add an optional `&mut Vec<MergeStep>` collector to
`merge_node_impl`. After each `merge_descent_into` and
`merge_entity_fields` call, clone the accumulator and push a step.
Capture pre-interpolation state before Phase 6. This is low-risk because
`MergeAccumulator` is already `Clone`.

**Why this matters**: Merge replay is the single most useful feature for
understanding reclass-style inventories. Users select a node, see the
inheritance chain, and click through it step by step watching parameters
evolve. This transforms reclass from "black box that produces a result"
into "transparent pipeline you can inspect at every stage."

### Phase 4: Feature work

- Requirements/prerequisites validation
- See `docs/interfaces.md` for interface-specific features

## Roadmap: Interfaces

The three interfaces (LSP, MCP, Explorer) build on the library phases
above, but the order matters. From the analysis in `docs/interfaces.md`:

1. **LSP first** — Highest impact for the least effort. Meets users where
   they already work (their editor). No UI to build — the editor IS the
   UI. Diagnostics, go-to-definition, hover, and autocomplete transform
   the experience of editing inventory YAML. Depends on Phases 0-2.

2. **MCP second** — Straightforward JSON-RPC over stdio. Lower impact
   than LSP (agents are less enthusiastic than humans) but easy to build
   once the library API is clean. Depends on Phases 0-3.

3. **Explorer (TUI/Web) last** — Most work. Two renderers, REST API,
   WebSocket, file watching. The merge replay feature is the killer
   feature but requires significant UI investment. Depends on Phases 0-3.

```
Phase 0 (API cleanup + diagnostics) ✅ Done ──┐
Phase 1 (thread safety) ✅ Done                 ├── LSP v1
Phase 2 (query API + error collection) ✅ Partial ──┘
Phase 2a (collect-and-continue) ✅ In progress ──┘
Phase 3 (merge replay) ────────────────────────── MCP v1
Phase 4 (Explorer) ────────────────────────────── TUI + Web UI
```

## API Design Principles

These principles guide the new APIs we'll add for the LSP, MCP, and
Explorer interfaces. They're based on analyzing the current API surface
and identifying patterns that should change vs. patterns that are already
correct.

### Borrow when you can, own when you must

Rust gives us a choice between returning borrowed data (`&str`, `&[T]`,
`impl Iterator<Item = &T>`) and owned data (`String`, `Vec<T>`, `T`).
The right choice depends on the lifecycle:

- **Query methods on `&self`** should borrow, not allocate. The caller is
  just reading from the inventory; they shouldn't pay for cloning.

  ```rust,ignore
  // Done (v0.13.0): borrows, zero allocation
  pub fn node_names(&self) -> impl Iterator<Item = &str>
  ```

  This matters especially for the LSP, which calls `node_names()` on
  every keystroke for autocomplete.

- **Setters should accept flexible types.** `&str`, `String`, and
  `impl Into<String>` callers should all work without forced allocation:

  ```rust,ignore
  // Done (v0.13.0): borrows when possible, owns when needed
  pub fn set_uri(&mut self, uri: impl Into<String>)
  ```

- **Return `&str` instead of `&String`.** `&String` exposes the interior
  type and doesn't coerce as nicely. `&str` is the idiomatic Rust choice:

  ```rust,ignore
  // Done (v0.13.0): idiomatic, coerces to &str, &String, etc.
  pub fn name(&self) -> &str
  ```

- **Constructors and merge methods should return owned types.** This is
  already correct in the current API:

  ```rust,ignore
  // Already correct: merge creates new data, can't return a reference
  pub fn merge_node(&self, node_name: &str) -> Result<Node, Error>
  pub fn load(options: &StorageOptions) -> Result<Inventory, Error>
  ```

### Consume builders, don't clone them ✅

The builder pattern in Rust should consume `self`, not borrow `&mut self`.
This was fixed in v0.13.0 — `NodeBuilder` and `ClassBuilder` now take
`self` instead of `&mut self`:

```rust,ignore
// Done (v0.13.0): self means build() takes ownership, zero clones
let node = Node::builder("web01".to_string())
    .classes(vec!["web".to_string()])
    .build();  // moves fields, zero clones
```

**Exception**: `MergeConfig` already uses the consuming pattern (`self`),
which is correct. The builder methods return `Self` and can be chained:

```rust,ignore
let config = MergeConfig::new()
    .value_override_prefix("~")  // consumes self, returns Self
    .automatic_parameters(true)    // consumes self, returns Self
    .compile_regexps();            // &mut self, mutates in place
```

### No multi-variant methods

Some Rust crates provide three versions of each method:

```rust,ignore
fn foo(&self) -> &T          // borrow
fn foo_mut(&mut self) -> &mut T  // mutable borrow
fn into_foo(self) -> T         // consume
```

This pattern makes sense for collection types (`Vec::iter()`,
`Vec::iter_mut()`, `Vec::into_iter()`), but not for ferroclass.
The reason: `Inventory` has a clear ownership model.

- **Load once, query many times**: `Inventory` is loaded once and then
  read-only for queries. `&self` receivers are correct. There's no use
  case for `&mut self` or `self` on `Inventory` (except `set_merge_config`).

- **Merge produces new data**: `merge_node()` returns an owned `Node`
  because it constructs a new merged result. You can't return a borrowed
  reference because the data didn't exist before the merge. No need for
  a `&self` variant.

- **`Node` and `Class` are results, not containers**: You either borrow
  them (to read) or own them (to serialize/transform). You don't need
  three variants.

**Principle**: provide the simplest signature that covers the common
case. Only add variants when there's a demonstrated performance or
ergonomics need.

### ~~The Rc → Arc change is internal and invisible~~ ✅ Done

Switching `Rc` to `Arc` in `Value` was completed in v0.13.0. It doesn't
change the public API surface — callers never construct `Value` directly,
they get it from the library. The change adds `Send + Sync` guarantees
invisibly.

The scope was mechanical but wide: ~200 sites across 16 files. `Regex`
in `MergeConfig` was wrapped in `Arc<Mutex<Option<Regex>>>` because
`regex::Regex` is `!Send + !Sync`.

The performance cost of `Arc` vs `Rc` is negligible for ferroclass:
atomic increments cost ~5-10 ns vs ~1 ns for plain increments, but the
merge/interpolation operations each take milliseconds to seconds. The
atomic overhead is unmeasurable in practice.

### Diagnostic-returning APIs

The library now uses a collect-and-continue model for domain errors:

```rust,ignore
// Domain errors return Ok(Failed) — the node exists with diagnostics
let node = inventory.merge_node("web01")?;
if !node.is_usable() {
    for diag in node.diagnostics() {
        eprintln!("{}: {}", diag.severity, diag.message);
    }
    return Err(...);
}

// Implementation errors (I/O, bugs) still return Err
let inventory = ferroclass::load(&options)?;  // RepositoryError
```

The `EntityState` enum tracks how far processing progressed:

```rust,ignore
pub enum EntityState {
    Source,        // Parsed from YAML, no merging applied
    Merged,        // Class inheritance resolved, not yet interpolated
    Interpolated,  // Fully processed (default for merge_node())
    Failed,        // Processing failed; data is NOT trustworthy
}
```

`Inventory::state()` returns the aggregate minimum across all nodes and
classes. An empty inventory defaults to `Interpolated`. If any entity is
`Failed`, the inventory is `Failed`.

Per-entity diagnostic summaries are stored in a `HashMap<String, Diagnostic>`
keyed by entity name, ensuring a 0-or-1 invariant: there is never more
than one inventory-level diagnostic per entity. When a node or class is
re-added (fixed), the old diagnostic is automatically removed.

### Iterator-based query APIs

New query methods should return iterators, not vectors:

```rust,ignore
// Preferred: borrows, lazy, zero allocation
pub fn find_nodes_by_class(&self, class: &str) -> impl Iterator<Item = &Node>
pub fn find_nodes_by_environment(&self, env: &Environment) -> impl Iterator<Item = &Node>
pub fn search_nodes(&self, pattern: &str) -> impl Iterator<Item = &Node>
```

This avoids allocation when the caller just wants to iterate or take the
first N results. If the caller needs a `Vec`, they can `.collect()`.

### Current API Issues Summary

| Issue                               | Current                              | Preferred                              | Status                   |
| ----------------------------------- | ------------------------------------ | -------------------------------------- | ------------------------ |
| `node_names()` allocates           | ~~`Vec<String>`~~ `impl Iterator<Item = &str>` | ✅ Done | LSP autocomplete perf     |
| `name()` returns `&String`          | ~~`&String`~~ `&str`                        | ✅ Done | Idiomatic Rust            |
| `NodeBuilder`/`ClassBuilder` clone  | ~~`&mut self` on build~~ `self` on build     | ✅ Done | Avoids forced clones      |
| Setters require owned `String`     | ~~`set_uri(String)`~~ `set_uri(impl Into<String>)` | ✅ Done | Caller ergonomics |
| `StorageOptionsTrait` clones       | `fn parameter_key_style() -> ParameterKeyStyle` | `fn parameter_key_style() -> &ParameterKeyStyle` | Nit, deferred |
| `merge_node()` aborts on error     | ~~`Result<Node, Error>`~~ `Result<Node, Error>` (domain errors → `Ok(Failed)`) | ✅ Done (Phase 2a) | LSP shows all problems at once |
| `Value` uses `Rc`                   | ~~`!Send + !Sync`~~ `Arc` → `Send + Sync`    | ✅ Done | Thread safety             |

### Caching and indexing — optional, not in the library core

The CLI does not need caching. It loads an inventory, merges nodes,
outputs results, and exits. Caching would add complexity with no benefit
for this workflow.

Caching and indexing are concerns **only for long-running processes**
(LSP, web server, MCP server) that query the same inventory repeatedly.
The library should provide the building blocks, but caching must remain
optional and never be required for basic use.

#### What we don't need

**No external crate dependencies for caching.** The ferroclass data model
(structured key-value hierarchies) doesn't map well to generic solutions:

- **Embedded databases** (sled, redb, SQLite via rusqlite) — These are
  designed for unstructured or flat key-value data. Our data is a
  `LinkedHashMap<String, Node>` with recursive `Value` trees. Serializing
  to bytes and deserializing on startup is probably similar speed to just
  re-parsing the YAML files. Not worth the dependency or complexity.

- **Full-text search engines** (tantivy, meilisearch) — These index
  unstructured text for fuzzy matching. Our data is structured: we know
  every parameter key and value. A simple `HashMap` reverse index gives
  exact-match queries in nanoseconds without a search engine.

- **Cache libraries** (moka, cached, lru) — These add generational
  eviction policies and TTL logic that we don't need. Our invalidation is
  simple: when a file changes on disk, reload. A `HashMap` behind
  `Arc<RwLock<>>` (after the Rc→Arc migration) is simpler and sufficient.

#### What we do need — simple data structures in `Inventory`

These are all plain `HashMap`-based indexes that can be built during
`load()` or `build_inventory_map()`. No crate required:

```rust,ignore
pub struct Inventory {
    // ... existing fields ...

    // --- Caching (optional, only for long-running processes) ---

    /// Cache of merged nodes. Built lazily or on demand.
    /// Cleared when source files change.
    /// Not used by the CLI.
    merged_nodes: HashMap<String, Node>,

    /// Reverse index: class name → node names that include it.
    /// Used for "which nodes depend on this class?" queries.
    /// Built during load() or build_index().
    class_to_nodes: HashMap<String, Vec<String>>,

    /// Reverse index: parameter key → node names that have this key.
    /// Used for "find all nodes with parameter X" queries.
    /// Built during load() or build_index().
    param_key_to_nodes: HashMap<String, Vec<String>>,
}
```

**The CLI never touches these fields.** They're built on demand by the
LSP, MCP, or Explorer when they call `build_index()` or query methods
that trigger lazy construction.

#### Incremental updates — the real performance win

For the LSP, the key optimization isn't caching — it's **not reloading
everything when one file changes**. The pattern is:

1. File change detected by `notify` crate
2. Re-parse only the changed YAML file
3. Update the affected `Class` or `Node` in the `Inventory`
4. Re-merge only the nodes that depend on the changed class (using
   `class_to_nodes` index)
5. Push updated diagnostics

This requires `Inventory` to support updating a single class or node
without reloading from scratch — which means making `add_node()` and
`add_class()` public (already planned in Phase 0).

#### When to consider external crates

If someone has a 10,000+ node inventory where load time becomes seconds
instead of milliseconds, then consider:

- **sled or redb** for persistent storage of parsed YAML (skip parsing
  on cold start) — but only if profiling shows parsing is the bottleneck
- **tantivy** for full-text search across parameter values — but only
  if the `HashMap` reverse index isn't sufficient for some use case

For v1, plain `HashMap` indexes are simpler, faster to implement, and
easier to debug. Add crate dependencies only when profiling proves they're
needed.

#### Principle: caching is a layer above the library

The library provides `Inventory` with optional index fields. The caching
logic lives in the interface layer (LSP, web server, MCP), not in the
library itself:

```
┌─────────────────────────────────────────────┐
│  LSP / Web / MCP (caching layer)            │
│                                              │
│  Arc<RwLock<Inventory>>                      │
│  ┌────────────────────────────────────────┐  │
│  │  load() → Inventory                    │  │
│  │  file change → reload() or update()    │  │
│  │  query → merged_nodes cache (hit/miss) │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  CLI (no caching)                            │
│  ┌────────────────────────────────────────┐  │
│  │  load() → merge_node() → output → exit │  │
│  └────────────────────────────────────────┘  │
└─────────────────────────────────────────────┘
```

The CLI path stays exactly as it is. The caching layer wraps `Inventory`
and adds `Arc<RwLock<>>`, index building, and cache invalidation. The
library doesn't know about caching — it just provides the data structures
and merge operations.