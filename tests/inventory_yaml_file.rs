// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::options::{ParameterKeyStyle, StorageOptions, StorageType};
use ferroclass::inventory::value::{Key, Value};
use ferroclass::inventory::{load, load_from_yaml_string};
use indoc::indoc;
use snafu::Report;
use std::path::PathBuf;

#[test]
fn test_invalid_parameter_key_ansible_style() {
    const TEST_INVENTORY: &str = indoc!(
        r#"
    ---
    ---
    name: classA
    parameters:
        valid_key: value
        invalid-key: value
    "#
    );

    let error = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
        .expect_err("Expected an error for invalid key");
    assert!(
        Report::from_error(error)
            .to_string()
            .contains("Invalid parameter key")
    );
}

#[test]
fn test_valid_parameter_key_ansible_style() {
    const TEST_INVENTORY: &str = indoc!(
        r#"
    ---
    ---
    name: classA
    parameters:
        valid_key: value
        another_valid_key123: value
    "#
    );

    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
        .expect("Should accept valid keys");
    assert!(inventory.get_class("classA").is_some());
}

const TEST_INVENTORY: &str = r#"---
classes_uri: classes
nodes_uri: nodes
---
name: domain.local
parameters:
  domain: example.com
---
name: linux.distro.tumbleweed
classes:
  - all
parameters:
  os: linux
  package_manager: zypper
---
name: all
environment: production
parameters:
  timezone: UTC
---
name: laptop
type: node
classes:
  - domain.local
  - linux.distro.tumbleweed
parameters:
  hostname: laptop
  system:
    is_server: false
    cpu_count: 8
    memory_gb: 16.5
---
name: desktop
type: node
classes:
  - domain.local
  - linux.distro.tumbleweed
parameters:
  hostname: desktop
"#;

#[test]
fn test_get_class_by_name() {
    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::default())
        .expect("Failed to load inventory");

    let class = inventory.get_class("all");
    assert!(class.is_some(), "Expected to find class 'all'");
    assert_eq!(class.unwrap().name(), "all");

    let class = inventory.get_class("linux.distro.tumbleweed");
    assert!(
        class.is_some(),
        "Expected to find class 'linux.distro.tumbleweed'"
    );
    assert_eq!(class.unwrap().name(), "linux.distro.tumbleweed");

    let class = inventory.get_class("nonexistent");
    assert!(class.is_none(), "Expected not to find class 'nonexistent'");
}

#[test]
fn test_get_node_by_name() {
    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::default())
        .expect("Failed to load inventory");

    let node = inventory.get_node("laptop");
    assert!(node.is_some(), "Expected to find node 'laptop'");
    assert_eq!(node.unwrap().name(), "laptop");

    let node = inventory.get_node("nonexistent");
    assert!(node.is_none(), "Expected not to find node 'nonexistent'");
}

#[test]
fn test_merge_class_simple() {
    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::default())
        .expect("Failed to load inventory");

    let merged = inventory
        .merge_class("all")
        .expect("Failed to merge class 'all'");
    assert_eq!(merged.name(), "all");
    assert_eq!(
        merged
            .parameters()
            .get(&Key::String("timezone".to_string())),
        Some(&Value::String("UTC".to_string()))
    );
}

#[test]
fn test_merge_class_with_inheritance() {
    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::default())
        .expect("Failed to load inventory");

    let merged = inventory
        .merge_class("linux.distro.tumbleweed")
        .expect("Failed to merge class");
    assert_eq!(merged.name(), "linux.distro.tumbleweed");
    assert_eq!(
        merged
            .parameters()
            .get(&Key::String("timezone".to_string())),
        Some(&Value::String("UTC".to_string())),
        "should inherit timezone from 'all'"
    );
    assert_eq!(
        merged.parameters().get(&Key::String("os".to_string())),
        Some(&Value::String("linux".to_string())),
        "should have own 'os' parameter"
    );
    assert_eq!(
        merged
            .parameters()
            .get(&Key::String("package_manager".to_string())),
        Some(&Value::String("zypper".to_string())),
        "should have own 'package_manager' parameter"
    );
}

#[test]
fn test_merge_node_simple() {
    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::default())
        .expect("Failed to load inventory");

    let node = inventory
        .merge_node("laptop")
        .expect("Failed to merge node 'laptop'");

    assert_eq!(
        node.parameters().get(&Key::String("hostname".to_string())),
        Some(&Value::String("laptop".to_string())),
        "should have node's own hostname"
    );
    assert_eq!(
        node.parameters().get(&Key::String("timezone".to_string())),
        Some(&Value::String("UTC".to_string())),
        "should inherit timezone from 'all'"
    );
    assert_eq!(
        node.parameters().get(&Key::String("os".to_string())),
        Some(&Value::String("linux".to_string())),
        "should inherit os from 'linux.distro.tumbleweed'"
    );
    assert_eq!(
        node.parameters().get(&Key::String("domain".to_string())),
        Some(&Value::String("example.com".to_string())),
        "should inherit domain from 'domain.local'"
    );
}

#[test]
fn test_class_not_found() {
    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::default())
        .expect("Failed to load inventory");

    let result = inventory.merge_class("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_node_not_found() {
    let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::default())
        .expect("Failed to load inventory");

    let result = inventory.merge_node("nonexistent");
    assert!(result.is_err());
}

#[test]
fn test_load_inventory_yaml_file() {
    let inventory_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("inventories")
        .join("example_file")
        .join("inventory.yml");

    let storage_options = StorageOptions {
        storage_type: StorageType::YamlFile,
        yaml_file_options: ferroclass::inventory::options::YamlFileStorageOptions {
            inventory_file: inventory_path.to_string_lossy().to_string(),
            parameter_key_style: ParameterKeyStyle::default(),
            ..ferroclass::inventory::options::YamlFileStorageOptions::default()
        },
        ..StorageOptions::default()
    };

    let inventory = load(&storage_options).expect("Failed to load inventory");

    let classes: Vec<_> = inventory.classes_iter().collect();
    assert_eq!(classes.len(), 2, "Expected 2 classes");

    let class_names: Vec<_> = classes.iter().map(|c| c.name()).collect();
    assert!(
        class_names.iter().any(|n| *n == "all"),
        "Expected class 'all', got {:?}",
        class_names
    );
    assert!(
        class_names.iter().any(|n| *n == "linux.distro.tumbleweed"),
        "Expected class 'linux.distro.tumbleweed', got {:?}",
        class_names
    );

    let nodes: Vec<_> = inventory.nodes_iter().collect();
    assert_eq!(nodes.len(), 2, "Expected 2 nodes");

    let node_names: Vec<_> = nodes.iter().map(|n| n.name()).collect();
    assert!(
        node_names.iter().any(|n| *n == "laptop"),
        "Expected node 'laptop', got {:?}",
        node_names
    );
    assert!(
        node_names.iter().any(|n| *n == "desktop"),
        "Expected node 'desktop', got {:?}",
        node_names
    );

    let laptop = inventory
        .get_node("laptop")
        .expect("Expected to find laptop node");
    let params = laptop.parameters();

    assert_eq!(
        params.get(&Key::String("hostname".to_string())),
        Some(&Value::String("laptop".to_string())),
        "Expected hostname parameter"
    );
}

#[test]
fn test_invalid_definition_unknown_key() {
    const TEST_INVENTORY: &str = indoc!(
        r#"
    ---
    ---
    name: classA
    parameters:
        a: b
    unknown_key:
        - 1
    "#
    );

    let error = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::default())
        .expect_err("Expected an error");
    assert!(
        Report::from_error(error)
            .to_string()
            .contains("unknown_key")
    );
}
