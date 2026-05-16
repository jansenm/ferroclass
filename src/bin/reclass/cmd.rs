// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory as inv;
use ferroclass::inventory::options::Options;
use ferroclass::output::format_output;
use ferroclass::output::format_timestamp;
use ferroclass::output::reclass::{InventoryError, InventoryOutput, NodeInfoOutput};
use snafu::prelude::*;

#[derive(Debug, Snafu)]
pub(crate) enum Error {
    #[snafu(display("Error while loading the inventory"))]
    Inventory { source: inv::Error },
    #[snafu(display("Error while loading the inventory"))]
    InventoryLoad { source: InventoryError },
    #[snafu(display("Node not found: {node_name}"))]
    NodeNotFound { node_name: String },
    #[snafu(display("Error while merging node {node_name}"))]
    Merge {
        source: inv::Error,
        node_name: String,
    },
    #[snafu(display("Error serializing output: {message}"))]
    Output { message: String },
}

#[cfg(not(tarpaulin_include))]
pub(crate) fn inventory_main(config: Options) -> Result<(), Error> {
    tracing::debug!("starting");
    let merge_config = config.build_merge_config();
    let ignore_failed_node = merge_config.inventory_ignore_failed_node;
    let ignore_failed_render = merge_config.inventory_ignore_failed_render;
    let mut inventory_obj = inv::load(&config.storage_options).context(InventorySnafu {})?;
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
    .context(InventoryLoadSnafu {})?;
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
    let mut inventory_obj = inv::load(&config.storage_options).context(InventorySnafu {})?;
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
            source: e,
            node_name: node_name.to_string(),
        })?;

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
