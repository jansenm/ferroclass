<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0] - 2026-05-16

First public release. Feature-complete reimplementation of Python reclass
([salt-formulas/reclass](https://github.com/salt-formulas/reclass)) with 100% CLI
compatibility for the `reclass`, `reclass-ansible`, and `reclass-salt` commands.

### Added

- Full class inheritance with depth-first resolution
- Parameter interpolation (`${...}` references, nested references)
- Inventory queries (`$[...]` syntax: VALUE, TEST, LIST_TEST)
- Two-pass rendering pipeline for cross-node references
- Override prefix (`~key`) for replacing values instead of merging
- Constant prefix (`=key`) for locking values against later changes
- Class name interpolation with `${...}` in the classes list
- Relative class names (`.` and `..` prefix)
- Class mappings (glob/regex patterns matching node names)
- Environment support (default `base`, per-node environment)
- Exports with interpolation and inventory query resolution
- Applications list with `~` negation prefix
- `compose_node_name` for subdirectory-based node names
- `ignore_class_notfound` with regexp filtering
- `ignore_overwritten_missing_reference`
- `inventory_ignore_failed_node` / `inventory_ignore_failed_render`
- `group_errors` for error summarization
- Output formats: YAML and JSON (with pretty-print and sort options)
- Ansible dynamic inventory adapter (`--list` / `--host`)
- Salt external node classifier adapter (`--top` / `--pillar` / `--out`)
- Man pages for all three commands
- RPM packaging (SUSE and RHEL conditional macros)

### Known Incompatibilities

See [README.md](README.md#reclass-compatibility) for the full list.
