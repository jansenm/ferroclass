// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::options::Options as InventoryOptions;
use ferroclass::inventory::options::OutputOptions as InventoryOutputOptions;
use ferroclass::inventory::options::StorageOptions as InventoryStorageOptions;
use ferroclass::inventory::options::StorageOptionsTrait;
use shellexpand::LookupError;
use snafu::prelude::*;

#[derive(Debug, Snafu)]
pub enum ApplyError {
    #[snafu(display("invalid configuration: {message}"))]
    Configuration {
        message: String,
        source: LookupError<std::env::VarError>,
    },
}

pub fn apply_options(
    config: &mut InventoryOptions,
    options: &crate::parser_options::Arguments,
) -> Result<InventoryOptions, ApplyError> {
    let lso = &config.storage_options;
    let rso = &options.storage_options;

    let inventory_base_uri = shellexpand::full(
        rso.inventory_base_uri
            .as_ref()
            .unwrap_or(&lso.yaml_fs_options.inventory_base_uri),
    )
    .context(ConfigurationSnafu {
        message: "inventory_base_uri is invalid",
    })?;
    let parameter_key_style = rso
        .parameter_key_style
        .clone()
        .unwrap_or(lso.parameter_key_style());
    let default_environment = config.default_environment.clone();

    let storage_options: InventoryStorageOptions = InventoryStorageOptions {
        storage_type: rso.storage_type.unwrap_or(lso.storage_type),
        yaml_fs_options: ferroclass::inventory::options::YamlFsStorageOptions {
            inventory_base_uri: inventory_base_uri.to_string(),
            nodes_uri: rso
                .nodes_uri
                .as_ref()
                .unwrap_or(&lso.yaml_fs_options.nodes_uri)
                .clone(),
            classes_uri: rso
                .classes_uri
                .as_ref()
                .unwrap_or(&lso.yaml_fs_options.classes_uri)
                .clone(),
            parameter_key_style: parameter_key_style.clone(),
            compose_node_name: rso.compose_node_name || lso.yaml_fs_options.compose_node_name,
            default_environment: default_environment.clone(),
        },
        yaml_file_options: ferroclass::inventory::options::YamlFileStorageOptions {
            inventory_file: lso.yaml_file_options.inventory_file.clone(),
            parameter_key_style,
            default_environment: default_environment.clone(),
        },
    };
    let loo = &config.output_options;
    let roo = &options.output_options;
    let output_options: InventoryOutputOptions = InventoryOutputOptions {
        output: *roo.output.as_ref().unwrap_or(&loo.output),
        pretty_print: roo.pretty_print || loo.pretty_print,
        output_sorted: roo.output_sorted || loo.output_sorted,
        no_refs: roo.no_refs || loo.no_refs,
        group_errors: if options.output_options.single_error {
            false
        } else if options.output_options.group_errors {
            true
        } else {
            loo.group_errors
        },
    };

    Ok(InventoryOptions {
        storage_options,
        output_options,
        class_mappings: config.class_mappings.clone(),
        class_mappings_match_path: config.class_mappings_match_path,
        default_environment,
        verbose: options.verbose,
        ignore_class_notfound: options.storage_options.ignore_class_notfound,
        ignore_class_notfound_regexp: options
            .storage_options
            .ignore_class_notfound_regexp
            .clone()
            .unwrap_or_default(),
        inventory_ignore_failed_node: config.inventory_ignore_failed_node,
        inventory_ignore_failed_render: config.inventory_ignore_failed_render,
        ..InventoryOptions::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_options() -> InventoryOptions {
        InventoryOptions::default()
    }

    fn default_arguments() -> crate::parser_options::Arguments {
        crate::parser_options::Arguments {
            verbose: false,
            storage_options: crate::parser_options::StorageOptions::default(),
            output_options: crate::parser_options::OutputOptions::default(),
            command_options: crate::parser_options::CommandOptions {
                inventory: true,
                nodeinfo: None,
            },
        }
    }

    #[test]
    fn test_apply_options_defaults() {
        let mut config = default_options();
        let args = default_arguments();
        let result = apply_options(&mut config, &args).unwrap();
        assert_eq!(result.storage_options.yaml_fs_options.nodes_uri, "nodes");
        assert_eq!(
            result.storage_options.yaml_fs_options.classes_uri,
            "classes"
        );
    }

    #[test]
    fn test_apply_options_verbose() {
        let mut config = default_options();
        let mut args = default_arguments();
        args.verbose = true;
        let result = apply_options(&mut config, &args).unwrap();
        assert!(result.verbose);
    }

    #[test]
    fn test_apply_options_group_errors_single() {
        let mut config = default_options();
        let mut args = default_arguments();
        args.output_options.single_error = true;
        let result = apply_options(&mut config, &args).unwrap();
        assert!(!result.output_options.group_errors);
    }

    #[test]
    fn test_apply_options_group_errors_group() {
        let mut config = default_options();
        let mut args = default_arguments();
        args.output_options.group_errors = true;
        let result = apply_options(&mut config, &args).unwrap();
        assert!(result.output_options.group_errors);
    }

    #[test]
    fn test_apply_options_ignore_class_notfound() {
        let mut config = default_options();
        let mut args = default_arguments();
        args.storage_options.ignore_class_notfound = true;
        let result = apply_options(&mut config, &args).unwrap();
        assert!(result.ignore_class_notfound);
    }
}
