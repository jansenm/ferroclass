# AGENTS.md

This file contains essential information for agentic coding agents working in this Rust inventory
management project.

## Project Overview

This is a Rust reimplementation of [reclass](https://reclass.pantsfullofunix.net/),
specifically the [salt-formulas/reclass](https://github.com/salt-formulas/reclass) fork.
The goal is **100% compatibility** with the Python reclass implementation. Any
deviation from Python reclass behavior is a bug and must be documented as a known
incompatibility.

The program functionality is described in [README.md](README.md) (concepts, rules, configuration,
and compatibility) and [docs/process.md](docs/process.md) (detailed processing pipeline).

## Licensing

This project is licensed under the **Mozilla Public License 2.0 (MPL-2.0)**.

- **SPDX License Identifier**: `MPL-2.0`
- **Copyright Holder**: Michael Jansen `<mike@michael-jansen.biz>`
- **License text**: `LICENSES/MPL-2.0.txt`

## Documentation Structure

| File                 | Audience      | Content                                           |
|----------------------|---------------|---------------------------------------------------|
| `README.md`          | Users         | Concepts, rules, configuration, compatibility     |
| `docs/process.md`    | Users         | Detailed processing pipeline (deep-dive)          |
| `docs/conventions.md`| Developers    | Error handling, snafu patterns, development rules |
| `CHANGELOG.md`       | Users         | Release history                                   |
| `CONTRIBUTING.md`    | Contributors  | Build, test, licensing, release process           |
| `docs/todo.md`       | Users         | Planned features and roadmap                      |
| `AGENTS.md`          | AI/Developers | Architecture rules, code style, development guide, agent workflow |

## Agent Workflow

This repository uses a multi-agent workflow for development. Agent definitions are
under `.opencode/agents/` and skills are under `.agents/skills/`.

### plan (lead planner)

The lead planner and coordinator. Analyzes requirements, designs changes, and delegates
to specialized agents. Delegates Rust research questions to `@analyze-rust`, Python
reference questions to `@analyze-python`, and implementation to `@build`.
Does NOT write code — only plans and coordinates.

### build (implementer)

The built-in Build agent, customized with project-specific instructions. Implements
changes based on plans from the `plan` agent. Makes precise code changes, verifies with
`cargo build`/`cargo test`/`cargo clippy`, and reports back. Does NOT plan or make
design decisions — it executes plans. Can delegate Python behavior questions to
`@analyze-python`.

### analyze-rust (Rust analyst)

Researches and analyzes the Rust codebase. Answers questions about architecture,
behavior, and code with precise `file_path:line_number` references. Does NOT plan
changes or coordinate implementation.

### analyze-python (reference analyst)

Read-only agent that analyzes the Python reclass codebase. Invoked by the `plan` or
`build` agents when Python behavior needs to be understood.

Python reference code is expected at `references/reclass/` (clone via `make setup-reclass`)
or `~/Projects/python/reclass/` as fallback.

### build-packaging (packaging specialist)

Subagent that analyzes and modifies RPM packaging configuration (spec files, Makefiles,
changelogs, OBS integration). Invoked by the `plan` or `build` agents for packaging tasks.
Has access to the `rust-rpm-packaging` skill for SUSE/RHEL patterns.

## Python Reference

The Python reclass codebase is the reference implementation for ferroclass. To enable
Python reference analysis, clone reclass into the repo:

```shell
make setup-reclass    # Clones https://github.com/salt-formulas/reclass to references/reclass/
```

Once cloned, the `analyze-python` agent can read Python source code and answer questions
about Python reclass behavior. The definitive documentation for reclass internals is
at `references/reclass/reclass/overview.md`.

Key Python modules for quick reference:

- `references/reclass/reclass/core.py` — Core merging and interpolation logic
- `references/reclass/reclass/config.py` — Configuration handling
- `references/reclass/reclass/defaults.py` — Default configuration values
- `references/reclass/reclass/storage/` — Storage backends (directory, yaml, etc.)

## REUSE Compliance

This project follows the [REUSE Specification](https://reuse.software/spec/) version 3.3
to make licensing and copyright information machine-readable.

### How It Works

- Every source file (`.rs`, etc.) has an SPDX header at the top:
  ```rust
  // SPDX-FileCopyrightText: 2025 Michael Jansen <mike@michael-jansen.biz>
  // SPDX-License-Identifier: MPL-2.0
  ```
- Cover non-source files (Cargo.toml, snapshots, etc.) with a wildcard
  rule in `REUSE.toml`.
- The full license text lives in `LICENSES/MPL-2.0.txt`.

### When Adding New Files

All new `.rs` files **must** include the two-line SPDX header as the first lines
of the file. Files without it will cause `reuse lint` to fail.

### Verification

Run `reuse lint` from the project root to verify compliance. The project must
show zero errors and zero warnings.

### Known Incompatibilities

- **YAML 1.1 vs YAML 1.2**: The Python reclass uses PyYAML, which follows YAML 1.1
  (e.g., `yes`/`no`/`on`/`off` are booleans). Our Rust implementation uses
  `yaml-rust2` which follows YAML 1.2, where these are plain strings. This affects
  how boolean-like values are parsed from inventory files.

## Build/Test Commands

### Core Commands

- `cargo build` - Build the project
- `cargo test` - Run all tests
- `cargo test <test_name>` - Run a specific test
- `cargo clean` - Clean build artifacts
- `cargo run --bin reclass -- <args>` - Run the main binary

### Single Test Examples

```bash
cargo test test_repository_init
cargo test empty_document
cargo test test_default_options
```

### Coverage and Quality

- `cargo tarpaulin` - Generate a coverage report
- `cargo tarpaulin --out html` - Generate an HTML coverage report
- `cargo clippy` - Run lints
- `cargo fmt` - Format code
- `make commit` - Run full commit checks (format, test, clippy, check-manpages)

### Development Workflow

- Use `make` for standard operations (build, test, clean, commit)
- The main binary is located at `target/debug/reclass`
- Tests are located in `tests/` directory and inline in modules

## Code Style Guidelines

### Project Structure

- **Library code**: `src/` with modular organization
- **Binary**: `src/bin/reclass/` - main CLI application
- **Tests**: `tests/` for integration tests, inline unit tests in modules
- **Storage layer**: `src/storage/file_system/` for file system operations

### Error Handling

- **Error type**: Use `snafu` for error handling throughout the codebase
- **Pattern**: `#[derive(Debug, Snafu)]` for error enums
- **Context**: Use `.context()` method to add context to errors
- **Visibility**: Use `#[snafu(visibility(pub(in super::module)))]` for module-level visibility
- **Full conventions**: See [docs/conventions.md](docs/conventions.md) for funnel principle,
  display rules, source chain discipline, pass-through elimination, and transparent variants

### Import Organization

```rust
// Standard library imports first
use std::path::{Path, PathBuf};
use std::{fs, path};

// External crates next
use clap::Parser;
use snafu::prelude::*;
use yaml_rust2::YamlLoader;

// Internal imports last (use crate:: prefix)
use crate::inventory::elements::{class_parser, node_parser};
use crate::parser::yaml::{Parser, YamlParser};
```

### Naming Conventions

- **Types**: `PascalCase` for structs, enums, traits
- **Functions/Methods**: `snake_case`
- **Constants**: `SCREAMING_SNAKE_CASE`
- **Modules**: `snake_case` for files/directories
- **Error variants**: `PascalCase` with descriptive names

### Module Organization

- **Public API**: Export types with `pub use` for clean interface
- **Private modules**: Use `pub(crate)` for internal-only modules
- **Module declarations**: Group-related functionality in submodules
- **Re-exports**: Use `pub use` to create clean public interfaces

### Testing Patterns

- **Unit tests**: Use `#[cfg(test)]` modules within source files
- **Integration tests**: Place in `tests/` directory
- **Test naming**: Use descriptive test names with `test_` prefix
- **Assertions**: Use standard `assert_eq!`, `assert!`, `expect()` methods

### Code Organization

- **Builder pattern**: Use for complex struct construction (see `ClassBuilder`)
- **Traits**: Define behaviors with `trait` declarations
- **Iterator pattern**: Implement custom iterators for data traversal
- **Option/Result**: Use `Option<T>` and `Result<T, E>` for error handling

### Logging and Debugging

- **Logging**: Use `tracing` crate with the appropriate level macros
- **Debug**: Use `#[derive(Debug)]` for all public types
- **Debug output**: Use `dbg!()` for temporary debugging (remove before commit)

### Configuration and Options

- **CLI parsing**: Use `clap` with derive macros
- **Configuration**: Separate config types in `configuration.rs`
- **Options module**: Group all option types in `inventory/options/`

### Memory and Performance

- **Ownership**: Follow Rust ownership principles carefully
- **Borrowing**: Prefer borrowing to cloning when possible
- **String handling**: Use `String` for owned data, `&str` for borrowed
- **Collections**: Use appropriate collection types (`Vec`, `HashMap`, etc.)

## Key Dependencies

### Core Dependencies

- `clap` - Command line argument parsing
- `snafu` - Error handling
- `serde` + `serde_yaml` - Serialization/deserialization
- `yaml-rust2` - YAML parsing
- `tracing` + `tracing-subscriber` - Logging
- `walkdir` - File system traversal
- `hashlink` - Linked hash maps for ordered data

### Development Tools

- `rustfmt` - Code formatting (included in toolchain)
- `clippy` - Linting (included in toolchain)
- `rust-analyzer` - IDE support (included in toolchain)

## File Naming and Paths

### Source Organization

- Module files: `snake_case.rs` (e.g., `class_parser.rs`)
- Module directories: `snake_case/mod.rs` structure
- Tests: `tests/` directory for integration tests
- Examples: Use `examples/` directory if needed

### Path Handling

- Use `PathBuf` for owned paths
- Use `&Path` for borrowed path references
- Use `path::absolute()` for canonical paths
- Handle path errors gracefully with proper context

## Testing Strategy

### Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::other_module;

    #[test]
    fn test_descriptive_name() {
        // Arrange
        let input = setup_test_data();

        // Act
        let result = function_under_test(&input);

        // Assert
        assert_eq!(result.expected, result.actual);
    }
}
```

### Coverage Goals

- Aim for high test coverage on core business logic
- Test error paths and edge cases
- Use property-based testing for complex data transformations
- Include integration tests for major workflows

## Development Guidelines

### When Making Changes

1. Run `cargo test` to ensure existing tests pass
2. Add or update tests for new functionality
3. Run `cargo clippy` to check for lints
4. Run `cargo fmt` to format code
5. Test manually with the CLI binary if applicable

### Code Review Checklist

- Error handling is comprehensive and idiomatic
- Public API is well-documented and stable
- Tests cover the main functionality and edge cases
- Code follows established naming conventions
- Imports are properly organized
- No `dbg!()` statements left in code
- Logging levels are appropriate

### Architecture Rules

1. **Favor library code over binary code.** Shared logic (output formatting,
   serialization, node metadata structs) belongs in `src/`, not copy-pasted
   across binaries. If two `bin/` programs need the same function, extract it
   to the library crate.

2. **No duplicated functions across binaries.** Identical or near-identical
   functions in separate `bin/` directories are forbidden. Factor the common
   code into `src/` and import it.

3. **Single serialization path per format per data type.** JSON output uses
   `Serialize`. YAML output uses `to_yaml_value()`. These are two different
   formats, so having both is acceptable — but each format has exactly one
   path. No external converter function (like `json_value_to_yaml`) that
   manually destructs the type into an intermediate representation.

4. **Configuration defaults are defined once.** Default values for output
   format, pretty-printing, sorting, etc. live in the library's options
   module. Binaries inherit from the library and never redefine defaults
   independently.

5. **Consistent output pipeline across all binaries.** All binaries use the
   same formatting infrastructure for JSON/YAML, pretty/sorted. The
   `reclass` binary is the reference; `reclass-ansible` and `reclass-salt`
   must share the same code path, not re-implement it.

6. **Domain types serialize themselves.** `Value`, `Key`, and other domain
   types own their serialization logic via `Serialize` impls (for JSON) and
   `to_yaml_value()` methods (for YAML). No external converter function
   (like `json_value_to_yaml`) that manually destructs the type.

7. **Separate data construction from formatting.** Build a domain object
   first, then format it for output. Never mix business logic (merging,
   interpolation, inventory queries) with serialization concerns.

8. **Three-layer architecture.** The codebase is structured in three layers:
    - **Core library** (`inventory` crate) — produces plain Rust types
      (`Node`, `Class`, `Value`, `InventoryMap`). No output formatting.
    - **Adapter layer** (`output` module) — transforms core types into
      adapter-specific output types (`AnsibleInventory`, `InventoryOutput`,
      `TopData`), enriching with metadata (`__reclass__`, `timestamp`, etc.).
    - **Format layer** (`output/mod.rs`) — takes any output type and produces
      the final serialized form: JSON via `Serialize`, YAML via
      `to_yaml_value()`.

9. **Binaries are thin CLI wrappers.** All domain logic — inventory
   construction, output types, serialization, formatting — lives in
   `src/`. Binaries (`bin/reclass/`, `bin/reclass-ansible/`,
   `bin/reclass-salt/`) only parse CLI arguments, call into the library,
   and print output. This is necessary because the library will be exposed
   via FFI (PyO3 for Python, Rustler for Elixir) — adapters must be able
   to call the same functions the binaries call, without duplicating
   logic. Adapter-specific output types (e.g., `AnsibleInventory`) belong
   in `src/output/`, not in `bin/`.

### Performance Considerations

- Avoid unnecessary allocations in hot paths
- Use iterators for processing collections
- Profile with appropriate tools if performance issues arise
- Consider memory usage for large inventory datasets

This file should be updated as the project evolves and new patterns emerge.

## Performance Profiling

### Profiling Commands

To profile the inventory command and identify performance bottlenecks:

```bash
# Build release version with debug symbols for profiling
cargo build --release

# Time the command for baseline measurement
time ./target/release/reclass --inventory --output json --inventory-base-uri $PROJECT_DIR/inventories/e2e/

# Profile with perf (requires root or proper permissions)
perf record -F 99 -g --call-graph dwarf -o /tmp/reclass.perf.data \
  -- ./target/release/reclass --inventory --output json --inventory-base-uri $PROJECT_DIR/inventories/e2e/

# View perf report
perf report -i /tmp/reclass.perf.data --stdio --no-children | head -200
```

### Using cargo-flamegraph

```bash
# Generate flamegraph (may require sudo for perf access)
cargo flamegraph --root -o /tmp/flamegraph.svg --bin reclass -- \
  --inventory --output json --inventory-base-uri $PROJECT_DIR/inventories/e2e/
```

### Test Inventories

For performance and stability testing use the built-in test inventories:

- inventories/e2e (small, used by integration tests)
- inventories/example (small, used by integration tests)
- inventories/python_compat (used for Python compatibility tests)
