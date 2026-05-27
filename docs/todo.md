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

## Completed

- **Wildcard/regexp class mappings** — Glob and regex patterns matching node names
  (see [reclass operations docs](https://reclass.pantsfullofunix.net/operations.html#wildcard-regexp-mappings)).
  Available since 0.9.0 (CLI) and 0.11.1 (Python API).
- **PyO3 Python bindings** — Native Python extension module built with PyO3 0.23.
  Provides `ext_pillar()`, `top()`, and `load()` for Salt integration. Available
  since 0.11.1.

## Deferred Indefinitely

- **Mixed storage type** — No known users. Will revisit if demand emerges.
- **`scalar_reclass_parameters`** — Zero test coverage, zero documentation, no known users in the
  wild. Will not be implemented.
