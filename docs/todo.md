<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Roadmap

## Planned Features

- **Git storage type (`yaml_git`)** — Read nodes and classes from a remote Git repository, with
  branch-per-environment support, SSH key configuration, and caching
- **`propagate_pillar_data_to_reclass` for Salt** — Forward existing pillar data into the
  reclass rendering pipeline so classes and parameters can reference pillar values via
  interpolation. Requires PyO3 bindings (available since 0.11.1). Deferred to v2.
- **CLI input data flags** — `--input-data` and `--input-data-file` flags for `reclass-salt` for
  standalone use with JSON/YAML input data
- **End-to-end integration tests** — Comprehensive tests against real inventory files, including
  Python compatibility comparison
- **aarch64 support** — Enable aarch64 RPM builds after fixing snapshot test determinism
  (HashMap ordering differences in peers map)
- **DEB packaging** — Debian/Ubuntu packages
- **CI pipeline** — GitHub Actions for build, test, clippy, coverage
- **YAML output optimization** — Avoid intermediate Yaml tree conversion in serialization
- **Interpolation failure → Merged state** — When interpolation fails, return `Node { state: Merged }`
  with the merged data still intact and a diagnostic about unresolvable refs, instead of `Failed`
  with empty data. Useful for LSP "show merged data with broken refs highlighted." Blocked on
  refactoring the merge pipeline to produce intermediate results.

## Completed

- **Wildcard/regexp class mappings** — Glob and regex patterns matching node names
  (see [reclass operations docs](https://reclass.pantsfullofunix.net/operations.html#wildcard-regexp-mappings)).
  Available since 0.9.0 (CLI) and 0.11.1 (Python API).
- **PyO3 Python bindings** — Native Python extension module built with PyO3 0.23.
  Provides `ext_pillar()`, `top()`, and `load()` for Salt integration. Available
  since 0.11.1.
- **Thread safety (Rc → Arc)** — All `Rc<LinkedHashMap>` and `Rc<Array>` in `Value`
  replaced with `Arc` counterparts. `Inventory`, `Class`, `Node`, `Value`, and
  `MergeConfig` are now `Send + Sync`. PyO3 types no longer need `unsendable`.
  Available since 0.13.0.
- **Public construction APIs** — `Inventory::add_node()` and `add_class()` are now
  public, enabling programmatic inventory construction without the file system
  loader. Available since 0.13.0.
- **Adapter decoupling** — `build_inventory_from()`, `build_host_vars_from()`,
  `build_top_from()`, `build_pillar_from()` accept pre-loaded `&Inventory`
  instead of loading from disk. Available since 0.13.0.
- **Unified error type** — `ferroclass::Error` at crate root wrapping all sub-module
  errors via `#[snafu(transparent)]`. Available since 0.13.0.
- **Diagnostic types** — `Diagnostic`, `DiagnosticSeverity`, `SourceLocation` for
  structured error/warning/info/hint reporting. Foundation for Phase 2's
  collect-and-continue error handling. Available since 0.13.0.
- **Builder pattern consumes self** — `NodeBuilder` and `ClassBuilder` take `self`
  instead of `&mut self`, eliminating forced clones on `build()`. Available
  since 0.13.0.
- **Ergonomic API improvements** — `node_names()` returns `impl Iterator<Item = &str>`,
  `Class::name()` and `Node::name()` return `&str`, setters accept `impl Into<String>`.
  Available since 0.13.0.
- **Query API** — `find_nodes_by_class()`, `find_nodes_by_resolved_class()`,
  `find_nodes_by_environment()`, `search_nodes()`, `class_names()`. Reverse
  indexes (`class_to_nodes`, `environment_to_nodes`) built lazily. Available
  since 0.13.0.
- **EntityState and collect-and-continue for merge** — `EntityState` enum with
  four pipeline stages (`Source`, `Merged`, `Interpolated`, `Failed`). Domain
  errors (missing class, interpolation failure, type conflicts) now produce
  `Ok(Failed)` nodes/classes with diagnostics instead of `Err`. Per-entity
  diagnostic summaries with 0-or-1 invariant via `HashMap<String, Diagnostic>`.
  `Inventory::state()` returns aggregate minimum. Available since 0.14.0.
- **Collect-and-continue for loading** — `load_with_diagnostics()` returns
  `LoadResult` with both `Inventory` and `Vec<Diagnostic>`. Per-file parse
  errors are collected as diagnostics instead of aborting. `add_node()` now
  accepts duplicate names as warnings (INV-003) instead of returning `Err`.
  `load()` preserved for backward compatibility. Diagnostic codes: PARSE-001
  (single-file parse error), PARSE-002 (class load error), PARSE-003 (node
  load error), INV-003 (duplicate node name).

## Deferred Indefinitely

- **Mixed storage type** — No known users. Will revisit if demand emerges.
- **`scalar_reclass_parameters`** — Zero test coverage, zero documentation, no known users in the
  wild. Will not be implemented.
