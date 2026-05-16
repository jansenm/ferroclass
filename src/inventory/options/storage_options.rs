// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Storage backend configuration options.

use crate::inventory::options::storage_type::StorageType;
use crate::inventory::types::Environment;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Parameter key validation style.
#[derive(Debug, Deserialize, Serialize, Clone, Default, ValueEnum)]
pub enum ParameterKeyStyle {
    /// No validation of parameter keys.
    #[default]
    None,
    /// Ansible variable name rules (letters, numbers, underscores only).
    Ansible,
}

/// Trait for storage options providing common functionality.
pub trait StorageOptionsTrait {
    fn inventory_path(&self) -> PathBuf;
    fn parameter_key_style(&self) -> ParameterKeyStyle;
    fn compose_node_name(&self) -> bool;
    fn default_environment(&self) -> Environment;
}

/// Configuration for the file system repository storage backend.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct YamlFsStorageOptions {
    /// The base URI for the inventory
    pub inventory_base_uri: String,

    /// The URI path for node storage (relative to base URI or absolute)
    pub nodes_uri: String,

    /// The URI path for class storage (relative to base URI or absolute)
    pub classes_uri: String,

    /// Parameter key validation style
    pub parameter_key_style: ParameterKeyStyle,

    /// Whether to compose node names from subdirectory paths
    /// (e.g., nodes/munich/server.yml → munich.server instead of server)
    pub compose_node_name: bool,

    /// Default environment for nodes/classes that don't specify one
    pub default_environment: Environment,
}

impl YamlFsStorageOptions {
    /// Creates a new YamlFsStorageOptions with the specified base URI.
    pub fn build(inventory_base_uri: String) -> Self {
        Self {
            inventory_base_uri,
            ..Self::default()
        }
    }

    /// Creates a new YamlFsStorageOptions with all fields specified.
    pub fn build_with_options(
        inventory_base_uri: String,
        nodes_uri: String,
        classes_uri: String,
        parameter_key_style: ParameterKeyStyle,
        compose_node_name: bool,
        default_environment: Environment,
    ) -> Self {
        Self {
            inventory_base_uri,
            nodes_uri,
            classes_uri,
            parameter_key_style,
            compose_node_name,
            default_environment,
        }
    }

    /// Returns the nodes storage path.
    pub fn nodes_path(&self) -> PathBuf {
        PathBuf::from(&self.inventory_base_uri).join(PathBuf::from(&self.nodes_uri))
    }

    /// Returns the classes storage path.
    pub fn classes_path(&self) -> PathBuf {
        PathBuf::from(&self.inventory_base_uri).join(PathBuf::from(&self.classes_uri))
    }
}

impl StorageOptionsTrait for YamlFsStorageOptions {
    fn inventory_path(&self) -> PathBuf {
        PathBuf::from(&self.inventory_base_uri)
    }

    fn parameter_key_style(&self) -> ParameterKeyStyle {
        self.parameter_key_style.clone()
    }

    fn compose_node_name(&self) -> bool {
        self.compose_node_name
    }

    fn default_environment(&self) -> Environment {
        self.default_environment.clone()
    }
}

impl Default for YamlFsStorageOptions {
    fn default() -> Self {
        Self {
            inventory_base_uri: String::from("/etc/reclass"),
            nodes_uri: "nodes".to_string(),
            classes_uri: "classes".to_string(),
            parameter_key_style: ParameterKeyStyle::default(),
            compose_node_name: false,
            default_environment: Environment::default(),
        }
    }
}

/// Configuration for the single file repository storage backend.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct YamlFileStorageOptions {
    /// The path to the inventory file (same concept as inventory_base_uri for YamlFs)
    pub inventory_file: String,

    /// Parameter key validation style
    pub parameter_key_style: ParameterKeyStyle,

    /// Default environment for nodes/classes that don't specify one
    pub default_environment: Environment,
}

impl YamlFileStorageOptions {
    pub fn build(inventory_file: String) -> Self {
        Self {
            inventory_file,
            parameter_key_style: ParameterKeyStyle::default(),
            default_environment: Environment::default(),
        }
    }
}

impl StorageOptionsTrait for YamlFileStorageOptions {
    fn inventory_path(&self) -> PathBuf {
        PathBuf::from(&self.inventory_file)
    }

    fn parameter_key_style(&self) -> ParameterKeyStyle {
        self.parameter_key_style.clone()
    }

    fn compose_node_name(&self) -> bool {
        false
    }

    fn default_environment(&self) -> Environment {
        self.default_environment.clone()
    }
}

impl Default for YamlFileStorageOptions {
    fn default() -> Self {
        Self {
            inventory_file: String::from("/etc/reclass/inventory.yml"),
            parameter_key_style: ParameterKeyStyle::default(),
            default_environment: Environment::default(),
        }
    }
}

/// Configuration for the storage backend.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StorageOptions {
    /// The type of storage backend to use
    pub storage_type: StorageType,

    /// File system repository options
    pub yaml_fs_options: YamlFsStorageOptions,

    /// Single file repository options
    pub yaml_file_options: YamlFileStorageOptions,
}

impl StorageOptions {
    /// Creates a new StorageOptions with the specified storage type and base URI.
    pub fn build(storage_type: StorageType, inventory_base_uri: String) -> Self {
        Self {
            storage_type,
            yaml_fs_options: YamlFsStorageOptions::build(inventory_base_uri),
            ..Self::default()
        }
    }

    /// Returns the inventory path using the appropriate storage options.
    pub fn inventory_path(&self) -> PathBuf {
        match self.storage_type {
            StorageType::YamlFs => self.yaml_fs_options.inventory_path(),
            StorageType::YamlFile => self.yaml_file_options.inventory_path(),
        }
    }

    /// Returns the nodes storage path (only for YamlFs).
    pub fn nodes_path(&self) -> PathBuf {
        self.yaml_fs_options.nodes_path()
    }

    /// Returns the classes storage path (only for YamlFs).
    pub fn classes_path(&self) -> PathBuf {
        self.yaml_fs_options.classes_path()
    }

    /// Returns the YamlFs storage options.
    pub fn yaml_fs_options(&self) -> &YamlFsStorageOptions {
        &self.yaml_fs_options
    }

    /// Returns the YamlFile storage options.
    pub fn yaml_file_options(&self) -> &YamlFileStorageOptions {
        &self.yaml_file_options
    }
}

impl StorageOptionsTrait for StorageOptions {
    fn inventory_path(&self) -> PathBuf {
        match self.storage_type {
            StorageType::YamlFs => self.yaml_fs_options.inventory_path(),
            StorageType::YamlFile => self.yaml_file_options.inventory_path(),
        }
    }

    fn parameter_key_style(&self) -> ParameterKeyStyle {
        self.yaml_fs_options.parameter_key_style()
    }

    fn compose_node_name(&self) -> bool {
        self.yaml_fs_options.compose_node_name()
    }

    fn default_environment(&self) -> Environment {
        self.yaml_fs_options.default_environment()
    }
}

impl Default for StorageOptions {
    fn default() -> Self {
        Self {
            storage_type: StorageType::YamlFs,
            yaml_fs_options: YamlFsStorageOptions::default(),
            yaml_file_options: YamlFileStorageOptions::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths() {
        let storage_options = StorageOptions::build(StorageType::YamlFs, String::from("/tmp"));
        assert_eq!(storage_options.inventory_path(), PathBuf::from("/tmp"));
        assert_eq!(storage_options.nodes_path(), PathBuf::from("/tmp/nodes"));
        assert_eq!(
            storage_options.classes_path(),
            PathBuf::from("/tmp/classes")
        );
    }

    #[test]
    fn mixed() {
        let storage_options = StorageOptions {
            yaml_fs_options: YamlFsStorageOptions {
                nodes_uri: "/tmp/nodes".to_string(),
                ..YamlFsStorageOptions::default()
            },
            ..StorageOptions::default()
        };

        assert_eq!(
            storage_options.inventory_path(),
            PathBuf::from("/etc/reclass")
        );
        assert_eq!(storage_options.nodes_path(), PathBuf::from("/tmp/nodes"));
        assert_eq!(
            storage_options.classes_path(),
            PathBuf::from("/etc/reclass/classes")
        );
    }
    #[test]
    fn test_paths_absolut() {
        let storage_options = StorageOptions {
            yaml_fs_options: YamlFsStorageOptions {
                nodes_uri: "/tmp/nodes".to_string(),
                classes_uri: "/tmp/classes".to_string(),
                ..YamlFsStorageOptions::default()
            },
            ..StorageOptions::default()
        };

        assert_eq!(
            storage_options.inventory_path(),
            PathBuf::from("/etc/reclass")
        );
        assert_eq!(storage_options.nodes_path(), PathBuf::from("/tmp/nodes"));
        assert_eq!(
            storage_options.classes_path(),
            PathBuf::from("/tmp/classes")
        );
    }
}
