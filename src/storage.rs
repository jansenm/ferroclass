// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Storage backends for reading YAML class and node definitions from disk.
//!
//! Currently only the [`file_system`] backend is supported (directory-tree
//! layout with `_init.yml` autoloading), matching the Python reclass
//! `yaml_fs` and `yaml_file` storage types.

pub mod file_system;
