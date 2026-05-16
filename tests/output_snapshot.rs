// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::load;
use ferroclass::inventory::options::{
    Options, OutputFormat, OutputOptions, ParameterKeyStyle, StorageOptions, StorageType,
    YamlFsStorageOptions,
};
use ferroclass::output::format_output;
use ferroclass::output::reclass::{InventoryOutput, NodeInfoOutput};
use std::path::PathBuf;

const TEST_TIMESTAMP: &str = "Thu Jan  1 00:00:00 2026";

fn normalize_uris(output: &str) -> String {
    output.replace(env!("CARGO_MANIFEST_DIR"), "$PROJECT_ROOT")
}

fn e2e_inventory_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("inventories")
        .join("e2e")
}

fn python_compat_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("inventories")
        .join("python_compat")
}

fn load_e2e_inventory() -> ferroclass::inventory::Inventory {
    let path = e2e_inventory_path();
    let storage_options = StorageOptions {
        storage_type: StorageType::YamlFs,
        yaml_fs_options: YamlFsStorageOptions {
            inventory_base_uri: path.to_string_lossy().to_string(),
            nodes_uri: "nodes".to_string(),
            classes_uri: "classes".to_string(),
            parameter_key_style: ParameterKeyStyle::None,
            compose_node_name: false,
            ..YamlFsStorageOptions::default()
        },
        ..StorageOptions::default()
    };

    load(&storage_options).expect("Failed to load e2e inventory")
}

fn load_python_compat_inventory() -> ferroclass::inventory::Inventory {
    let path = python_compat_path();
    let storage_options = StorageOptions {
        storage_type: StorageType::YamlFs,
        yaml_fs_options: YamlFsStorageOptions {
            inventory_base_uri: path.to_string_lossy().to_string(),
            nodes_uri: "nodes".to_string(),
            classes_uri: "classes".to_string(),
            parameter_key_style: ParameterKeyStyle::default(),
            compose_node_name: false,
            ..YamlFsStorageOptions::default()
        },
        ..StorageOptions::default()
    };

    load(&storage_options).expect("Failed to load python_compat inventory")
}

fn e2e_options() -> Options {
    let path = e2e_inventory_path();
    Options {
        storage_options: StorageOptions {
            storage_type: StorageType::YamlFs,
            yaml_fs_options: YamlFsStorageOptions {
                inventory_base_uri: path.to_string_lossy().to_string(),
                nodes_uri: "nodes".to_string(),
                classes_uri: "classes".to_string(),
                parameter_key_style: ParameterKeyStyle::None,
                compose_node_name: false,
                ..YamlFsStorageOptions::default()
            },
            ..StorageOptions::default()
        },
        output_options: OutputOptions {
            output: OutputFormat::JSON,
            pretty_print: true,
            output_sorted: false,
            no_refs: false,
            group_errors: true,
        },
        ..Options::default()
    }
}

// --- Reclass format snapshot tests ---

#[test]
fn test_reclass_inventory_yaml_sorted() {
    let inventory = load_e2e_inventory();
    let mut output = InventoryOutput::from_inventory(&inventory, TEST_TIMESTAMP, false, false)
        .expect("Failed to build inventory output");
    output.sort_keys();

    let yaml =
        format_output(&output, OutputFormat::Yaml, true, true).expect("YAML formatting failed");
    insta::assert_snapshot!("reclass_inventory_yaml_sorted", normalize_uris(&yaml));
}

#[test]
fn test_reclass_nodeinfo_yaml() {
    let inventory = load_e2e_inventory();
    let merged = inventory
        .merge_node("web-prod-01")
        .expect("Failed to merge node");
    let output = NodeInfoOutput::from_node(&merged, TEST_TIMESTAMP);

    let yaml =
        format_output(&output, OutputFormat::Yaml, true, false).expect("YAML formatting failed");
    insta::assert_snapshot!("reclass_nodeinfo_yaml", normalize_uris(&yaml));
}

// --- Python compat snapshot tests ---

#[test]
fn test_reclass_python_compat_inventory_yaml_sorted() {
    let inventory = load_python_compat_inventory();
    let mut output = InventoryOutput::from_inventory(&inventory, TEST_TIMESTAMP, false, false)
        .expect("Failed to build inventory output");
    output.sort_keys();

    let yaml =
        format_output(&output, OutputFormat::Yaml, true, true).expect("YAML formatting failed");
    insta::assert_snapshot!(
        "reclass_python_compat_inventory_yaml_sorted",
        normalize_uris(&yaml)
    );
}

#[test]
fn test_reclass_python_compat_nodeinfo_yaml() {
    let inventory = load_python_compat_inventory();
    let merged = inventory
        .merge_node("reclass")
        .expect("Failed to merge node");
    let output = NodeInfoOutput::from_node(&merged, TEST_TIMESTAMP);

    let yaml =
        format_output(&output, OutputFormat::Yaml, true, false).expect("YAML formatting failed");
    insta::assert_snapshot!("reclass_python_compat_nodeinfo_yaml", normalize_uris(&yaml));
}

// --- Ansible format snapshot tests ---

#[test]
fn test_ansible_inventory_yaml() {
    let config = e2e_options();
    let inventory = ferroclass::output::ansible::build_inventory(&config, "_grp", TEST_TIMESTAMP)
        .expect("Failed to build ansible inventory");

    let yaml =
        format_output(&inventory, OutputFormat::Yaml, true, true).expect("YAML formatting failed");
    insta::assert_snapshot!("ansible_inventory_yaml", normalize_uris(&yaml));
}

#[test]
fn test_ansible_hostvars_yaml() {
    let config = e2e_options();
    let host_vars =
        ferroclass::output::ansible::build_host_vars(&config, "web-prod-01", TEST_TIMESTAMP)
            .expect("Failed to build host vars");

    let yaml =
        format_output(&host_vars, OutputFormat::Yaml, true, false).expect("YAML formatting failed");
    insta::assert_snapshot!("ansible_hostvars_yaml", normalize_uris(&yaml));
}

#[test]
fn test_ansible_hostvars_inv_query_yaml() {
    let config = e2e_options();
    let host_vars =
        ferroclass::output::ansible::build_host_vars(&config, "web-prod-02", TEST_TIMESTAMP)
            .expect("Failed to build host vars for web-prod-02");

    let yaml =
        format_output(&host_vars, OutputFormat::Yaml, true, false).expect("YAML formatting failed");
    insta::assert_snapshot!("ansible_hostvars_inv_query_yaml", normalize_uris(&yaml));
}

// --- Salt format snapshot tests ---

#[test]
fn test_salt_top_yaml() {
    let config = e2e_options();
    let top_data = ferroclass::output::salt::build_top(&config).expect("Failed to build salt top");

    let yaml =
        format_output(&top_data, OutputFormat::Yaml, true, true).expect("YAML formatting failed");
    insta::assert_snapshot!("salt_top_yaml", normalize_uris(&yaml));
}

#[test]
fn test_salt_pillar_yaml() {
    let config = e2e_options();
    let pillar_data = ferroclass::output::salt::build_pillar(&config, "web-prod-01")
        .expect("Failed to build pillar");

    let yaml = format_output(&pillar_data, OutputFormat::Yaml, true, false)
        .expect("YAML formatting failed");
    insta::assert_snapshot!("salt_pillar_yaml", normalize_uris(&yaml));
}
