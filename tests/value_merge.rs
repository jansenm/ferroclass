// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use ferroclass::inventory::options::MergeConfig;
use ferroclass::inventory::value::{Key, Value};
use ferroclass::inventory::value_merge::merge;
use hashlink::LinkedHashMap;
use std::rc::Rc;

fn default_config() -> MergeConfig {
    MergeConfig::default()
}

fn str_val(s: &str) -> Value {
    Value::String(s.to_string())
}

fn int_val(i: i64) -> Value {
    Value::Integer(i)
}

fn real_val(s: &str) -> Value {
    Value::Real(s.to_string())
}

fn bool_val(b: bool) -> Value {
    Value::Boolean(b)
}

fn null_val() -> Value {
    Value::Null
}

fn array_val(items: Vec<Value>) -> Value {
    Value::Array(Rc::new(items))
}

fn hash_val(items: Vec<(String, Value)>) -> Value {
    let mut hash: LinkedHashMap<Key, Value> = LinkedHashMap::new();
    for (k, v) in items {
        hash.insert(Key::String(k), v);
    }
    Value::Hash(Rc::new(hash))
}

#[test]
fn test_merge_two_empty_lists() {
    let config = default_config();
    let base = array_val(vec![]);
    let other = array_val(vec![]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, array_val(vec![]));
}

#[test]
fn test_merge_list_with_duplicates() {
    let config = default_config();
    let base = array_val(vec![str_val("a"), str_val("b")]);
    let other = array_val(vec![str_val("b"), str_val("c")]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(
        result,
        array_val(vec![str_val("a"), str_val("b"), str_val("b"), str_val("c")])
    );
}

#[test]
fn test_merge_empty_list_with_list() {
    let config = default_config();
    let base = array_val(vec![]);
    let other = array_val(vec![str_val("a")]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, array_val(vec![str_val("a")]));
}

#[test]
fn test_merge_list_with_empty_list() {
    let config = default_config();
    let base = array_val(vec![str_val("a")]);
    let other = array_val(vec![]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, array_val(vec![str_val("a")]));
}

#[test]
fn test_merge_two_empty_maps() {
    let config = default_config();
    let base = hash_val(vec![]);
    let other = hash_val(vec![]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, hash_val(vec![]));
}

#[test]
fn test_merge_map_with_override_key() {
    let config = default_config();
    let base = hash_val(vec![("key".to_string(), int_val(1))]);
    let other = hash_val(vec![("key".to_string(), int_val(2))]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    let expected = hash_val(vec![("key".to_string(), int_val(2))]);
    assert_eq!(result, expected);
}

#[test]
fn test_merge_map_nested_override() {
    let config = default_config();
    let base = hash_val(vec![(
        "config".to_string(),
        hash_val(vec![("enabled".to_string(), bool_val(true))]),
    )]);
    let other = hash_val(vec![(
        "config".to_string(),
        hash_val(vec![("timeout".to_string(), int_val(30))]),
    )]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    match &result {
        Value::Hash(h) => {
            let config_val = h.get(&Key::String("config".to_string()));
            assert!(config_val.is_some());
        }
        _ => panic!("Expected Hash"),
    }
}

#[test]
fn test_merge_empty_map_with_map() {
    let config = default_config();
    let base = hash_val(vec![]);
    let other = hash_val(vec![("key".to_string(), str_val("value"))]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, other);
}

#[test]
fn test_merge_map_with_empty_map() {
    let config = default_config();
    let base = hash_val(vec![("key".to_string(), str_val("value"))]);
    let other = hash_val(vec![]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, base);
}

#[test]
fn test_merge_integer_to_integer() {
    let config = default_config();
    let base = int_val(1);
    let other = int_val(2);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, int_val(2));
}

#[test]
fn test_merge_string_to_string() {
    let config = default_config();
    let base = str_val("base");
    let other = str_val("other");
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, str_val("other"));
}

#[test]
fn test_merge_boolean_to_boolean() {
    let config = default_config();
    let base = bool_val(true);
    let other = bool_val(false);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, bool_val(false));
}

#[test]
fn test_merge_real_to_real() {
    let config = default_config();
    let base = real_val("1.0");
    let other = real_val("2.5");
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, real_val("2.5"));
}

#[test]
fn test_merge_null_to_null() {
    let config = default_config();
    let base = null_val();
    let other = null_val();
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, null_val());
}

#[test]
fn test_merge_integer_to_string() {
    let config = default_config();
    let base = int_val(42);
    let other = str_val("text");
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, str_val("text"));
}

#[test]
fn test_merge_string_to_integer() {
    let config = default_config();
    let base = str_val("text");
    let other = int_val(42);
    let result = merge(&base, &other, &config, &[]).unwrap();
    assert_eq!(result, int_val(42));
}

#[test]
fn test_merge_array_to_hash_error() {
    let config = default_config();
    let base = array_val(vec![str_val("a")]);
    let other = hash_val(vec![("key".to_string(), str_val("value"))]);
    let result = merge(&base, &other, &config, &[]);
    assert!(result.is_err());
}

#[test]
fn test_merge_hash_to_array_error() {
    let config = default_config();
    let base = hash_val(vec![("key".to_string(), str_val("value"))]);
    let other = array_val(vec![str_val("a")]);
    let result = merge(&base, &other, &config, &[]);
    assert!(result.is_err());
}

#[test]
fn test_merge_array_to_string_error() {
    let config = default_config();
    let base = array_val(vec![str_val("a"), str_val("b")]);
    let other = str_val("override");
    let result = merge(&base, &other, &config, &[]);
    assert!(result.is_err());
}

#[test]
fn test_merge_mixed_list_and_string_error() {
    let config = default_config();
    let base = array_val(vec![str_val("a"), int_val(1)]);
    let other = str_val("scalar");
    let result = merge(&base, &other, &config, &[]);
    assert!(result.is_err());
}

#[test]
fn test_merge_deeply_nested_maps() {
    let config = default_config();
    let base = hash_val(vec![(
        "level1".to_string(),
        hash_val(vec![(
            "level2".to_string(),
            hash_val(vec![("value".to_string(), int_val(1))]),
        )]),
    )]);
    let other = hash_val(vec![(
        "level1".to_string(),
        hash_val(vec![(
            "level2".to_string(),
            hash_val(vec![("other".to_string(), int_val(2))]),
        )]),
    )]);
    let result = merge(&base, &other, &config, &[]).unwrap();
    match &result {
        Value::Hash(h) => {
            let level1 = h.get(&Key::String("level1".to_string()));
            assert!(level1.is_some());
        }
        _ => panic!("Expected Hash"),
    }
}

#[test]
fn test_override_with_tilde() {
    let config = MergeConfig {
        value_override_prefix: Some("~".to_string()),
        feature_value_override: true,
        ..MergeConfig::default()
    };

    // Parent: port: 80, Child: ~port: 443
    // At merge() level: port: OverrideMarker(443) (override signal preserved for interpolation)
    let base = hash_val(vec![("port".to_string(), int_val(80))]);
    let other = hash_val(vec![("~port".to_string(), int_val(443))]);
    let result = merge(&base, &other, &config, &[]).unwrap();

    match &result {
        Value::Hash(h) => {
            let port_val = h
                .get(&Key::String("port".to_string()))
                .expect("port should exist");
            assert!(
                matches!(port_val, Value::OverrideMarker(v) if **v == int_val(443)),
                "Expected OverrideMarker(443), got {:?}",
                port_val
            );
            assert!(!h.contains_key(&Key::String("~port".to_string())));
        }
        _ => panic!("Expected Hash"),
    }
}

#[test]
fn test_override_disabled() {
    let config = MergeConfig::disabled();

    // Parent: port: 80, Child: ~port: 443
    // Result: port: 80, ~port: 443 (both keys)
    let base = hash_val(vec![("port".to_string(), int_val(80))]);
    let other = hash_val(vec![("~port".to_string(), int_val(443))]);
    let result = merge(&base, &other, &config, &[]).unwrap();

    match &result {
        Value::Hash(h) => {
            assert_eq!(h.get(&Key::String("port".to_string())), Some(&int_val(80)));
            assert_eq!(
                h.get(&Key::String("~port".to_string())),
                Some(&int_val(443))
            );
        }
        _ => panic!("Expected Hash"),
    }
}
