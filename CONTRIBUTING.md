<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Contributing to Ferroclass

Thank you for your interest in contributing! This guide covers the essentials.

## Building

```shell
cargo build                # Debug build
cargo build --release      # Release build
```

## Testing

```shell
cargo test                 # Run all tests
cargo test <test_name>     # Run a specific test
```

### Test Inventories

The repository contains three test fixture inventories under `inventories/`:

- **`example/`** and **`example_file/`** — Minimal "hello world" inventories used by
  unit tests for `inventory_merge`, `inventory_yaml_fs`, and `inventory_yaml_file`.
  They are intentionally simple and demonstrate the same logical data in both
  directory-based and single-file storage formats. They also serve as human-readable
  minimal examples (referenced from [README.md](README.md)).

- **`e2e/`** — Comprehensive end-to-end inventory used by `tests/e2e.rs`. This is where
  complex features (interpolation, exports, inventory queries, environments, class
  name interpolation, etc.) are tested. **When adding new integration tests that need
  inventory data, extend `e2e/` rather than `example/`** to keep the minimal examples
  clean and easy to understand.

- **`python_compat/`** — Compatibility inventory matching the upstream Python reclass
  test fixtures. Used by snapshot and e2e tests to verify behavioral parity.

A full-featured showcase inventory for documentation purposes is planned for a future
release and will live under `inventories/showcase/`.

## Code Quality

Before pushing changes, run:

```shell
make commit                # Runs: format, test, clippy, check-manpages
```

Or individually:

```shell
cargo fmt                  # Format code
cargo clippy               # Lint
make check-manpages        # Verify man pages are up to date
```

## Architecture

See [AGENTS.md](AGENTS.md) for architecture rules, code style guidelines,
error handling patterns, and module organization.

## Licensing

This project is licensed under the [Mozilla Public License 2.0](LICENSES/MPL-2.0.txt).

All new `.rs` files must include SPDX headers:

```rust
// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0
```

Verify compliance with [REUSE](https://reuse.software/):

```shell
reuse lint
```

## Man Pages

Man pages are generated from CLI definitions and committed to `man/`. If you change
CLI arguments, regenerate them:

```shell
make manpages              # Generate man pages
make check-manpages        # Verify they're committed
```

## Agentic Development

This project uses [OpenCode](https://opencode.ai/) agentic coding agents to assist with
planning, analysis, and implementation. Agent definitions live in `.opencode/agents/`
and skills (reusable knowledge) live in `.agents/skills/`.

### Available Agents

| Agent | Role |
|-------|------|
| `@plan` | Lead planner — analyzes requirements and delegates to other agents |
| `@build` | Implements Rust code changes, runs tests, reports results |
| `@analyze-rust` | Read-only Rust codebase analysis (architecture, types, code) |
| `@analyze-python` | Read-only analysis of the Python reclass reference codebase |
| `@build-packaging` | RPM packaging specialist (spec files, Makefiles, changelogs) |

Skills are loaded on demand via the `skill` tool. Available skills include:
- **`gnu-makefile`** — GNU Make conventions for Rust projects
- **`rust-rpm-packaging`** — Packaging Rust projects as RPMs (SUSE/RHEL)
- **`ferroclass-release`** — Release process checklist (see [Making a Release](#making-a-release))

Load a skill from any agent:

```
Use the ferroclass-release skill to ...
```

See [AGENTS.md](AGENTS.md) for architecture rules, agent delegation patterns, and
development guidelines.

## Making a Release

For the full release checklist, load the **`ferroclass-release`** skill (see
[Agentic Development](#agentic-development) above) or read
[`.agents/skills/ferroclass-release/SKILL.md`](.agents/skills/ferroclass-release/SKILL.md).

Brief version:

1. Update `version` in `Cargo.toml` and `Version`/`Release` in `packaging/rpm/ferroclass.spec`
2. Add a changelog entry in `packaging/rpm/ferroclass.changes`
3. Update `CHANGELOG.md`
4. Run `make commit` to verify everything passes
5. Commit and tag

> **Note:** `Makefile` reads the version from the spec file. Keep it as the single
> source of truth and ensure `Cargo.toml` matches it.
