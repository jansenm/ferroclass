// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::load;
use ferroclass::inventory::options::{ParameterKeyStyle, StorageOptions, StorageType};
use ferroclass::inventory::value::{Key, Value};
use std::path::PathBuf;

fn load_test_inventory() -> ferroclass::inventory::Inventory {
    let inventory_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("inventories")
        .join("example");

    let storage_options = StorageOptions {
        storage_type: StorageType::YamlFs,
        yaml_fs_options: ferroclass::inventory::options::YamlFsStorageOptions {
            inventory_base_uri: inventory_path.to_string_lossy().to_string(),
            nodes_uri: "nodes".to_string(),
            classes_uri: "classes".to_string(),
            parameter_key_style: ParameterKeyStyle::default(),
            compose_node_name: false,
            ..ferroclass::inventory::options::YamlFsStorageOptions::default()
        },
        ..StorageOptions::default()
    };

    load(&storage_options).expect("Failed to load inventory")
}

fn str_key(s: &str) -> Key {
    Key::String(s.to_string())
}

#[test]
fn test_merge_class_all() {
    let inventory = load_test_inventory();
    let result = inventory.merge_class("all").expect("Failed to merge class");

    let params = result.parameters();
    let global = params.get(&str_key("global"));
    assert!(global.is_some(), "Expected 'global' key in merged class");
}

#[test]
fn test_merge_class_with_inheritance() {
    let inventory = load_test_inventory();
    let result = inventory
        .merge_class("linux.distro.tumbleweed")
        .expect("Failed to merge class");

    let params = result.parameters();
    let global = params.get(&str_key("global"));
    assert!(global.is_some(), "Expected 'global' from 'all' class");

    let os = params.get(&str_key("os"));
    assert!(
        os.is_some(),
        "Expected 'os' from 'linux.distro.tumbleweed' class"
    );
}

#[test]
fn test_merge_node_laptop() {
    let inventory = load_test_inventory();
    let result = inventory
        .merge_node("laptop")
        .expect("Failed to merge node");

    let params = result.parameters();
    let hostname = params.get(&str_key("hostname"));
    assert!(hostname.is_some(), "Expected 'hostname' parameter");
    assert_eq!(hostname.unwrap(), &Value::String("laptop".to_string()));

    let system = params.get(&str_key("system"));
    assert!(system.is_some(), "Expected 'system' parameter");

    let global = params.get(&str_key("global"));
    assert!(global.is_some(), "Expected 'global' from inherited class");

    let os = params.get(&str_key("os"));
    assert!(os.is_some(), "Expected 'os' from inherited class");
}

#[test]
fn test_merge_node_desktop() {
    let inventory = load_test_inventory();
    let result = inventory
        .merge_node("desktop")
        .expect("Failed to merge node");

    let params = result.parameters();
    let hostname = params.get(&str_key("hostname"));
    assert!(hostname.is_some(), "Expected 'hostname' parameter");
    assert_eq!(hostname.unwrap(), &Value::String("desktop".to_string()));
}

#[test]
fn test_merge_class_not_found() {
    let inventory = load_test_inventory();
    let result = inventory.merge_class("nonexistent");
    assert!(result.is_err(), "Expected error for nonexistent class");
}

#[test]
fn test_merge_node_not_found() {
    let inventory = load_test_inventory();
    let result = inventory.merge_node("nonexistent");
    assert!(result.is_err(), "Expected error for nonexistent node");
}

#[test]
fn test_merge_preserves_parameter_order() {
    let inventory = load_test_inventory();
    let result = inventory
        .merge_node("laptop")
        .expect("Failed to merge node");

    let params = result.parameters();
    assert!(params.contains_key(&str_key("hostname")));
    assert!(params.contains_key(&str_key("system")));
    assert!(params.contains_key(&str_key("global")));
    assert!(params.contains_key(&str_key("os")));
    assert!(params.contains_key(&str_key("domain")));
}
