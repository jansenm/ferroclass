// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Output format definitions.
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Supported output formats for inventory data.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
#[clap(rename_all = "lower")]
pub enum OutputFormat {
    /// YAML output format
    #[default]
    Yaml,
    /// JSON output format
    JSON,
}
