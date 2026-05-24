// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::load;
use ferroclass::inventory::options::{
    ParameterKeyStyle, StorageOptions, StorageType, YamlFsStorageOptions,
};
use ferroclass::inventory::value::{Key, Value};
use std::path::PathBuf;

#[test]
fn test_load_nodes_and_classes_from_yaml_fs_repo() {
    let inventory_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("inventories")
        .join("example");

    let storage_options = StorageOptions {
        storage_type: StorageType::YamlFs,
        yaml_fs_options: YamlFsStorageOptions {
            inventory_base_uri: inventory_path.to_string_lossy().to_string(),
            nodes_uri: "nodes".to_string(),
            classes_uri: "classes".to_string(),
            parameter_key_style: ParameterKeyStyle::default(),
            compose_node_name: false,
            ..YamlFsStorageOptions::default()
        },
        ..StorageOptions::default()
    };

    let inventory = load(&storage_options).expect("Failed to load inventory");

    let classes: Vec<_> = inventory.classes_iter().collect();
    assert!(
        classes.len() >= 2,
        "Expected at least 2 classes, got {}",
        classes.len()
    );

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
    assert!(!nodes.is_empty(), "Expected at least 1 node");

    let node_names: Vec<_> = nodes.iter().map(|n| n.name()).collect();
    assert!(
        node_names.iter().any(|n| *n == "laptop"),
        "Expected node 'laptop', got {:?}",
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

    assert_eq!(
        params.get(&Key::String("system".to_string())),
        Some(&Value::Hash(std::rc::Rc::new({
            let mut hash = hashlink::LinkedHashMap::new();
            hash.insert(Key::String("is_server".to_string()), Value::Boolean(false));
            hash.insert(Key::String("cpu_count".to_string()), Value::Integer(8));
            hash.insert(
                Key::String("memory_gb".to_string()),
                Value::Real("16.5".to_string()),
            );
            hash
        }))),
        "Expected system parameters with boolean, integer, and real values"
    );
}
