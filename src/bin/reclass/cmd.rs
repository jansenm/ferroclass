// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory as inv;
use ferroclass::inventory::options::Options;
use ferroclass::output::format_output;
use ferroclass::output::format_timestamp;
use ferroclass::output::reclass::{InventoryError, InventoryOutput, NodeInfoOutput};
use snafu::prelude::*;

#[derive(Debug, Snafu)]
pub(crate) enum Error {
    #[snafu(transparent)]
    Inventory { source: inv::Error },
    #[snafu(transparent)]
    InventoryLoad { source: InventoryError },
    #[snafu(display("node '{node_name}' not found"))]
    NodeNotFound { node_name: String },
    #[snafu(display("error merging node '{node_name}'"))]
    Merge {
        source: Box<inv::Error>,
        node_name: String,
    },
    #[snafu(display("error serializing output: {message}"))]
    Output { message: String },
    #[snafu(display("inventory has {count} error(s)"))]
    LoadErrors { count: usize },
}

/// Print diagnostics to stderr.
///
/// Returns `Err(LoadErrors)` if any `Error`-severity diagnostics are present,
/// causing the CLI to exit with no data output. Warnings are printed but do
/// not block output.
fn check_diagnostics(result: &inv::LoadResult) -> Result<(), Error> {
    let diagnostics = result.diagnostics();
    if diagnostics.is_empty() {
        return Ok(());
    }

    let error_count = diagnostics
        .iter()
        .filter(|d| d.severity == inv::DiagnosticSeverity::Error)
        .count();

    for diag in diagnostics {
        match diag.severity {
            inv::DiagnosticSeverity::Error => {
                eprintln!("{}", diag);
            }
            inv::DiagnosticSeverity::Warning => {
                eprintln!("{}", diag);
            }
            inv::DiagnosticSeverity::Info => {
                tracing::info!("{}", diag);
            }
            inv::DiagnosticSeverity::Hint => {
                tracing::debug!("{}", diag);
            }
        }
    }

    if error_count > 0 {
        return Err(Error::LoadErrors { count: error_count });
    }

    Ok(())
}

#[cfg(not(tarpaulin_include))]
pub(crate) fn inventory_main(config: Options) -> Result<(), Error> {
    tracing::debug!("starting");
    let merge_config = config.build_merge_config();
    let ignore_failed_node = merge_config.inventory_ignore_failed_node;
    let ignore_failed_render = merge_config.inventory_ignore_failed_render;
    let load_result = inv::load_with_diagnostics(&config.storage_options).map_err(Error::from)?;
    check_diagnostics(&load_result)?;
    let mut inventory_obj = load_result.into_inventory();
    inventory_obj.set_class_mappings(config.class_mappings);
    inventory_obj.set_class_mappings_match_path(config.class_mappings_match_path);
    inventory_obj.set_merge_config(merge_config);

    let timestamp = format_timestamp();
    let pretty = config.output_options.pretty_print;
    let sorted = config.output_options.output_sorted;
    let output_format = config.output_options.output;
    let mut output = InventoryOutput::from_inventory(
        &inventory_obj,
        &timestamp,
        ignore_failed_node,
        ignore_failed_render,
    )
    .map_err(Error::from)?;
    if sorted {
        output.sort_keys();
    }

    let result =
        format_output(&output, output_format, pretty, sorted).map_err(|e| Error::Output {
            message: e.to_string(),
        })?;
    println!("{}", result);
    tracing::debug!("finished");
    Ok(())
}

#[cfg(not(tarpaulin_include))]
pub(crate) fn nodeinfo_main(config: Options, node_name: &str) -> Result<(), Error> {
    tracing::debug!("starting nodeinfo for {node_name}");
    let merge_config = config.build_merge_config();
    let load_result = inv::load_with_diagnostics(&config.storage_options).map_err(Error::from)?;
    check_diagnostics(&load_result)?;
    let mut inventory_obj = load_result.into_inventory();
    inventory_obj.set_class_mappings(config.class_mappings);
    inventory_obj.set_class_mappings_match_path(config.class_mappings_match_path);
    inventory_obj.set_merge_config(merge_config);

    let _node = inventory_obj
        .get_node(node_name)
        .ok_or_else(|| Error::NodeNotFound {
            node_name: node_name.to_string(),
        })?;

    let merged = inventory_obj
        .merge_node(node_name)
        .map_err(|e| Error::Merge {
            source: Box::new(e),
            node_name: node_name.to_string(),
        })?;

    if !merged.is_usable() {
        // Print all diagnostics for the failed node
        for diag in merged.diagnostics() {
            eprintln!("{}", diag);
        }
        let first_msg = merged
            .diagnostics()
            .first()
            .map(|d| d.message.as_str())
            .unwrap_or("merge failed");
        return Err(Error::Merge {
            source: Box::new(inv::Error::NodeNotFound {
                node_name: node_name.to_string(),
            }),
            node_name: format!("{}: {}", node_name, first_msg),
        });
    }

    let timestamp = format_timestamp();
    let pretty = config.output_options.pretty_print;
    let output_format = config.output_options.output;
    let output = NodeInfoOutput::from_node(&merged, &timestamp);

    let result =
        format_output(&output, output_format, pretty, false).map_err(|e| Error::Output {
            message: e.to_string(),
        })?;
    println!("{}", result);
    tracing::debug!("finished");
    Ok(())
}
