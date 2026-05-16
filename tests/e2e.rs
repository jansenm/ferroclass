// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::load;
use ferroclass::inventory::options::{
    ParameterKeyStyle, StorageOptions, StorageType, YamlFsStorageOptions,
};
use ferroclass::inventory::value::{Key, Value};
use std::path::PathBuf;

fn e2e_inventory_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("inventories")
        .join("e2e")
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

fn str_key(s: &str) -> Key {
    Key::String(s.to_string())
}

#[test]
fn test_e2e_load_all_nodes_and_classes() {
    let inventory = load_e2e_inventory();

    let node_names: Vec<_> = inventory.node_names();
    assert!(
        node_names.len() >= 4,
        "Expected at least 4 nodes, got {:?}",
        node_names
    );

    let class_names: Vec<_> = inventory.classes_iter().map(|c| c.name()).collect();
    assert!(
        class_names.iter().any(|n| *n == "all"),
        "Expected class 'all', got {:?}",
        class_names
    );
}

#[test]
fn test_e2e_merge_web_prod_01() {
    let inventory = load_e2e_inventory();

    let node = inventory
        .merge_node("web-prod-01")
        .expect("Failed to merge web-prod-01");

    let params = node.parameters();

    assert_eq!(
        params.get(&str_key("hostname")),
        Some(&Value::String("web-prod-01".to_string())),
        "hostname should be web-prod-01"
    );

    assert_eq!(
        params.get(&str_key("ip")),
        Some(&Value::String("10.0.1.10".to_string())),
        "ip should be 10.0.1.10"
    );

    assert_eq!(
        params.get(&str_key("environment_name")),
        Some(&Value::String("production".to_string())),
        "environment_name should be production (from env.production class)"
    );

    assert_eq!(
        params.get(&str_key("role")),
        Some(&Value::String("web".to_string())),
        "role should be web (from role.web class)"
    );

    let global = params
        .get(&str_key("global"))
        .expect("global should exist (from all class)");
    match global {
        Value::Hash(h) => {
            assert!(
                h.contains_key(&str_key("timezone")),
                "global should contain timezone"
            );
            assert!(
                h.contains_key(&str_key("admin")),
                "global should contain admin"
            );
        }
        _ => panic!("global should be a Hash, got {:?}", global),
    }

    let webserver = params
        .get(&str_key("webserver"))
        .expect("webserver should exist (from role.web class)");
    match webserver {
        Value::Hash(h) => {
            assert_eq!(
                h.get(&str_key("port")),
                Some(&Value::Integer(80)),
                "webserver.port should be 80"
            );
        }
        _ => panic!("webserver should be a Hash, got {:?}", webserver),
    }
}

#[test]
fn test_e2e_merge_db_prod_01() {
    let inventory = load_e2e_inventory();

    let node = inventory
        .merge_node("db-prod-01")
        .expect("Failed to merge db-prod-01");

    let params = node.parameters();

    assert_eq!(
        params.get(&str_key("hostname")),
        Some(&Value::String("db-prod-01".to_string())),
        "hostname should be db-prod-01"
    );

    assert_eq!(
        params.get(&str_key("environment_name")),
        Some(&Value::String("production".to_string())),
        "environment_name should be production"
    );

    assert_eq!(
        params.get(&str_key("role")),
        Some(&Value::String("db".to_string())),
        "role should be db"
    );

    let database = params
        .get(&str_key("database"))
        .expect("database should exist");
    match database {
        Value::Hash(h) => {
            assert_eq!(
                h.get(&str_key("engine")),
                Some(&Value::String("postgresql".to_string())),
                "database.engine should be postgresql"
            );
        }
        _ => panic!("database should be a Hash, got {:?}", database),
    }
}

#[test]
fn test_e2e_merge_app_staging_01() {
    let inventory = load_e2e_inventory();

    let node = inventory
        .merge_node("app-staging-01")
        .expect("Failed to merge app-staging-01");

    let params = node.parameters();

    assert_eq!(
        params.get(&str_key("hostname")),
        Some(&Value::String("app-staging-01".to_string())),
        "hostname should be app-staging-01"
    );

    assert_eq!(
        params.get(&str_key("environment_name")),
        Some(&Value::String("staging".to_string())),
        "environment_name should be staging"
    );

    assert_eq!(
        params.get(&str_key("role")),
        Some(&Value::String("web".to_string())),
        "role should be web (from role.web via role.app)"
    );

    let app = params
        .get(&str_key("app"))
        .expect("app should exist (from role.app class)");
    match app {
        Value::Hash(h) => {
            assert_eq!(
                h.get(&str_key("name")),
                Some(&Value::String("myapp".to_string())),
                "app.name should be myapp"
            );
            assert_eq!(
                h.get(&str_key("version")),
                Some(&Value::String("1.0".to_string())),
                "app.version should be 1.0"
            );
        }
        _ => panic!("app should be a Hash, got {:?}", app),
    }

    let monitor_endpoint = params.get(&str_key("monitor_endpoint"));
    match monitor_endpoint {
        Some(Value::String(s)) => {
            assert!(
                s.contains("app-staging-01"),
                "monitor_endpoint should contain hostname, got {}",
                s
            );
            assert!(
                s.contains("80"),
                "monitor_endpoint should contain webserver port 80, got {}",
                s
            );
        }
        Some(v) => panic!("monitor_endpoint should be a String, got {:?}", v),
        None => panic!("monitor_endpoint should exist"),
    }
}

#[test]
fn test_e2e_exports() {
    let inventory = load_e2e_inventory();

    let node = inventory
        .merge_node("web-prod-01")
        .expect("Failed to merge web-prod-01");

    let exports = node.exports();
    assert_eq!(
        exports.get(&str_key("ip")),
        Some(&Value::String("10.0.1.10".to_string())),
        "exports.ip should be 10.0.1.10"
    );
    assert_eq!(
        exports.get(&str_key("role")),
        Some(&Value::String("web".to_string())),
        "exports.role should be web"
    );
}

#[test]
fn test_e2e_node_names() {
    let inventory = load_e2e_inventory();
    let names = inventory.node_names();

    assert!(
        names.contains(&"web-prod-01".to_string()),
        "node_names should contain web-prod-01"
    );
    assert!(
        names.contains(&"db-prod-01".to_string()),
        "node_names should contain db-prod-01"
    );
    assert!(
        names.contains(&"app-staging-01".to_string()),
        "node_names should contain app-staging-01"
    );
    assert!(
        names.contains(&"web-prod-02".to_string()),
        "node_names should contain web-prod-02"
    );
}

#[test]
fn test_e2e_two_pass_inv_query() {
    let inventory = load_e2e_inventory();

    let inv_map = inventory
        .build_inventory_map()
        .expect("Failed to build inventory map");

    let node = inventory
        .merge_node_with_inventory("web-prod-02", &inv_map)
        .expect("Failed to merge web-prod-02 with inventory");

    let params = node.parameters();

    let peers = params
        .get(&str_key("peers"))
        .expect("peers should exist after inv query resolution");

    match peers {
        Value::Hash(h) => {
            assert!(
                !h.is_empty(),
                "peers should have at least 1 web node with ip export, got {:?}",
                h
            );
        }
        Value::Array(arr) => {
            assert!(
                !arr.is_empty(),
                "peers list should not be empty, got {:?}",
                arr
            );
        }
        _ => panic!(
            "peers should be a Hash or Array after inv query resolution, got {:?}",
            peers
        ),
    }
}

#[test]
fn test_e2e_class_inheritance_chain() {
    let inventory = load_e2e_inventory();

    let node = inventory
        .merge_node("app-staging-01")
        .expect("Failed to merge app-staging-01");

    let classes = node.classes();
    assert!(
        classes.contains(&"all".to_string()),
        "should inherit 'all' class, got {:?}",
        classes
    );
    assert!(
        classes.contains(&"env.staging".to_string()),
        "should inherit 'env.staging' class, got {:?}",
        classes
    );
    assert!(
        classes.contains(&"role.web".to_string()),
        "should inherit 'role.web' class, got {:?}",
        classes
    );
    assert!(
        classes.contains(&"role.app".to_string()),
        "should inherit 'role.app' class, got {:?}",
        classes
    );
}

#[test]
fn test_e2e_deep_merge_parameters() {
    let inventory = load_e2e_inventory();

    let node = inventory
        .merge_node("web-prod-01")
        .expect("Failed to merge web-prod-01");

    let params = node.parameters();

    let global = params.get(&str_key("global")).expect("global should exist");
    match global {
        Value::Hash(h) => {
            let admin = h.get(&str_key("admin")).expect("global.admin should exist");
            match admin {
                Value::Hash(admin_h) => {
                    assert_eq!(
                        admin_h.get(&str_key("email")),
                        Some(&Value::String("admin@example.com".to_string())),
                        "admin.email should be admin@example.com"
                    );
                }
                _ => panic!("admin should be a Hash, got {:?}", admin),
            }
            assert_eq!(
                h.get(&str_key("timezone")),
                Some(&Value::String("UTC".to_string())),
                "global.timezone should be UTC"
            );
        }
        _ => panic!("global should be a Hash, got {:?}", global),
    }
}

#[test]
fn test_e2e_node_not_found() {
    let inventory = load_e2e_inventory();
    let result = inventory.merge_node("nonexistent-node");
    assert!(
        result.is_err(),
        "should error on nonexistent node, got {:?}",
        result
    );
}

// --- Python reclass compatibility tests ---
// These tests use the same inventory data as the Python reclass test/model/default fixture
// to verify Rust produces compatible results.

fn python_compat_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("inventories")
        .join("python_compat")
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

#[test]
fn test_python_compat_interpolation() {
    let inventory = load_python_compat_inventory();

    let node = inventory
        .merge_node("reclass")
        .expect("Failed to merge reclass node");

    let params = node.parameters();

    assert_eq!(
        params.get(&str_key("myparam")),
        Some(&Value::String("param".to_string())),
        "myparam should interpolate from _param.some to 'param'"
    );

    let colour_unescaped = params.get(&str_key("colour")).and_then(|v| match v {
        Value::Hash(h) => h.get(&str_key("unescaped")),
        _ => None,
    });
    assert_eq!(
        colour_unescaped,
        Some(&Value::String("red".to_string())),
        "colour.unescaped should resolve _param:colour to 'red'"
    );
}

#[test]
fn test_python_compat_tilde_override() {
    let inventory = load_python_compat_inventory();

    let node = inventory
        .merge_node("reclass")
        .expect("Failed to merge reclass node");

    let params = node.parameters();

    let list_to_override = params.get(&str_key("list_to_override"));
    match list_to_override {
        Some(Value::Array(arr)) => {
            assert!(
                arr.is_empty(),
                "list_to_override should be empty after ~override with empty list, got {:?}",
                arr
            );
        }
        _ => panic!(
            "list_to_override should be an empty Array after override, got {:?}",
            list_to_override
        ),
    }

    let dict_to_override = params.get(&str_key("dict_to_override"));
    match dict_to_override {
        Some(Value::Hash(h)) => {
            assert!(
                h.is_empty(),
                "dict_to_override should be empty after ~override with empty dict, got {:?}",
                h
            );
        }
        _ => panic!(
            "dict_to_override should be an empty Hash after override, got {:?}",
            dict_to_override
        ),
    }
}

#[test]
fn test_python_compat_deep_merge() {
    let inventory = load_python_compat_inventory();

    let node = inventory
        .merge_node("reclass")
        .expect("Failed to merge reclass node");

    let params = node.parameters();

    let one = params.get(&str_key("one"));
    match one {
        Some(Value::Hash(h)) => {
            assert_eq!(
                h.get(&str_key("a")),
                Some(&Value::Integer(1)),
                "one.a should be 1"
            );
            assert_eq!(
                h.get(&str_key("b")),
                Some(&Value::Integer(2)),
                "one.b should be 2"
            );
        }
        _ => panic!("one should be a Hash, got {:?}", one),
    }

    let two = params.get(&str_key("two"));
    match two {
        Some(Value::Hash(h)) => {
            assert_eq!(
                h.get(&str_key("c")),
                Some(&Value::Integer(3)),
                "two.c should be 3"
            );
            assert_eq!(
                h.get(&str_key("d")),
                Some(&Value::Integer(4)),
                "two.d should be 4"
            );
        }
        _ => panic!("two should be a Hash, got {:?}", two),
    }
}

#[test]
fn test_python_compat_numeric_keys() {
    let inventory = load_python_compat_inventory();

    let node = inventory
        .merge_node("reclass")
        .expect("Failed to merge reclass node");

    let params = node.parameters();

    assert!(
        params.contains_key(&Key::from(1i64)),
        "params should have integer key 1"
    );
    assert!(
        params.contains_key(&Key::from(2i64)),
        "params should have integer key 2"
    );
    assert!(
        params.contains_key(&Key::from(3i64)),
        "params should have integer key 3"
    );
}

#[test]
fn test_python_compat_ref_chain() {
    let inventory = load_python_compat_inventory();

    let node = inventory
        .merge_node("reclass")
        .expect("Failed to merge reclass node");

    let params = node.parameters();

    let three = params.get(&str_key("three"));
    match three {
        Some(Value::Hash(h)) => {
            assert_eq!(
                h.get(&str_key("e")),
                Some(&Value::Integer(5)),
                "three.e should be 5 (from third class)"
            );
        }
        _ => panic!("three should be a Hash, got {:?}", three),
    }
}

#[test]
fn test_python_compat_will_resolve() {
    let inventory = load_python_compat_inventory();

    let node = inventory
        .merge_node("reclass")
        .expect("Failed to merge reclass node");

    let params = node.parameters();

    let tree = params
        .get(&str_key("will"))
        .and_then(|v| match v {
            Value::Hash(h) => h.get(&str_key("not")),
            _ => None,
        })
        .and_then(|v| match v {
            Value::Hash(h) => h.get(&str_key("fail")),
            _ => None,
        })
        .and_then(|v| match v {
            Value::Hash(h) => h.get(&str_key("at")),
            _ => None,
        });
    match tree {
        Some(Value::Hash(h)) => {
            assert_eq!(
                h.get(&str_key("tree")),
                Some(&Value::String("exist".to_string())),
                "will.not.fail.at.tree should resolve to 'exist'"
            );
        }
        _ => panic!("will.not.fail.at should be a Hash, got {:?}", tree),
    }
}

#[test]
fn test_python_compat_escaped_interpolation() {
    let inventory = load_python_compat_inventory();

    let node = inventory
        .merge_node("reclass")
        .expect("Failed to merge reclass node");

    let params = node.parameters();

    let colour = params.get(&str_key("colour")).expect("colour should exist");

    match colour {
        Value::Hash(h) => {
            let escaped = h.get(&str_key("escaped"));
            match escaped {
                Some(Value::String(s)) => {
                    assert!(
                        s.contains("${_param:colour}") || s.contains("$"),
                        "escaped should contain literal dollar-braces, got {}",
                        s
                    );
                }
                Some(v) => panic!("escaped should be a String, got {:?}", v),
                None => panic!("escaped key should exist"),
            }

            let doubleescaped = h.get(&str_key("doubleescaped"));
            match doubleescaped {
                Some(Value::String(s)) => {
                    assert!(
                        s.contains("\\") || s.contains("$"),
                        "doubleescaped should contain backslash or dollar, got {}",
                        s
                    );
                }
                Some(v) => panic!("doubleescaped should be a String, got {:?}", v),
                None => panic!("doubleescaped key should exist"),
            }
        }
        _ => panic!("colour should be a Hash, got {:?}", colour),
    }
}
