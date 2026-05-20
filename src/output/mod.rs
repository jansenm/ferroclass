// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Output formatting — the format layer of the three-layer architecture.
//!
//! The output module transforms domain types from the adapter layer
//! ([`ansible::AnsibleInventory`], [`salt::TopData`], [`reclass::InventoryOutput`])
//! into their final serialized form. JSON output uses [`Serialize`]; YAML
//! output uses the [`YamlOutput`] trait.
//!
//! Adapters live in the sub-modules:
//!
//! - [`reclass`] — plain reclass-compatible YAML/JSON output
//! - [`ansible`] — Ansible dynamic inventory and host-vars output
//! - [`salt`] — Salt top data and pillar output

pub mod ansible;
pub mod reclass;
pub mod salt;

use crate::inventory::options::OutputFormat;
use crate::inventory::value::{Key, Value};
use hashlink::LinkedHashMap;
use serde::ser::{Serialize, SerializeMap, Serializer};
use yaml_rust2::Yaml;

/// Serialize a value to JSON.
///
/// When `pretty` is true, the output is indented with newlines;
/// otherwise compact single-line JSON is produced.
pub fn format_json<T: Serialize>(value: &T, pretty: bool) -> Result<String, serde_json::Error> {
    if pretty {
        serde_json::to_string_pretty(value)
    } else {
        serde_json::to_string(value)
    }
}

/// Serialize a [`Yaml`] value to a YAML string.
///
/// When `pretty` is true, the emitter uses multiline block style
/// and preserves string newlines.
pub fn format_yaml(yaml_value: &Yaml, pretty: bool) -> Result<String, yaml_rust2::EmitError> {
    let mut output = String::new();
    let mut emitter = yaml_rust2::YamlEmitter::new(&mut output);
    if pretty {
        emitter.compact(false);
        emitter.multiline_strings(true);
    }
    emitter.dump(yaml_value)?;
    Ok(output)
}

/// Format a value as JSON or YAML according to the given output format.
///
/// This is the primary formatting entry point used by all binary adapters.
/// For JSON, the value must implement [`Serialize`]; for YAML, it must
/// implement [`YamlOutput`]. The `sorted` flag controls whether YAML
/// mapping keys are emitted in alphabetical order.
pub fn format_output<T: Serialize + YamlOutput>(
    value: &T,
    output_format: OutputFormat,
    pretty: bool,
    sorted: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    match output_format {
        OutputFormat::JSON => format_json(value, pretty).map_err(|e| e.into()),
        OutputFormat::Yaml => {
            let yaml_value = value.to_yaml_value(sorted);
            format_yaml(&yaml_value, pretty).map_err(|e| e.into())
        }
    }
}

/// Trait for types that can render themselves as a YAML value tree.
///
/// When `sorted` is true, all mapping keys at every nesting level
/// are emitted in alphabetical order.
pub trait YamlOutput {
    fn to_yaml_value(&self, sorted: bool) -> Yaml;
}

pub(super) struct ReclassMap<'a>(pub &'a LinkedHashMap<Key, Value>);

impl<'a> Serialize for ReclassMap<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (k, v) in self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'a> ReclassMap<'a> {
    pub fn to_yaml_sorted(&self, sorted: bool) -> LinkedHashMap<Yaml, Yaml> {
        let entries: Vec<_> = self
            .0
            .iter()
            .map(|(k, v)| (k.to_yaml_value(), v.to_yaml_value_sorted(sorted)))
            .collect();
        let mut map = LinkedHashMap::new();
        if sorted {
            let mut sorted_entries = entries;
            sorted_entries.sort_by(|a, b| yaml_key_cmp(&a.0, &b.0));
            for (k, v) in sorted_entries {
                map.insert(k, v);
            }
        } else {
            for (k, v) in entries {
                map.insert(k, v);
            }
        }
        map
    }
}

fn yaml_key_cmp(a: &Yaml, b: &Yaml) -> std::cmp::Ordering {
    match (a, b) {
        (Yaml::String(sa), Yaml::String(sb)) => sa.cmp(sb),
        (Yaml::Integer(ia), Yaml::Integer(ib)) => ia.cmp(ib),
        (Yaml::Real(ra), Yaml::Real(rb)) => ra.cmp(rb),
        (Yaml::Boolean(ba), Yaml::Boolean(bb)) => ba.cmp(bb),
        _ => a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")),
    }
}

/// Return the current local time formatted as a reclass-style timestamp
/// (e.g. `Thu May 20 14:30:00 2026`).
#[cfg(not(tarpaulin_include))]
pub fn format_timestamp() -> String {
    chrono::Local::now()
        .format("%a %b %e %H:%M:%S %Y")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_json_pretty() {
        let value = serde_json::json!({"key": "value"});
        let result = format_json(&value, true).unwrap();
        assert!(result.contains('\n'));
        assert!(result.contains("\"key\""));
        assert!(result.contains("\"value\""));
    }

    #[test]
    fn test_format_json_compact() {
        let value = serde_json::json!({"key": "value"});
        let result = format_json(&value, false).unwrap();
        assert!(!result.contains('\n'));
        assert!(result.contains("\"key\""));
    }

    #[test]
    fn test_format_yaml_pretty() {
        let yaml = yaml_rust2::Yaml::Hash(yaml_rust2::yaml::Hash::new());
        let result = format_yaml(&yaml, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_yaml_compact() {
        let yaml = yaml_rust2::Yaml::Hash(yaml_rust2::yaml::Hash::new());
        let result = format_yaml(&yaml, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_output_json() {
        use crate::inventory::value::{Key, Value};
        let mut map = LinkedHashMap::new();
        map.insert(Key::from("key"), Value::String("value".to_string()));
        let reclass_map = ReclassMap(&map);
        let json = format_json(&reclass_map, true).unwrap();
        assert!(json.contains("\"key\""));
    }

    #[test]
    fn test_format_output_yaml() {
        use crate::inventory::value::{Key, Value};
        let mut map = LinkedHashMap::new();
        map.insert(Key::from("name"), Value::String("test".to_string()));
        let reclass_map = ReclassMap(&map);
        let yaml_hash = reclass_map.to_yaml_sorted(false);
        let yaml_value = Yaml::Hash(yaml_hash);
        let result = format_yaml(&yaml_value, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reclass_map_to_yaml_sorted_false() {
        use crate::inventory::value::{Key, Value};
        let mut map = LinkedHashMap::new();
        map.insert(Key::from("b"), Value::Integer(2));
        map.insert(Key::from("a"), Value::Integer(1));
        let reclass_map = ReclassMap(&map);
        let result = reclass_map.to_yaml_sorted(false);
        let keys: Vec<_> = result.keys().collect();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_reclass_map_to_yaml_sorted_true() {
        use crate::inventory::value::{Key, Value};
        let mut map = LinkedHashMap::new();
        map.insert(Key::from("b"), Value::Integer(2));
        map.insert(Key::from("a"), Value::Integer(1));
        let reclass_map = ReclassMap(&map);
        let result = reclass_map.to_yaml_sorted(true);
        let keys: Vec<_> = result.keys().collect();
        if let yaml_rust2::Yaml::String(s) = &keys[0] {
            assert_eq!(s, "a");
        }
    }

    #[test]
    fn test_yaml_key_cmp_strings() {
        let a = yaml_rust2::Yaml::String("abc".to_string());
        let b = yaml_rust2::Yaml::String("def".to_string());
        assert_eq!(yaml_key_cmp(&a, &b), std::cmp::Ordering::Less);
    }

    #[test]
    fn test_yaml_key_cmp_integers() {
        let a = yaml_rust2::Yaml::Integer(1);
        let b = yaml_rust2::Yaml::Integer(2);
        assert_eq!(yaml_key_cmp(&a, &b), std::cmp::Ordering::Less);
    }
}
