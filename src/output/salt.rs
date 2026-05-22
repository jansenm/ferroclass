// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Salt output adapter for top data and pillar data.
//!
//! Produces [`TopData`] (state-to-node mapping) and [`HostVars`] (per-minion
//! pillar data) in the format expected by Salt's external pillar system.

use crate::inventory as inv;
use crate::inventory::options::Options;
use crate::inventory::value::{Key, Value};
use hashlink::LinkedHashMap;
use serde::ser::{Serialize, SerializeMap, Serializer};
use snafu::prelude::*;
use std::rc::Rc;
use yaml_rust2::Yaml;

use super::YamlOutput;
use super::ansible::{self, HostVars};

/// Errors that can occur while building Salt top data.
#[derive(Debug, Snafu)]
pub enum TopError {
    #[snafu(transparent)]
    TopInventoryLoad { source: inv::Error },
}

/// Salt top-file data — a mapping from environment names to
/// node names to state lists.
pub struct TopData {
    environments: LinkedHashMap<String, LinkedHashMap<String, Vec<String>>>,
}

struct EnvEntry<'a> {
    nodes: &'a LinkedHashMap<String, Vec<String>>,
}

impl<'a> Serialize for EnvEntry<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut node_keys: Vec<&String> = self.nodes.keys().collect();
        node_keys.sort();
        let mut map = serializer.serialize_map(Some(node_keys.len()))?;
        for key in node_keys {
            map.serialize_entry(key, &self.nodes[key])?;
        }
        map.end()
    }
}

impl Serialize for TopData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut env_keys: Vec<&String> = self.environments.keys().collect();
        env_keys.sort();
        let mut map = serializer.serialize_map(Some(env_keys.len()))?;
        for env in &env_keys {
            let entry = EnvEntry {
                nodes: &self.environments[*env],
            };
            map.serialize_entry(*env, &entry)?;
        }
        map.end()
    }
}

impl YamlOutput for TopData {
    fn to_yaml_value(&self, sorted: bool) -> Yaml {
        let mut env_keys: Vec<&String> = self.environments.keys().collect();
        if sorted {
            env_keys.sort();
        }
        let mut map = LinkedHashMap::new();
        for env in &env_keys {
            let nodes = &self.environments[*env];
            let mut node_keys: Vec<&String> = nodes.keys().collect();
            if sorted {
                node_keys.sort();
            }
            let mut node_map = LinkedHashMap::new();
            for key in &node_keys {
                node_map.insert(
                    Yaml::String(key.to_string()),
                    Yaml::Array(
                        nodes[*key]
                            .iter()
                            .map(|s| Yaml::String(s.clone()))
                            .collect(),
                    ),
                );
            }
            map.insert(Yaml::String(env.to_string()), Yaml::Hash(node_map));
        }
        Yaml::Hash(map)
    }
}

/// Errors that can occur while building Salt pillar data.
#[derive(Debug, Snafu)]
pub enum PillarError {
    #[snafu(transparent)]
    PillarInventoryLoad { source: inv::Error },
    #[snafu(display("node '{node_name}' not found"))]
    NodeNotFound { node_name: String },
    #[snafu(display("error merging node '{node_name}'"))]
    Merge {
        source: Box<inv::Error>,
        node_name: String,
    },
}

fn inject_salt_reclass_fields(
    parameters: &mut LinkedHashMap<Key, Value>,
    nodename: &str,
    classes: &[String],
    applications: &[String],
    environment: &dyn std::fmt::Display,
) {
    let reclass_key = Key::String("__reclass__".to_string());
    let mut reclass_hash = match parameters.remove(&reclass_key) {
        Some(Value::Hash(h)) => Rc::try_unwrap(h).unwrap_or_else(|rc| (*rc).clone()),
        _ => LinkedHashMap::new(),
    };
    reclass_hash.insert(
        Key::String("nodename".to_string()),
        Value::String(nodename.to_string()),
    );
    reclass_hash.insert(
        Key::String("classes".to_string()),
        Value::Array(Rc::new(
            classes.iter().map(|s| Value::String(s.clone())).collect(),
        )),
    );
    reclass_hash.insert(
        Key::String("applications".to_string()),
        Value::Array(Rc::new(
            applications
                .iter()
                .map(|s| Value::String(s.clone()))
                .collect(),
        )),
    );
    reclass_hash.insert(
        Key::String("environment".to_string()),
        Value::String(environment.to_string()),
    );
    parameters.insert(reclass_key, Value::Hash(Rc::new(reclass_hash)));
}

/// Build Salt top data from configuration.
///
/// Returns a [`TopData`] mapping each environment to its nodes and
/// their state lists.
pub fn build_top(config: &Options) -> Result<TopData, TopError> {
    let merge_config = config.build_merge_config();
    let ignore_failed_node = merge_config.inventory_ignore_failed_node;
    let ignore_failed_render = merge_config.inventory_ignore_failed_render;
    let mut inventory = inv::load(&config.storage_options).map_err(TopError::from)?;
    inventory.set_class_mappings(config.class_mappings.clone());
    inventory.set_class_mappings_match_path(config.class_mappings_match_path);
    inventory.set_merge_config(merge_config);

    let inv_map = inventory.build_inventory_map().map_err(TopError::from)?;

    let mut environments: LinkedHashMap<String, LinkedHashMap<String, Vec<String>>> =
        LinkedHashMap::new();

    for node in inventory.nodes_iter() {
        let merged = match inventory.merge_node(node.name()) {
            Ok(n) => n,
            Err(e) => {
                if ignore_failed_node {
                    tracing::warn!("Skipping node '{}': {}", node.name(), e);
                    continue;
                }
                return Err(TopError::TopInventoryLoad { source: e });
            }
        };

        let has_inv_query = {
            let params = merged.parameters();
            params.values().any(|v| v.has_inv_query())
                || merged.exports().values().any(|v| v.has_inv_query())
        };

        let merged = if has_inv_query {
            match inventory.merge_node_with_inventory(node.name(), &inv_map) {
                Ok(n) => n,
                Err(e) => {
                    if ignore_failed_render {
                        tracing::warn!(
                            "Skipping node '{}': inv query render failed: {}",
                            node.name(),
                            e
                        );
                        continue;
                    }
                    return Err(TopError::TopInventoryLoad { source: e });
                }
            }
        } else {
            merged
        };

        let env = merged.environment().to_string();
        let apps: Vec<String> = merged.applications().as_list().to_vec();
        environments
            .entry(env)
            .or_insert_with(LinkedHashMap::new)
            .insert(merged.name().to_string(), apps);
    }

    Ok(TopData { environments })
}

/// Build Salt pillar data for a single minion.
///
/// Merges the node, resolves inventory queries, and returns the
/// host variables including `__reclass__` metadata.
pub fn build_pillar(config: &Options, minion_id: &str) -> Result<HostVars, PillarError> {
    let merge_config = config.build_merge_config();
    let ignore_failed_render = merge_config.inventory_ignore_failed_render;
    let mut inventory = inv::load(&config.storage_options).map_err(PillarError::from)?;
    inventory.set_class_mappings(config.class_mappings.clone());
    inventory.set_class_mappings_match_path(config.class_mappings_match_path);
    inventory.set_merge_config(merge_config);

    let _node = inventory
        .get_node(minion_id)
        .ok_or_else(|| PillarError::NodeNotFound {
            node_name: minion_id.to_string(),
        })?;

    let merged = inventory
        .merge_node(minion_id)
        .map_err(|e| PillarError::Merge {
            source: Box::new(e),
            node_name: minion_id.to_string(),
        })?;

    let has_inv_query = {
        let params = merged.parameters();
        params.values().any(|v| v.has_inv_query())
            || merged.exports().values().any(|v| v.has_inv_query())
    };

    let merged = if has_inv_query {
        let inv_map = inventory.build_inventory_map().map_err(PillarError::from)?;
        match inventory.merge_node_with_inventory(minion_id, &inv_map) {
            Ok(n) => n,
            Err(e) => {
                if ignore_failed_render {
                    tracing::warn!(
                        "Failed to render inv queries for node '{}': {}",
                        minion_id,
                        e
                    );
                }
                return Err(PillarError::Merge {
                    source: Box::new(e),
                    node_name: minion_id.to_string(),
                });
            }
        }
    } else {
        merged
    };

    let mut parameters = LinkedHashMap::new();
    for (k, v) in merged.parameters() {
        parameters.insert(k.clone(), v.clone());
    }
    ansible::inject_classes_and_applications_into_reclass(
        &mut parameters,
        merged.classes(),
        merged.applications().as_list(),
    );
    inject_salt_reclass_fields(
        &mut parameters,
        minion_id,
        merged.classes(),
        merged.applications().as_list(),
        merged.environment(),
    );

    Ok(HostVars { parameters })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_salt_reclass_fields() {
        let mut params = LinkedHashMap::new();
        let mut reclass_inner = LinkedHashMap::new();
        reclass_inner.insert(Key::from("name"), Value::String("test".to_string()));
        params.insert(
            Key::from("__reclass__"),
            Value::Hash(Rc::new(reclass_inner)),
        );
        params.insert(Key::from("hostname"), Value::String("web-01".to_string()));

        inject_salt_reclass_fields(
            &mut params,
            "web-01",
            &["all".to_string(), "env.prod".to_string()],
            &["web".to_string()],
            &"production",
        );

        let reclass = params.get(&Key::from("__reclass__")).unwrap();
        if let Value::Hash(h) = reclass {
            let h = Rc::try_unwrap(h.clone()).unwrap_or_else(|rc| (*rc).clone());
            assert!(h.contains_key(&Key::from("nodename")));
            assert!(h.contains_key(&Key::from("classes")));
            assert!(h.contains_key(&Key::from("applications")));
            assert!(h.contains_key(&Key::from("environment")));
            assert!(h.contains_key(&Key::from("name")));

            let nodename = h.get(&Key::from("nodename")).unwrap();
            assert_eq!(nodename, &Value::String("web-01".to_string()));

            let env = h.get(&Key::from("environment")).unwrap();
            assert_eq!(env, &Value::String("production".to_string()));
        } else {
            panic!("expected Hash");
        }
    }

    #[test]
    fn test_inject_salt_reclass_fields_creates_key_when_absent() {
        let mut params = LinkedHashMap::new();
        params.insert(Key::from("hostname"), Value::String("web-01".to_string()));

        inject_salt_reclass_fields(
            &mut params,
            "web-01",
            &["all".to_string()],
            &["web".to_string()],
            &"base",
        );

        assert!(params.contains_key(&Key::from("__reclass__")));
        assert!(!params.contains_key(&Key::from("_reclass_")));

        let reclass = params.get(&Key::from("__reclass__")).unwrap();
        if let Value::Hash(h) = reclass {
            let h = Rc::try_unwrap(h.clone()).unwrap_or_else(|rc| (*rc).clone());
            assert!(h.contains_key(&Key::from("nodename")));
            assert!(h.contains_key(&Key::from("classes")));
            assert!(h.contains_key(&Key::from("applications")));
            assert!(h.contains_key(&Key::from("environment")));
        } else {
            panic!("expected Hash");
        }
    }

    #[test]
    fn test_inject_salt_reclass_fields_preserves_automatic_params() {
        let mut params = LinkedHashMap::new();
        let mut auto_inner = LinkedHashMap::new();
        let mut name_map = LinkedHashMap::new();
        name_map.insert(
            Key::from("full"),
            Value::String("web-01.example.com".to_string()),
        );
        name_map.insert(Key::from("short"), Value::String("web-01".to_string()));
        auto_inner.insert(Key::from("name"), Value::Hash(Rc::new(name_map)));
        auto_inner.insert(Key::from("environment"), Value::String("base".to_string()));
        params.insert(Key::from("_reclass_"), Value::Hash(Rc::new(auto_inner)));

        inject_salt_reclass_fields(
            &mut params,
            "web-01.example.com",
            &["all".to_string()],
            &["web".to_string()],
            &"production",
        );

        assert!(params.contains_key(&Key::from("_reclass_")));
        assert!(params.contains_key(&Key::from("__reclass__")));

        let auto_reclass = params.get(&Key::from("_reclass_")).unwrap();
        if let Value::Hash(h) = auto_reclass {
            let h = Rc::try_unwrap(h.clone()).unwrap_or_else(|rc| (*rc).clone());
            assert!(h.contains_key(&Key::from("name")));
            assert!(h.contains_key(&Key::from("environment")));
            assert!(!h.contains_key(&Key::from("nodename")));
            assert!(!h.contains_key(&Key::from("classes")));
        } else {
            panic!("expected Hash for _reclass_");
        }

        let adapter_reclass = params.get(&Key::from("__reclass__")).unwrap();
        if let Value::Hash(h) = adapter_reclass {
            let h = Rc::try_unwrap(h.clone()).unwrap_or_else(|rc| (*rc).clone());
            assert_eq!(
                h.get(&Key::from("nodename")),
                Some(&Value::String("web-01.example.com".to_string()))
            );
            assert_eq!(
                h.get(&Key::from("environment")),
                Some(&Value::String("production".to_string()))
            );
        } else {
            panic!("expected Hash for __reclass__");
        }
    }
}
