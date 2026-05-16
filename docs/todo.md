<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

# Roadmap

## Planned Features

- **Git storage type (`yaml_git`)** — Read nodes and classes from a remote Git repository, with
  branch-per-environment support, SSH key configuration, and caching
- **Wildcard/regexp class mappings** — Pattern-based class name matching for nodes (
  see [reclass operations docs](https://reclass.pantsfullofunix.net/operations.html#wildcard-regexp-mappings))
- **PyO3 Python bindings** — Compile the Rust library as a native Python extension module, enabling
  `propagate_pillar_data_to_reclass` for Salt
- **CLI input data flags** — `--input-data` and `--input-data-file` flags for `reclass-salt` for
  standalone use with JSON/YAML input data
- **End-to-end integration tests** — Comprehensive tests against real inventory files, including
  Python compatibility comparison
- **DEB packaging** — Debian/Ubuntu packages
- **CI pipeline** — GitHub Actions for build, test, clippy, coverage
- **YAML output optimization** — Avoid intermediate Yaml tree conversion in serialization

## Deferred Indefinitely

- **Mixed storage type** — No known users. Will revisit if demand emerges.
- **`scalar_reclass_parameters`** — Zero test coverage, zero documentation, no known users in the
  wild. Will not be implemented.
