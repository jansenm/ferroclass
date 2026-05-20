// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

extern crate core;

pub mod cli;
pub mod configuration;
pub(crate) mod configuration_file;
pub mod inventory;
pub mod output;
pub(crate) mod parser;
pub mod storage;

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
