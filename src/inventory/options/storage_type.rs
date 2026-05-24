// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Storage backend type definitions.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Represents the available storage backend types.
#[derive(
    Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum, Serialize, Deserialize,
)]
pub enum StorageType {
    /// YAML file system storage (directory-based)
    #[default]
    YamlFs,
    /// YAML file storage (single file with multiple documents)
    YamlFile,
}
