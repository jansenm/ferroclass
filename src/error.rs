// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Unified error type for the ferroclass library.
//!
//! [`Error`] wraps all sub-module errors into a single type that can be used
//! throughout the library and by external consumers. Each variant carries
//! the original sub-error via `#[snafu(source)]`, so you can downcast
//! to the specific error type if needed.
//!
//! Sub-module error types remain accessible for granular matching:
//!
//! - [`inventory::Error`](crate::inventory::Error) — inventory loading/merging errors
//! - [`inventory::MergeError`](crate::inventory::MergeError) — merge-specific errors
//! - [`inventory::ValueMergeError`](crate::inventory::ValueMergeError) — value merge errors
//!
//! # Example
//!
//! ```rust,ignore
//! use ferroclass::{load, StorageOptions, StorageType, Error};
//!
//! let result = load(&storage_options);
//! match result {
//!     Ok(inventory) => { /* ... */ }
//!     Err(Error::Inventory { source, .. }) => eprintln!("Inventory error: {}", source),
//!     Err(Error::Configuration { source, .. }) => eprintln!("Config error: {}", source),
//!     Err(Error::Storage { source, .. }) => eprintln!("Storage error: {}", source),
//!     Err(e) => eprintln!("Other error: {}", e),
//! }
//! ```

use crate::configuration;
use crate::configuration_file;
use crate::inventory;
use crate::storage::file_system;
use snafu::Snafu;

/// The top-level error type for the ferroclass library.
///
/// All errors returned by ferroclass functions can be converted into this
/// type. It wraps domain-specific errors from sub-modules while providing
/// a unified interface for consumers.
///
/// For granular error matching, use the sub-module error types directly
/// (e.g., `inventory::Error` for inventory-specific errors).
#[derive(Debug, Snafu)]
pub enum Error {
    /// An error during inventory loading, merging, or querying.
    #[snafu(transparent)]
    Inventory { source: inventory::Error },

    /// An error during merge operations (class not found, circular reference, etc.).
    #[snafu(transparent)]
    Merge { source: inventory::MergeError },

    /// An error during value merge operations (type conflicts, override errors, etc.).
    #[snafu(transparent)]
    ValueMerge { source: inventory::ValueMergeError },

    /// An error reading configuration files (reclass-config.yml).
    #[snafu(transparent)]
    Configuration { source: configuration::Error },

    /// An error parsing configuration file contents.
    #[snafu(transparent)]
    ConfigurationFile { source: configuration_file::Error },

    /// An error reading files from the storage backend (YAML files, directory traversal).
    #[snafu(transparent)]
    Storage { source: file_system::Error },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_from_inventory_error() {
        let inv_error = inventory::Error::NodeNotFound {
            node_name: "test".to_string(),
        };
        let error: Error = inv_error.into();
        match error {
            Error::Inventory { source } => {
                assert!(matches!(source, inventory::Error::NodeNotFound { .. }));
            }
            _ => panic!("Expected Inventory variant"),
        }
    }

    #[test]
    fn test_error_from_merge_error() {
        let merge_error = inventory::MergeError::ClassNotFound {
            class_name: "foo".to_string(),
        };
        let error: Error = merge_error.into();
        match error {
            Error::Merge { source } => {
                assert!(matches!(
                    source,
                    inventory::MergeError::ClassNotFound { .. }
                ));
            }
            _ => panic!("Expected Merge variant"),
        }
    }

    #[test]
    fn test_error_display() {
        let inv_error = inventory::Error::NodeNotFound {
            node_name: "web01".to_string(),
        };
        let error: Error = inv_error.into();
        let msg = format!("{}", error);
        assert!(
            msg.contains("web01"),
            "Error display should contain node name"
        );
    }

    #[test]
    fn test_error_chain() {
        // inventory::Error can wrap storage errors via Repository variant
        let inv_error = inventory::Error::NodeNotFound {
            node_name: "missing".to_string(),
        };
        let top_error: Error = inv_error.into();
        // Should be convertible back for inspection
        match top_error {
            Error::Inventory { source } => {
                assert!(matches!(source, inventory::Error::NodeNotFound { .. }));
            }
            _ => panic!("Expected Inventory variant"),
        }
    }
}
