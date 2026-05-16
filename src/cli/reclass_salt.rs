// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! CLI argument definitions for the reclass-salt binary.

use clap::{Args, Parser};
use serde::{Deserialize, Serialize};

use crate::inventory::options::{OutputFormat, ParameterKeyStyle, StorageType};

#[derive(Args, Debug, Default, Serialize, Deserialize)]
#[group(multiple = false, required = true)]
pub struct CommandOptions {
    /// output the state tops (environment to node to applications mapping)
    #[arg(short, long)]
    pub top: bool,

    /// output pillar data for a specific minion
    #[arg(short = 'p', long, value_name = "MINION_ID")]
    pub pillar: Option<String>,
}

#[derive(Args, Debug, Default, Deserialize, Serialize)]
#[group()]
pub struct StorageOptions {
    /// the type of storage backend to use
    #[arg(value_enum, short, long)]
    pub storage_type: Option<StorageType>,

    /// the base URI to prepend to nodes and classes
    #[arg(short = 'b', long)]
    pub inventory_base_uri: Option<String>,

    /// the URI to the node storage
    #[arg(short = 'u', long)]
    pub nodes_uri: Option<String>,

    /// the URI to the class storage
    #[arg(short = 'c', long)]
    pub classes_uri: Option<String>,

    /// parameter key validation style
    #[arg(value_enum, long)]
    pub parameter_key_style: Option<ParameterKeyStyle>,

    /// compose node names from subdirectory paths
    #[arg(short = 'a', long)]
    pub compose_node_name: bool,

    /// ignore classes that are not found instead of raising an error
    #[arg(short = 'z', long)]
    pub ignore_class_notfound: bool,

    /// regexp pattern for class names to ignore when not found (can be specified multiple times)
    #[arg(short = 'x', long, value_name = "PATTERN")]
    pub ignore_class_notfound_regexp: Option<Vec<String>>,
}

#[derive(Args, Debug, Default, Deserialize, Serialize)]
#[group()]
pub struct OutputOptions {
    /// output format (default: yaml)
    #[arg(value_enum, short, long, default_value = "yaml")]
    pub output: OutputFormat,

    /// pretty-print output (indented, newlines)
    #[arg(short = 'y', long)]
    pub pretty_print: bool,

    /// sort keys alphabetically in output
    #[arg(long)]
    pub output_sorted: bool,

    /// suppress YAML anchors/aliases in output
    #[arg(short = 'r', long)]
    pub no_refs: bool,

    /// group multiple resolve errors and report them together instead of failing on the first error
    #[arg(short = '0', long = "multiple-errors", conflicts_with = "single_error")]
    pub group_errors: bool,

    /// report only the first resolve error instead of grouping all errors
    #[arg(short = '1', long = "single-error", conflicts_with = "group_errors")]
    pub single_error: bool,
}

#[derive(Parser, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
#[command(
    name = "reclass-salt",
    bin_name = "reclass-salt",
    version,
    about = "Salt adapter for reclass",
    after_help = "This binary provides Salt-compatible output.\n\
                  Use --top to output the state tops (environment → node → applications).\n\
                  Use --pillar MINION_ID to output pillar data for a specific minion."
)]
pub struct Cli {
    /// enable verbose logging
    #[arg(short, long)]
    pub verbose: bool,

    #[command(flatten, next_help_heading = "Database options")]
    pub storage_options: StorageOptions,

    #[command(flatten, next_help_heading = "Output options")]
    pub output_options: OutputOptions,

    #[command(flatten, next_help_heading = "Modes")]
    pub command_options: CommandOptions,
}
