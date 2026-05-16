// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::options::MergeConfig;
use crate::inventory::value::{Hash, Key, Value};
use snafu::Snafu;
use std::rc::Rc;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Cannot merge {new_type} over {existing_type}{context}"))]
    TypeMerge {
        new_type: &'static str,
        existing_type: &'static str,
        context: String,
    },
}

fn format_path(path: &[String]) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!(", at {}", path.join(":"))
    }
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Hash(_) => "dictionary",
        Value::Array(_) => "list",
        Value::String(_)
        | Value::Integer(_)
        | Value::Boolean(_)
        | Value::Real(_)
        | Value::Null
        | Value::Reference(_)
        | Value::StringWithReference(_)
        | Value::InvQuery(_)
        | Value::StringWithInvQuery(_) => "scalar",
        Value::DeferredMerge(_) => "deferred_merge",
        Value::OverrideMarker(_) => "override",
        Value::ConstantMarker(_) => "constant",
    }
}

fn is_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::String(_)
            | Value::Integer(_)
            | Value::Boolean(_)
            | Value::Real(_)
            | Value::Null
            | Value::Reference(_)
            | Value::StringWithReference(_)
            | Value::InvQuery(_)
            | Value::StringWithInvQuery(_)
    )
}

pub fn merge(
    base: &Value,
    other: &Value,
    config: &MergeConfig,
    path: &[String],
) -> Result<Value, Error> {
    match (base, other) {
        (Value::Array(base_arr), Value::Array(other_arr)) => {
            if other_arr.is_empty() {
                return Ok(base.clone());
            }
            if base_arr.is_empty() {
                return Ok(other.clone());
            }

            let mut result = (**base_arr).clone();
            result.extend(other_arr.iter().cloned());
            Ok(Value::Array(Rc::new(result)))
        }
        (Value::Hash(base_hash), Value::Hash(other_hash)) => {
            merge_hash(base_hash, other_hash, config, path)
        }
        (Value::DeferredMerge(base_values), Value::DeferredMerge(other_values)) => {
            let mut result = (**base_values).clone();
            result.extend(other_values.iter().cloned());
            Ok(Value::DeferredMerge(Rc::new(result)))
        }
        (Value::DeferredMerge(base_values), _) => {
            let mut result = (**base_values).clone();
            result.push(other.clone());
            Ok(Value::DeferredMerge(Rc::new(result)))
        }
        (_, Value::DeferredMerge(other_values)) => {
            let mut result = vec![base.clone()];
            result.extend(other_values.iter().cloned());
            Ok(Value::DeferredMerge(Rc::new(result)))
        }
        (Value::Reference(_), _) | (_, Value::Reference(_)) => {
            Ok(Value::DeferredMerge(Rc::new(vec![
                base.clone(),
                other.clone(),
            ])))
        }
        (Value::InvQuery(_), _) | (_, Value::InvQuery(_)) => {
            Ok(Value::DeferredMerge(Rc::new(vec![
                base.clone(),
                other.clone(),
            ])))
        }
        (Value::StringWithInvQuery(_), _) | (_, Value::StringWithInvQuery(_)) => {
            Ok(Value::DeferredMerge(Rc::new(vec![
                base.clone(),
                other.clone(),
            ])))
        }
        (Value::OverrideMarker(_), _) => Ok(Value::DeferredMerge(Rc::new(vec![
            base.clone(),
            other.clone(),
        ]))),
        (_, Value::OverrideMarker(_)) => Ok(Value::DeferredMerge(Rc::new(vec![
            base.clone(),
            other.clone(),
        ]))),
        (Value::ConstantMarker(_), _) => Ok(Value::DeferredMerge(Rc::new(vec![
            base.clone(),
            other.clone(),
        ]))),
        (_, Value::ConstantMarker(_)) => Ok(Value::DeferredMerge(Rc::new(vec![
            base.clone(),
            other.clone(),
        ]))),
        (Value::Hash(_), new) if is_scalar(new) => {
            if config.allow_none_override && matches!(new, Value::Null) {
                return Ok(new.clone());
            }
            Err(Error::TypeMerge {
                new_type: value_type_name(new),
                existing_type: "dictionary",
                context: format_path(path),
            })
        }
        (Value::Array(_), new) if is_scalar(new) => {
            if config.allow_none_override && matches!(new, Value::Null) {
                return Ok(new.clone());
            }
            Err(Error::TypeMerge {
                new_type: value_type_name(new),
                existing_type: "list",
                context: format_path(path),
            })
        }
        (Value::Hash(_), Value::Array(_)) => Err(Error::TypeMerge {
            new_type: "list",
            existing_type: "dictionary",
            context: format_path(path),
        }),
        (Value::Array(_), Value::Hash(_)) => Err(Error::TypeMerge {
            new_type: "dictionary",
            existing_type: "list",
            context: format_path(path),
        }),
        _ => Ok(other.clone()),
    }
}

fn merge_hash(
    base: &Hash,
    other: &Hash,
    config: &MergeConfig,
    path: &[String],
) -> Result<Value, Error> {
    if other.is_empty() {
        return Ok(Value::Hash(Rc::new(base.clone())));
    }
    if base.is_empty() {
        if config.feature_value_override || config.feature_value_constant {
            let processed = process_special_keys(other, config);
            Ok(Value::Hash(Rc::new(processed)))
        } else {
            Ok(Value::Hash(Rc::new(other.clone())))
        }
    } else {
        let mut result = base.clone();
        for (key, value) in other.iter() {
            let (effective_key, action) = resolve_key(key, config);
            let key_str = match &effective_key {
                Key::String(s) => s.clone(),
                Key::Integer(i) => i.to_string(),
                Key::Null => "null".to_string(),
                Key::Boolean(b) => b.to_string(),
            };
            let mut child_path = path.to_vec();
            child_path.push(key_str);
            match action {
                KeyAction::Override => {
                    result.remove(&effective_key);
                    let override_value: Value = match value {
                        Value::Hash(hash) => {
                            merge_hash(&Hash::default(), hash, config, &child_path)?
                        }
                        _ => value.clone(),
                    };
                    result.insert(
                        effective_key,
                        Value::OverrideMarker(Rc::new(override_value)),
                    );
                }
                KeyAction::Constant => {
                    result.remove(&effective_key);
                    let constant_value: Value = match value {
                        Value::Hash(hash) => {
                            merge_hash(&Hash::default(), hash, config, &child_path)?
                        }
                        _ => value.clone(),
                    };
                    result.insert(
                        effective_key,
                        Value::ConstantMarker(Rc::new(constant_value)),
                    );
                }
                KeyAction::Normal => {
                    if let Some(existing) = result.get(&effective_key) {
                        let merged = merge(existing, value, config, &child_path)?;
                        result.insert(effective_key, merged);
                    } else {
                        result.insert(effective_key, value.clone());
                    }
                }
            }
        }
        Ok(Value::Hash(Rc::new(result)))
    }
}

pub(crate) fn merge_hash_direct(
    base: &Hash,
    other: &Hash,
    config: &MergeConfig,
) -> Result<Hash, Error> {
    if other.is_empty() {
        return Ok(base.clone());
    }
    if base.is_empty() {
        if config.feature_value_override || config.feature_value_constant {
            let processed = process_special_keys(other, config);
            Ok(processed)
        } else {
            Ok(other.clone())
        }
    } else {
        let mut result = base.clone();
        for (key, value) in other.iter() {
            let (effective_key, action) = resolve_key(key, config);
            let key_str = match &effective_key {
                Key::String(s) => s.clone(),
                Key::Integer(i) => i.to_string(),
                Key::Null => "null".to_string(),
                Key::Boolean(b) => b.to_string(),
            };
            let child_path = vec![key_str];
            match action {
                KeyAction::Override => {
                    result.remove(&effective_key);
                    let override_value: Value = match value {
                        Value::Hash(hash) => {
                            merge_hash(&Hash::default(), hash, config, &child_path)?
                        }
                        _ => value.clone(),
                    };
                    result.insert(
                        effective_key,
                        Value::OverrideMarker(Rc::new(override_value)),
                    );
                }
                KeyAction::Constant => {
                    result.remove(&effective_key);
                    let constant_value: Value = match value {
                        Value::Hash(hash) => {
                            merge_hash(&Hash::default(), hash, config, &child_path)?
                        }
                        _ => value.clone(),
                    };
                    result.insert(
                        effective_key,
                        Value::ConstantMarker(Rc::new(constant_value)),
                    );
                }
                KeyAction::Normal => {
                    if let Some(existing) = result.get(&effective_key) {
                        let merged = merge(existing, value, config, &child_path)?;
                        result.insert(effective_key, merged);
                    } else {
                        result.insert(effective_key, value.clone());
                    }
                }
            }
        }
        Ok(result)
    }
}

enum KeyAction {
    Normal,
    Override,
    Constant,
}

fn resolve_key(key: &Key, config: &MergeConfig) -> (Key, KeyAction) {
    if let Key::String(k) = key {
        if let Some(ref prefix) = config.value_override_prefix
            && config.feature_value_override
            && let Some(stripped) = k.strip_prefix(prefix)
        {
            return (Key::String(stripped.to_string()), KeyAction::Override);
        }
        if let Some(ref prefix) = config.value_constant_prefix
            && config.feature_value_constant
            && let Some(stripped) = k.strip_prefix(prefix)
        {
            return (Key::String(stripped.to_string()), KeyAction::Constant);
        }
    }
    (key.clone(), KeyAction::Normal)
}

fn process_special_keys(hash: &Hash, config: &MergeConfig) -> Hash {
    let mut result = Hash::new();
    for (key, value) in hash.iter() {
        let (effective_key, action) = resolve_key(key, config);
        match action {
            KeyAction::Override => {
                result.insert(effective_key, Value::OverrideMarker(Rc::new(value.clone())));
            }
            KeyAction::Constant => {
                result.insert(effective_key, Value::ConstantMarker(Rc::new(value.clone())));
            }
            KeyAction::Normal => {
                result.insert(effective_key, value.clone());
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::value::{Key, ReferencePathSegment};
    use hashlink::LinkedHashMap;

    fn default_config() -> MergeConfig {
        MergeConfig::default()
    }

    fn str_val(s: &str) -> Value {
        Value::String(s.to_string())
    }

    fn int_val(i: i64) -> Value {
        Value::Integer(i)
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
    fn test_merge_list_append() {
        let config = default_config();
        let base = array_val(vec![str_val("a"), str_val("b")]);
        let other = array_val(vec![str_val("c")]);
        let result = merge(&base, &other, &config, &[]).unwrap();
        assert_eq!(
            result,
            array_val(vec![str_val("a"), str_val("b"), str_val("c")])
        );
    }

    #[test]
    fn test_merge_map_merge() {
        let config = default_config();
        let base = hash_val(vec![("a".to_string(), int_val(1))]);
        let other = hash_val(vec![("b".to_string(), int_val(2))]);
        let result = merge(&base, &other, &config, &[]).unwrap();
        let expected = hash_val(vec![
            ("a".to_string(), int_val(1)),
            ("b".to_string(), int_val(2)),
        ]);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_merge_map_nested_merge() {
        let config = default_config();
        let base = hash_val(vec![(
            "outer".to_string(),
            hash_val(vec![("inner".to_string(), int_val(1))]),
        )]);
        let other = hash_val(vec![(
            "outer".to_string(),
            hash_val(vec![("other".to_string(), int_val(2))]),
        )]);
        let result = merge(&base, &other, &config, &[]).unwrap();
        let inner = match &result {
            Value::Hash(h) => h.get(&Key::String("outer".to_string())),
            _ => None,
        };
        assert!(inner.is_some());
    }

    #[test]
    fn test_merge_scalar_overwrite() {
        let config = default_config();
        let base = str_val("base");
        let other = str_val("override");
        let result = merge(&base, &other, &config, &[]).unwrap();
        assert_eq!(result, str_val("override"));

        let base = int_val(1);
        let other = str_val("override");
        let result = merge(&base, &other, &config, &[]).unwrap();
        assert_eq!(result, str_val("override"));
    }

    #[test]
    fn test_override_with_tilde() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // Parent: port: 80, Child: ~port: 443
        // merge_hash removes existing "port" and inserts OverrideMarker(443).
        // When this result is later merged with siblings, merge() will create
        // DeferredMerge([sibling_value, OverrideMarker(443)]).
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

    #[test]
    fn test_override_on_first_element() {
        // First element: ~port: 443 (no parent to override)
        // Result: port: OverrideMarker(443) (tilde stripped, value wrapped in OverrideMarker)
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };
        let base = hash_val(vec![]);
        let other = hash_val(vec![("~port".to_string(), int_val(443))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let port_val = h
                    .get(&Key::String("port".to_string()))
                    .expect("port should exist");
                assert!(matches!(port_val, Value::OverrideMarker(v) if **v == int_val(443)));
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_override_keep_prefix() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: false,
            ..MergeConfig::default()
        };

        // With feature_value_override disabled, ~prefix is not processed.
        // Parent: port: 80, Child: ~port: 443
        // Result: {port: 80, ~port: 443} (both keys, no override semantics)
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

    #[test]
    fn test_merge_reference_with_scalar() {
        let config = default_config();
        let base = Value::Reference(vec![ReferencePathSegment::Literal("x".to_string())]);
        let other = Value::Integer(42);
        let result = merge(&base, &other, &config, &[]).unwrap();
        match result {
            Value::DeferredMerge(values) => {
                assert_eq!(values.len(), 2);
            }
            _ => panic!("Expected DeferredMerge, got {:?}", result),
        }
    }

    #[test]
    fn test_merge_scalar_with_reference() {
        let config = default_config();
        let base = Value::Integer(42);
        let other = Value::Reference(vec![ReferencePathSegment::Literal("x".to_string())]);
        let result = merge(&base, &other, &config, &[]).unwrap();
        match result {
            Value::DeferredMerge(values) => {
                assert_eq!(values.len(), 2);
            }
            _ => panic!("Expected DeferredMerge, got {:?}", result),
        }
    }

    #[test]
    fn test_merge_reference_with_reference() {
        let config = default_config();
        let base = Value::Reference(vec![ReferencePathSegment::Literal("one".to_string())]);
        let other = Value::Reference(vec![ReferencePathSegment::Literal("two".to_string())]);
        let result = merge(&base, &other, &config, &[]).unwrap();
        match result {
            Value::DeferredMerge(values) => {
                assert_eq!(values.len(), 2);
            }
            _ => panic!("Expected DeferredMerge, got {:?}", result),
        }
    }

    #[test]
    fn test_merge_deferred_merge_with_deferred_merge() {
        let config = default_config();
        let base = Value::DeferredMerge(Rc::new(vec![Value::Integer(1)]));
        let other = Value::DeferredMerge(Rc::new(vec![Value::Integer(2)]));
        let result = merge(&base, &other, &config, &[]).unwrap();
        match result {
            Value::DeferredMerge(values) => {
                assert_eq!(values.len(), 2);
            }
            _ => panic!("Expected DeferredMerge, got {:?}", result),
        }
    }

    #[test]
    fn test_merge_deferred_merge_with_scalar() {
        let config = default_config();
        let base = Value::DeferredMerge(Rc::new(vec![Value::Integer(1)]));
        let other = Value::Integer(2);
        let result = merge(&base, &other, &config, &[]).unwrap();
        match result {
            Value::DeferredMerge(values) => {
                assert_eq!(values.len(), 2);
                assert_eq!(values[1], Value::Integer(2));
            }
            _ => panic!("Expected DeferredMerge, got {:?}", result),
        }
    }

    #[test]
    fn test_merge_scalar_with_deferred_merge() {
        let config = default_config();
        let base = Value::Integer(1);
        let other = Value::DeferredMerge(Rc::new(vec![Value::Integer(2)]));
        let result = merge(&base, &other, &config, &[]).unwrap();
        match result {
            Value::DeferredMerge(values) => {
                assert_eq!(values.len(), 2);
                assert_eq!(values[0], Value::Integer(1));
            }
            _ => panic!("Expected DeferredMerge, got {:?}", result),
        }
    }

    // --- Tilde override bug tests ---
    // At the merge() level, tilde override produces DeferredMerge or OverrideMarker.
    // The final resolution happens during interpolation.

    #[test]
    fn test_tilde_override_dict_replaces_not_deep_merges() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // base: {accounts: {ssh: true, ldap: true}}
        // other: {~accounts: {local: true}}
        // merge_hash removes existing "accounts" and inserts OverrideMarker({local: true})
        let base = hash_val(vec![(
            "accounts".to_string(),
            hash_val(vec![
                ("ssh".to_string(), Value::Boolean(true)),
                ("ldap".to_string(), Value::Boolean(true)),
            ]),
        )]);
        let other = hash_val(vec![(
            "~accounts".to_string(),
            hash_val(vec![("local".to_string(), Value::Boolean(true))]),
        )]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let accounts = h
                    .get(&Key::String("accounts".to_string()))
                    .expect("accounts key should exist");
                assert!(
                    matches!(accounts, Value::OverrideMarker(v) if matches!(**v, Value::Hash(_))),
                    "accounts should be OverrideMarker(Hash), got {:?}",
                    accounts
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_tilde_override_empty_dict_removes_content() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // base: {accounts: {ssh: true, ldap: true}}
        // other: {~accounts: {}}
        // merge_hash removes existing "accounts" and inserts OverrideMarker({})
        let base = hash_val(vec![(
            "accounts".to_string(),
            hash_val(vec![
                ("ssh".to_string(), Value::Boolean(true)),
                ("ldap".to_string(), Value::Boolean(true)),
            ]),
        )]);
        let other = hash_val(vec![("~accounts".to_string(), hash_val(vec![]))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let accounts = h
                    .get(&Key::String("accounts".to_string()))
                    .expect("accounts key should exist");
                assert!(
                    matches!(accounts, Value::OverrideMarker(v) if matches!(**v, Value::Hash(_))),
                    "accounts should be OverrideMarker(Hash), got {:?}",
                    accounts
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_tilde_override_list_replaces_not_appends() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // base: {ports: [80, 443]}
        // other: {~ports: [8080]}
        // merge_hash removes existing "ports" and inserts OverrideMarker([8080])
        let base = hash_val(vec![(
            "ports".to_string(),
            array_val(vec![int_val(80), int_val(443)]),
        )]);
        let other = hash_val(vec![("~ports".to_string(), array_val(vec![int_val(8080)]))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let ports = h
                    .get(&Key::String("ports".to_string()))
                    .expect("ports key should exist");
                assert!(
                    matches!(ports, Value::OverrideMarker(v) if matches!(**v, Value::Array(_))),
                    "ports should be OverrideMarker(Array), got {:?}",
                    ports
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_tilde_override_scalar_replaces_scalar() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // base: {port: 80}
        // other: {~port: 443}
        // merge_hash removes existing "port" and inserts OverrideMarker(443)
        let base = hash_val(vec![("port".to_string(), int_val(80))]);
        let other = hash_val(vec![("~port".to_string(), int_val(443))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let port = h
                    .get(&Key::String("port".to_string()))
                    .expect("port key should exist");
                assert!(
                    matches!(port, Value::OverrideMarker(v) if **v == int_val(443)),
                    "port should be OverrideMarker(443), got {:?}",
                    port
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    // --- allow_none_override tests ---
    // At the merge() level, tilde override with null produces OverrideMarker(Null).
    // Resolution happens at interpolation time.

    #[test]
    fn test_tilde_override_null_replaces_dict() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: true,
            ..MergeConfig::default()
        };

        // base: {accounts: {ssh: true}}
        // other: {~accounts: null}
        // merge_hash removes existing "accounts" and inserts OverrideMarker(Null)
        let base = hash_val(vec![(
            "accounts".to_string(),
            hash_val(vec![("ssh".to_string(), Value::Boolean(true))]),
        )]);
        let other = hash_val(vec![("~accounts".to_string(), Value::Null)]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let accounts = h
                    .get(&Key::String("accounts".to_string()))
                    .expect("accounts key should exist");
                assert!(
                    matches!(accounts, Value::OverrideMarker(v) if **v == Value::Null),
                    "accounts should be OverrideMarker(Null), got {:?}",
                    accounts
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_tilde_override_null_replaces_list() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: true,
            ..MergeConfig::default()
        };

        // base: {ports: [80, 443]}
        // other: {~ports: null}
        // merge_hash removes existing "ports" and inserts OverrideMarker(Null)
        let base = hash_val(vec![(
            "ports".to_string(),
            array_val(vec![int_val(80), int_val(443)]),
        )]);
        let other = hash_val(vec![("~ports".to_string(), Value::Null)]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let ports = h
                    .get(&Key::String("ports".to_string()))
                    .expect("ports key should exist");
                assert!(
                    matches!(ports, Value::OverrideMarker(v) if **v == Value::Null),
                    "ports should be OverrideMarker(Null), got {:?}",
                    ports
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_none_override_dict_with_allow_none() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: true,
            ..MergeConfig::default()
        };

        // Without tilde prefix, null replaces dict when allow_none_override is true
        // base: {accounts: {ssh: true}}
        // other: {accounts: null}
        // This still goes through regular merge, where Null replaces dict
        let base = hash_val(vec![(
            "accounts".to_string(),
            hash_val(vec![("ssh".to_string(), Value::Boolean(true))]),
        )]);
        let other = hash_val(vec![("accounts".to_string(), Value::Null)]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let accounts = h
                    .get(&Key::String("accounts".to_string()))
                    .expect("accounts key should exist");
                assert_eq!(
                    accounts,
                    &Value::Null,
                    "accounts should be Null, got {:?}",
                    accounts
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_none_override_list_with_allow_none() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: true,
            ..MergeConfig::default()
        };

        // base: {ports: [80, 443]}
        // other: {ports: null}
        // expected: {ports: null}
        let base = hash_val(vec![(
            "ports".to_string(),
            array_val(vec![int_val(80), int_val(443)]),
        )]);
        let other = hash_val(vec![("ports".to_string(), Value::Null)]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let ports = h
                    .get(&Key::String("ports".to_string()))
                    .expect("ports key should exist");
                assert_eq!(ports, &Value::Null, "ports should be Null, got {:?}", ports);
            }
            _ => panic!("Expected Hash"),
        }
    }

    // --- Nested and edge-case override tests ---

    #[test]
    fn test_override_in_other_when_base_is_empty() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // base: {} (empty)
        // other: {~accounts: {}, other_key: "value"}
        // When base is empty, process_override_keys should still wrap
        // ~accounts in OverrideMarker and keep other_key as-is.
        let base = hash_val(vec![]);
        let other = hash_val(vec![
            ("~accounts".to_string(), hash_val(vec![])),
            ("other_key".to_string(), Value::String("value".to_string())),
        ]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let accounts = h
                    .get(&Key::String("accounts".to_string()))
                    .expect("accounts should exist");
                assert!(
                    matches!(accounts, Value::OverrideMarker(v) if matches!(**v, Value::Hash(_))),
                    "accounts should be OverrideMarker(Hash), got {:?}",
                    accounts
                );
                let other_key = h
                    .get(&Key::String("other_key".to_string()))
                    .expect("other_key should exist");
                assert_eq!(
                    other_key,
                    &Value::String("value".to_string()),
                    "other_key should be 'value'"
                );
                assert!(
                    !h.contains_key(&Key::String("~accounts".to_string())),
                    "~accounts key should be stripped"
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_nested_override_key_inside_non_override_parent() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // base: {pve_vm: {template: "t1", description: ["old"]}}
        // other: {pve_vm: {~description: ["new"], vlan_tag: "11"}}
        // The ~description is nested inside pve_vm. Since pve_vm is not
        // override-prefixed, the two hashes should deep-merge, and ~description
        // should override the description key inside pve_vm.
        let base = hash_val(vec![(
            "pve_vm".to_string(),
            hash_val(vec![
                ("template".to_string(), Value::String("t1".to_string())),
                (
                    "description".to_string(),
                    array_val(vec![Value::String("old".to_string())]),
                ),
            ]),
        )]);
        let other = hash_val(vec![(
            "pve_vm".to_string(),
            hash_val(vec![
                (
                    "~description".to_string(),
                    array_val(vec![Value::String("new".to_string())]),
                ),
                ("vlan_tag".to_string(), Value::String("11".to_string())),
            ]),
        )]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let pve_vm = h
                    .get(&Key::String("pve_vm".to_string()))
                    .expect("pve_vm should exist");
                match pve_vm {
                    Value::Hash(inner) => {
                        assert_eq!(
                            inner.get(&Key::String("template".to_string())),
                            Some(&Value::String("t1".to_string())),
                            "template should be preserved from base"
                        );
                        assert_eq!(
                            inner.get(&Key::String("vlan_tag".to_string())),
                            Some(&Value::String("11".to_string())),
                            "vlan_tag should be merged from other"
                        );
                        let desc = inner
                            .get(&Key::String("description".to_string()))
                            .expect("description should exist");
                        assert!(
                            matches!(desc, Value::OverrideMarker(_)),
                            "description should be OverrideMarker (overridden), got {:?}",
                            desc
                        );
                        assert!(
                            !inner.contains_key(&Key::String("~description".to_string())),
                            "~description key should be stripped"
                        );
                    }
                    _ => panic!("pve_vm should be a Hash, got {:?}", pve_vm),
                }
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_deeply_nested_override_key() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // base: {a: {b: {c: {old: true}}}}
        // other: {a: {~b: {c: {new: true}}}}
        // ~b at level 2 should override the entire b subtree.
        let base = hash_val(vec![(
            "a".to_string(),
            hash_val(vec![(
                "b".to_string(),
                hash_val(vec![(
                    "c".to_string(),
                    hash_val(vec![("old".to_string(), Value::Boolean(true))]),
                )]),
            )]),
        )]);
        let other = hash_val(vec![(
            "a".to_string(),
            hash_val(vec![(
                "~b".to_string(),
                hash_val(vec![(
                    "c".to_string(),
                    hash_val(vec![("new".to_string(), Value::Boolean(true))]),
                )]),
            )]),
        )]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let a = h
                    .get(&Key::String("a".to_string()))
                    .expect("a should exist");
                match a {
                    Value::Hash(a_hash) => {
                        let b = a_hash
                            .get(&Key::String("b".to_string()))
                            .expect("b should exist");
                        assert!(
                            matches!(b, Value::OverrideMarker(_)),
                            "b should be OverrideMarker (overridden), got {:?}",
                            b
                        );
                        assert!(
                            !a_hash.contains_key(&Key::String("~b".to_string())),
                            "~b key should be stripped"
                        );
                    }
                    _ => panic!("a should be a Hash, got {:?}", a),
                }
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_override_key_in_first_class_no_parent() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            ..MergeConfig::default()
        };

        // First class in chain defines ~key with no parent to override.
        // base: {} (empty)
        // other: {~port: 443}
        // OverrideMarker should be created; when later merged with another class,
        // it should still override.
        let base = hash_val(vec![]);
        let other = hash_val(vec![("~port".to_string(), int_val(443))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let port = h
                    .get(&Key::String("port".to_string()))
                    .expect("port should exist");
                assert!(
                    matches!(port, Value::OverrideMarker(v) if **v == int_val(443)),
                    "port should be OverrideMarker(443), got {:?}",
                    port
                );
                assert!(
                    !h.contains_key(&Key::String("~port".to_string())),
                    "~port key should be stripped"
                );
            }
            _ => panic!("Expected Hash"),
        }

        // Now merge that result with another class that has port: 80
        let third = hash_val(vec![("port".to_string(), int_val(80))]);
        let final_result = merge(&result, &third, &config, &[]).unwrap();

        match &final_result {
            Value::Hash(h) => {
                let port = h
                    .get(&Key::String("port".to_string()))
                    .expect("port should exist");
                // OverrideMarker(443) should override port: 80,
                // so result should be DeferredMerge([80, OverrideMarker(443)])
                // which resolves to 443 at interpolation time, or
                // the merge produces a DeferredMerge that tracks the override.
                match port {
                    Value::OverrideMarker(v) => {
                        assert_eq!(**v, int_val(443), "OverrideMarker should contain 443");
                    }
                    Value::DeferredMerge(values) => {
                        assert!(
                            values.iter().any(|v| matches!(v, Value::OverrideMarker(_))),
                            "DeferredMerge should contain OverrideMarker, got {:?}",
                            values
                        );
                    }
                    other => panic!(
                        "port should be OverrideMarker or DeferredMerge, got {:?}",
                        other
                    ),
                }
            }
            _ => panic!("Expected Hash"),
        }
    }

    // --- Constant parameter tests ---

    #[test]
    fn test_constant_child_overrides_base_is_allowed() {
        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: true,
            strict_constant_parameters: true,
            ..MergeConfig::default()
        };

        // base: {port: 80}, child: {=port: 443}
        // A child class setting a constant is allowed — it just marks the
        // value as constant going forward. No error should be raised.
        let base = hash_val(vec![("port".to_string(), int_val(80))]);
        let other = hash_val(vec![("=port".to_string(), int_val(443))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let port_val = h
                    .get(&Key::String("port".to_string()))
                    .expect("port should exist");
                assert!(
                    matches!(port_val, Value::ConstantMarker(v) if **v == int_val(443)),
                    "Expected ConstantMarker(443), got {:?}",
                    port_val
                );
                assert!(
                    !h.contains_key(&Key::String("=port".to_string())),
                    "=port key should be stripped"
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_constant_with_equals_prefix() {
        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: true,
            ..MergeConfig::default()
        };

        // base: {port: 80}, other: {=port: 443}
        // =port marks port as constant; any later value should be blocked.
        let base = hash_val(vec![("port".to_string(), int_val(80))]);
        let other = hash_val(vec![("=port".to_string(), int_val(443))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let port_val = h
                    .get(&Key::String("port".to_string()))
                    .expect("port should exist");
                assert!(
                    matches!(port_val, Value::ConstantMarker(v) if **v == int_val(443)),
                    "Expected ConstantMarker(443), got {:?}",
                    port_val
                );
                assert!(
                    !h.contains_key(&Key::String("=port".to_string())),
                    "=port key should be stripped"
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_constant_empty_base() {
        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: true,
            ..MergeConfig::default()
        };

        // base: {}, other: {=port: 443}
        // Constant on first definition should still produce ConstantMarker.
        let base = hash_val(vec![]);
        let other = hash_val(vec![("=port".to_string(), int_val(443))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let port_val = h
                    .get(&Key::String("port".to_string()))
                    .expect("port should exist");
                assert!(
                    matches!(port_val, Value::ConstantMarker(v) if **v == int_val(443)),
                    "Expected ConstantMarker(443), got {:?}",
                    port_val
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_constant_merge_with_existing() {
        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: true,
            ..MergeConfig::default()
        };

        // base: {port: 80}, other: {=port: 443}
        // merge produces DeferredMerge([80, ConstantMarker(443)])
        // At interpolation: output = 80, then ConstantMarker(443) replaces output and sets constant flag.
        // Final result: 443 (constant).
        let base = hash_val(vec![("port".to_string(), int_val(80))]);
        let other = hash_val(vec![("=port".to_string(), int_val(443))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let port_val = h
                    .get(&Key::String("port".to_string()))
                    .expect("port should exist");
                assert!(
                    matches!(port_val, Value::ConstantMarker(v) if **v == int_val(443)),
                    "Expected ConstantMarker(443), got {:?}",
                    port_val
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_constant_disabled() {
        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: false,
            ..MergeConfig::default()
        };

        // When feature_value_constant is disabled, =port is kept as-is.
        let base = hash_val(vec![("port".to_string(), int_val(80))]);
        let other = hash_val(vec![("=port".to_string(), int_val(443))]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                assert!(h.contains_key(&Key::String("port".to_string())));
                assert!(
                    h.contains_key(&Key::String("=port".to_string())),
                    "=port key should NOT be stripped when constant feature is disabled"
                );
                assert_eq!(
                    h.get(&Key::String("=port".to_string())),
                    Some(&int_val(443))
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_constant_dict_replaces_and_blocks() {
        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: true,
            ..MergeConfig::default()
        };

        // base: {accounts: {ssh: true, ldap: true}}
        // other: {=accounts: {local: true}}
        // ConstantMarker replaces entire value and blocks later changes.
        let base = hash_val(vec![(
            "accounts".to_string(),
            hash_val(vec![
                ("ssh".to_string(), Value::Boolean(true)),
                ("ldap".to_string(), Value::Boolean(true)),
            ]),
        )]);
        let other = hash_val(vec![(
            "=accounts".to_string(),
            hash_val(vec![("local".to_string(), Value::Boolean(true))]),
        )]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let accounts = h
                    .get(&Key::String("accounts".to_string()))
                    .expect("accounts key should exist");
                assert!(
                    matches!(accounts, Value::ConstantMarker(v) if matches!(**v, Value::Hash(_))),
                    "accounts should be ConstantMarker(Hash), got {:?}",
                    accounts
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_override_and_constant_together() {
        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            value_constant_prefix: Some("=".to_string()),
            feature_value_override: true,
            feature_value_constant: true,
            ..MergeConfig::default()
        };

        // base: {a: 1, b: 2}
        // other: {~a: 10, =b: 20}
        // ~a -> OverrideMarker, =b -> ConstantMarker
        let base = hash_val(vec![
            ("a".to_string(), int_val(1)),
            ("b".to_string(), int_val(2)),
        ]);
        let other = hash_val(vec![
            ("~a".to_string(), int_val(10)),
            ("=b".to_string(), int_val(20)),
        ]);
        let result = merge(&base, &other, &config, &[]).unwrap();

        match &result {
            Value::Hash(h) => {
                let a_val = h
                    .get(&Key::String("a".to_string()))
                    .expect("a should exist");
                assert!(
                    matches!(a_val, Value::OverrideMarker(v) if **v == int_val(10)),
                    "a should be OverrideMarker(10), got {:?}",
                    a_val
                );
                let b_val = h
                    .get(&Key::String("b".to_string()))
                    .expect("b should exist");
                assert!(
                    matches!(b_val, Value::ConstantMarker(v) if **v == int_val(20)),
                    "b should be ConstantMarker(20), got {:?}",
                    b_val
                );
            }
            _ => panic!("Expected Hash"),
        }
    }
}
