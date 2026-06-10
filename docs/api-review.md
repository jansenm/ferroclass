<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# API Review: Current State and Improvement Plan

This document assesses the current ferroclass library API surface and
identifies changes needed to support external interfaces (TUI, web, MCP).

## Current Public API

### Crate Root Re-exports (`src/lib.rs`)

```rust
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

### 1. `Rc<Value>` blocks thread safety

`Value::Hash` and `Value::Array` use `Rc<Hash>` and `Rc<Array>`.
`Rc` is `!Send + !Sync`, so `Inventory`, `Node`, `Class`, and all their
contents cannot cross thread boundaries. This blocks async runtimes, web
servers, and any concurrent access.

**Fix**: Switch `Rc` → `Arc` throughout `Value`. Mechanical but
wide-reaching. PyO3's `#[pyclass(unsendable)]` annotation would need to
stay (or be removed once `Arc` makes `Value` `Send + Sync`).

### 2. Adapter builders are coupled to loading

`build_inventory()`, `build_top()`, `build_pillar()` all call
`Inventory::load()` internally. You cannot pass a pre-loaded inventory
to them. Every interface that wants to use an adapter must either
re-load from disk or duplicate the adapter logic.

**Fix**: Add `build_inventory_from(inventory: &Inventory, opts)` variants
that accept a pre-loaded inventory. Keep the old loading-based functions
as convenience wrappers.

### 3. No query or filter API

You can only get a node by exact name or iterate all of them. No
filtering by class membership, environment, or pattern.

**Fix**: Add `find_nodes_by_class()`, `find_nodes_by_environment()`,
`search_nodes(pattern)` with indexing structures.

### 4. Private construction APIs

`Inventory::add_node()` and `add_class()` are private. External code
cannot build inventories programmatically (e.g., from a REST payload
or test fixture).

**Fix**: Make them `pub`, or add an `InventoryBuilder`.

### 5. Fragmented error types

Separate error types per layer: `inventory::Error`, `merge::Error`,
`interpolation::Error`, `value_merge::Error`, `file_system::Error`,
plus adapter-specific errors. No single top-level error type.

**Fix**: Introduce `ferroclass::Error` wrapping all sub-errors, re-exported
from the crate root. Keep sub-errors accessible via `#[snafu(source)]`.

### 6. CLI concerns mixed into library types

`Options.verbose` is a CLI flag that doesn't belong in the library's
configuration type. `OutputOptions.no_refs` is accepted for CLI
compatibility but unused.

**Fix**: Split `Options` into `LibraryOptions` (for programmatic use) and
`CliOptions` (which adds CLI-specific fields). The CLI layer wraps
`LibraryOptions`.

## Improvement Plan

### Phase 0: API cleanup

Low-risk changes that unblock everything else:

- Make `add_node`/`add_class` public (or add `InventoryBuilder`)
- Decouple adapters from loading: add `_from(inventory, opts)` variants
- Unify error types: single `ferroclass::Error` at crate root
- Separate `Options` into library and CLI concerns
- Add `Diagnostic` and `DiagnosticSeverity` types (Error/Warning/Info/Hint)
- Add `SourceLocation { file, line, col }` to `Class` and `Node` during parsing
- Change `load()` return type to collect per-file/per-node errors instead of
  aborting on first error: `Result<DiagnosticReport, FatalError>` where
  `DiagnosticReport { diagnostics: Vec<Diagnostic>, inventory: Option<Inventory> }`
- Change `merge_node()` to return `MergeResult` with diagnostics alongside
  the merged node, so missing classes produce warnings instead of aborting

### Phase 1: Thread safety

- Switch `Rc` → `Arc` in `Value` variants
- Remove `#[pyclass(unsendable)]` from `PyInventory`/`PyNode`
- Verify `Inventory: Send + Sync`

### Phase 2: Query API

- Node search/filter: by class, environment, pattern
- Incremental merge: don't force full `build_inventory_map()` upfront
- Progress callbacks or tracing spans for observability

### Phase 3: Merge Replay

The marquee feature for all interfaces. Expose the intermediate state of a
node at each step of the merge process, so users can click through the
inheritance chain and see how parameters evolve.

**Current state**: `merge_node_impl` in `src/inventory/merge.rs` builds a
`MergeAccumulator` step by step — one class at a time — but discards all
intermediate states. Only the final merged result is returned.

**Proposed API**:

```rust
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
- MCP server, TUI, web interface (see `docs/interfaces.md`)