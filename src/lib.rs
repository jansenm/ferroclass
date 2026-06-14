// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Ferroclass — hierarchical inventory management compatible with Python reclass.
//!
//! Ferroclass reads a directory tree of YAML class and node definitions,
//! resolves class inheritance and variable interpolation (`$[...]` references
//! and inventory queries), merges parameters, and produces structured output
//! for Ansible, Salt, or plain reclass-compatible formats.
//!
//! # Quick start
//!
//! ```
//! use ferroclass::{Inventory, Options, StorageOptions, StorageType};
//!
//! let options = Options::default();
//! // Load from a YAML filesystem layout (requires a real directory):
//! // let inventory = ferroclass::load(&options.storage_options).unwrap();
//! // Merge a single node:
//! // let merged = inventory.merge_node("web01.example.com").unwrap();
//! ```
//!
//! # Crate layout
//!
//! - [`inventory`] — core domain types, loading, and merging
//! - [`output`] — formatting to Ansible, Salt, and reclass YAML/JSON
//! - [`configuration`] — loading `reclass-config.yml` configuration files
//! - [`storage`] — filesystem backends for reading YAML classes and nodes
//! - [`cli`] — CLI argument definitions shared by the binaries

extern crate core;

pub mod cli;
pub mod configuration;
pub(crate) mod configuration_file;
pub mod inventory;
pub mod output;
pub(crate) mod parser;
pub mod storage;

#[cfg(feature = "python")]
pub mod python;

// Re-export the most commonly used types at the crate root for convenience.
// These are the primary entry points for library consumers.
pub use inventory::Inventory;
pub use inventory::load;
pub use inventory::load_from_yaml_string;
pub use inventory::load_from_yaml_string_with_uri;
pub use inventory::options::Options;
pub use inventory::options::{
    MergeConfig, OutputFormat, OutputOptions, ParameterKeyStyle, StorageOptions,
    StorageOptionsTrait, StorageType, YamlFileStorageOptions, YamlFsStorageOptions,
};
pub use inventory::diagnostic::{Diagnostic, DiagnosticSeverity, SourceLocation};
