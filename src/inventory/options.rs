// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Configuration options for the inventory system.
//!
//! This module provides types for configuring storage, output formatting,
//! and general application options.
//!
//! # Re-exported types
//!
//! - [`StorageType`] - Defines the type of storage backend
//! - [`StorageOptions`] - Configuration for storage backend
//! - [`OutputFormat`] - Supported output formats (YAML, JSON)
//! - [`OutputOptions`] - Configuration for output formatting
//! - [`MergeConfig`] - Configuration for merge behavior

mod merge_options;
mod output_format;
mod output_options;
mod storage_options;
mod storage_type;

pub use merge_options::MergeConfig;
pub use output_format::OutputFormat;
pub use output_options::OutputOptions;
pub use storage_options::ParameterKeyStyle;
pub use storage_options::StorageOptions;
pub use storage_options::StorageOptionsTrait;
pub use storage_options::YamlFileStorageOptions;
pub use storage_options::YamlFsStorageOptions;
pub use storage_type::StorageType;

use crate::inventory::class_mapping::ClassMapping;
use crate::inventory::types::Environment;

/// Global application options combining storage, output, and class mapping configuration.
#[derive(Debug, Default)]
pub struct Options {
    /// Enable verbose logging output
    pub verbose: bool,
    /// Storage backend configuration
    pub storage_options: StorageOptions,
    /// Output formatting configuration
    pub output_options: OutputOptions,
    /// Class mappings for auto-including classes based on node name/path patterns
    pub class_mappings: Vec<ClassMapping>,
    /// Whether class mapping patterns match against node path instead of node name
    pub class_mappings_match_path: bool,
    /// Default environment for nodes/classes that don't specify one
    pub default_environment: Environment,
    /// Ignore classes that are not found instead of raising an error
    pub ignore_class_notfound: bool,
    /// Regexp patterns for class names to ignore when not found
    pub ignore_class_notfound_regexp: Vec<String>,
    /// Whether to warn when a class is not found and skipped
    pub ignore_class_notfound_warning: bool,
    /// Whether to suppress ResolveError for non-final values that get overwritten (default: true)
    pub ignore_overwritten_missing_references: bool,
    /// Whether to skip nodes that fail to render in inventory output instead of aborting (default: false)
    pub inventory_ignore_failed_node: bool,
    /// Whether to skip inv query render errors instead of aborting (default: false; per-query +IgnoreErrors overrides)
    pub inventory_ignore_failed_render: bool,
}

impl Options {
    pub fn build(storage_options: StorageOptions, output_options: OutputOptions) -> Self {
        Self {
            storage_options,
            output_options,
            ..Self::default()
        }
    }

    pub fn build_with_class_mappings(
        storage_options: StorageOptions,
        output_options: OutputOptions,
        class_mappings: Vec<ClassMapping>,
        class_mappings_match_path: bool,
    ) -> Self {
        Self {
            storage_options,
            output_options,
            class_mappings,
            class_mappings_match_path,
            ..Self::default()
        }
    }

    pub fn build_merge_config(&self) -> MergeConfig {
        let mut config = MergeConfig::default();
        if self.ignore_class_notfound {
            config = config
                .ignore_class_notfound(true)
                .ignore_class_notfound_regexp(self.ignore_class_notfound_regexp.clone())
                .ignore_class_notfound_warning(self.ignore_class_notfound_warning);
        }
        config = config.group_errors(self.output_options.group_errors);
        config = config
            .ignore_overwritten_missing_references(self.ignore_overwritten_missing_references);
        config = config.inventory_ignore_failed_node(self.inventory_ignore_failed_node);
        config = config.inventory_ignore_failed_render(self.inventory_ignore_failed_render);
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::options::output_options::OutputOptions;
    use crate::inventory::options::storage_options::StorageOptions;

    #[test]
    fn test_options_default() {
        let opts = Options::default();
        assert!(!opts.verbose);
        assert!(opts.class_mappings.is_empty());
        assert!(!opts.class_mappings_match_path);
        assert!(!opts.ignore_class_notfound);
        assert!(opts.ignore_class_notfound_regexp.is_empty());
        assert!(!opts.ignore_overwritten_missing_references);
        assert!(!opts.inventory_ignore_failed_node);
        assert!(!opts.inventory_ignore_failed_render);
    }

    #[test]
    fn test_build_merge_config_defaults() {
        let opts = Options::default();
        let config = opts.build_merge_config();
        assert!(!config.ignore_class_notfound);
        assert!(config.group_errors);
        assert!(!config.ignore_overwritten_missing_references);
        assert!(!config.inventory_ignore_failed_node);
        assert!(!config.inventory_ignore_failed_render);
    }

    #[test]
    fn test_build_merge_config_with_ignore_class_notfound() {
        let opts = Options {
            ignore_class_notfound: true,
            ignore_class_notfound_regexp: vec!["^my\\.".to_string()],
            ..Default::default()
        };
        let config = opts.build_merge_config();
        assert!(config.ignore_class_notfound);
        assert_eq!(
            config.ignore_class_notfound_regexp,
            vec!["^my\\.".to_string()]
        );
    }

    #[test]
    fn test_build_merge_config_group_errors_false() {
        let mut opts = Options::default();
        opts.output_options.group_errors = false;
        let config = opts.build_merge_config();
        assert!(!config.group_errors);
    }

    #[test]
    fn test_options_build() {
        let opts = Options::build(StorageOptions::default(), OutputOptions::default());
        assert!(opts.class_mappings.is_empty());
        assert!(!opts.verbose);
    }

    #[test]
    fn test_options_build_with_class_mappings() {
        let mappings = vec![ClassMapping::parse("* default").unwrap()];
        let opts = Options::build_with_class_mappings(
            StorageOptions::default(),
            OutputOptions::default(),
            mappings.clone(),
            true,
        );
        assert_eq!(opts.class_mappings.len(), 1);
        assert!(opts.class_mappings_match_path);
    }
}
