// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Output configuration options.

use super::output_format::OutputFormat;
use serde::{Deserialize, Serialize};

/// Configuration for output formatting.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutputOptions {
    /// The output format to use (YAML or JSON)
    pub output: OutputFormat,

    /// Whether to pretty-print the output (indented, newlines)
    pub pretty_print: bool,

    /// Whether to sort keys alphabetically in output
    pub output_sorted: bool,

    /// Whether to suppress YAML anchors/aliases in output
    /// (always effectively true in this implementation; accepted for CLI compatibility)
    pub no_refs: bool,

    /// Whether to group all resolve errors before reporting (true) or fail on first error (false)
    pub group_errors: bool,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            output: Default::default(),
            pretty_print: true,
            output_sorted: false,
            no_refs: false,
            group_errors: true,
        }
    }
}
