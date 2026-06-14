// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Reclass-compatible output adapter.
//!
//! Produces [`InventoryOutput`] (full inventory listing) and
//! [`NodeInfoOutput`] (single-node detail) in the same YAML/JSON format as
//! Python reclass. Includes `__reclass__` metadata, timestamps, and
//! application/class lists per node.

use crate::inventory as inv;
use crate::inventory::value::{Key, Value};
use hashlink::LinkedHashMap;
use serde::ser::{Serialize, SerializeMap, Serializer};
use snafu::prelude::*;
use std::sync::Arc;
use yaml_rust2::Yaml;

use super::{ReclassMap, YamlOutput};

/// Errors that can occur while building a reclass inventory or node-info output.
#[derive(Debug, Snafu)]
pub enum InventoryError {
    #[snafu(transparent)]
    InventoryLoad { source: inv::Error },
}

/// A timestamp wrapper that serializes as `{timestamp: "..."}` in JSON/YAML.
#[derive(Debug, serde::Serialize, Default)]
pub struct ReclassTimestamp {
    timestamp: String,
}

#[derive(Debug)]
struct NodeReclassInfo {
    node: String,
    name: String,
    uri: String,
    environment: String,
    timestamp: String,
}

impl Serialize for NodeReclassInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(5))?;
        map.serialize_entry("node", &self.node)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("uri", &self.uri)?;
        map.serialize_entry("environment", &self.environment)?;
        map.serialize_entry("timestamp", &self.timestamp)?;
        map.end()
    }
}

fn build_reclass_parameter(
    short_name: &str,
    full_name: &str,
    environment: &dyn std::fmt::Display,
) -> (Key, Value) {
    let mut name_hash: LinkedHashMap<Key, Value> = LinkedHashMap::new();
    name_hash.insert(
        Key::String("short".to_string()),
        Value::String(short_name.to_string()),
    );
    name_hash.insert(
        Key::String("full".to_string()),
        Value::String(full_name.to_string()),
    );
    let mut reclass_hash: LinkedHashMap<Key, Value> = LinkedHashMap::new();
    reclass_hash.insert(
        Key::String("name".to_string()),
        Value::Hash(Arc::new(name_hash)),
    );
    reclass_hash.insert(
        Key::String("environment".to_string()),
        Value::String(environment.to_string()),
    );
    (
        Key::String("_reclass_".to_string()),
        Value::Hash(Arc::new(reclass_hash)),
    )
}

/// A merged node together with a timestamp, serializable in reclass-compatible format.
#[derive(Debug)]
pub struct NodeWithMetadata {
    pub node: inv::Node,
    pub timestamp: String,
}

impl Serialize for NodeWithMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut parameters: LinkedHashMap<Key, Value> = LinkedHashMap::new();
        let (reclass_key, reclass_value) = build_reclass_parameter(
            self.node.short_name(),
            self.node.name(),
            self.node.environment(),
        );
        parameters.insert(reclass_key, reclass_value);
        for (k, v) in self.node.parameters() {
            parameters.insert(k.clone(), v.clone());
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry(
            "__reclass__",
            &NodeReclassInfo {
                node: self.node.name().to_string(),
                name: self.node.name().to_string(),
                uri: self.node.uri().unwrap_or("").to_string(),
                environment: self.node.environment().to_string(),
                timestamp: self.timestamp.clone(),
            },
        )?;
        map.serialize_entry("environment", self.node.environment())?;
        map.serialize_entry("classes", self.node.classes())?;
        map.serialize_entry("applications", self.node.applications())?;
        map.serialize_entry("parameters", &ReclassMap(&parameters))?;
        map.serialize_entry("exports", &ReclassMap(self.node.exports()))?;
        map.end()
    }
}

/// Full reclass-compatible inventory output.
///
/// Contains all nodes, classes, and applications with `__reclass__` metadata.
/// Use [`InventoryOutput::from_inventory`] to construct and
/// [`crate::output::format_output`] to serialize.
#[derive(Debug, serde::Serialize, Default)]
pub struct InventoryOutput {
    #[serde(serialize_with = "serialize_reclass_timestamp")]
    pub __reclass__: ReclassTimestamp,
    #[serde(serialize_with = "serialize_nodes")]
    pub nodes: LinkedHashMap<String, NodeWithMetadata>,
    #[serde(serialize_with = "serialize_classes_map")]
    pub classes: LinkedHashMap<String, Vec<String>>,
    #[serde(serialize_with = "serialize_classes_map")]
    pub applications: LinkedHashMap<String, Vec<String>>,
}

fn serialize_reclass_timestamp<S: Serializer>(
    value: &ReclassTimestamp,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(1))?;
    map.serialize_entry("timestamp", &value.timestamp)?;
    map.end()
}

fn serialize_nodes<S: Serializer>(
    value: &LinkedHashMap<String, NodeWithMetadata>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(value.len()))?;
    for (k, v) in value {
        map.serialize_entry(k, v)?;
    }
    map.end()
}

fn serialize_classes_map<S: Serializer>(
    value: &LinkedHashMap<String, Vec<String>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(value.len()))?;
    for (k, v) in value {
        map.serialize_entry(k, v)?;
    }
    map.end()
}

impl InventoryOutput {
    /// Build a full inventory output from the given [`crate::inventory::Inventory`].
    ///
    /// Merges all nodes, collects classes and applications, and resolves
    /// inventory queries. Failed nodes are skipped when `ignore_failed_node`
    /// is true; failed inv-query renders are skipped when
    /// `ignore_failed_render` is true.
    pub fn from_inventory(
        inventory: &inv::Inventory,
        timestamp: &str,
        ignore_failed_node: bool,
        ignore_failed_render: bool,
    ) -> Result<Self, InventoryError> {
        let inv_map = inventory
            .build_inventory_map()
            .map_err(InventoryError::from)?;

        let mut nodes = LinkedHashMap::new();
        let mut classes: LinkedHashMap<String, Vec<String>> = LinkedHashMap::new();
        let mut applications: LinkedHashMap<String, Vec<String>> = LinkedHashMap::new();

        for node in inventory.nodes_iter() {
            let merged = match inventory.merge_node(node.name()) {
                Ok(n) => n,
                Err(e) => {
                    // Implementation error (I/O, etc.) — propagate
                    return Err(InventoryError::InventoryLoad { source: e });
                }
            };

            if !merged.is_usable() {
                if ignore_failed_node {
                    tracing::warn!(
                        "Skipping failed node '{}': {}",
                        node.name(),
                        merged
                            .diagnostics()
                            .first()
                            .map(|d| d.message.as_str())
                            .unwrap_or("unknown error")
                    );
                    continue;
                }
                return Err(InventoryError::InventoryLoad {
                    source: inv::Error::NodeNotFound {
                        node_name: node.name().to_string(),
                    },
                });
            }

            let has_inv_query = {
                let params = merged.parameters();
                params.values().any(|v| v.has_inv_query())
                    || merged.exports().values().any(|v| v.has_inv_query())
            };

            let merged = if has_inv_query {
                match inventory.merge_node_with_inventory(node.name(), &inv_map) {
                    Ok(n) => {
                        if !n.is_usable() {
                            if ignore_failed_render {
                                tracing::warn!(
                                    "Skipping node '{}': inv query render failed: {}",
                                    node.name(),
                                    n.diagnostics()
                                        .first()
                                        .map(|d| d.message.as_str())
                                        .unwrap_or("unknown error")
                                );
                                continue;
                            }
                            return Err(InventoryError::InventoryLoad {
                                source: inv::Error::NodeNotFound {
                                    node_name: node.name().to_string(),
                                },
                            });
                        }
                        n
                    }
                    Err(e) => {
                        return Err(InventoryError::InventoryLoad { source: e });
                    }
                }
            } else {
                merged
            };

            tracing::debug!("merged node {}", merged.name());
            for class_name in merged.classes() {
                classes
                    .entry(class_name.clone())
                    .or_insert_with(Vec::new)
                    .push(merged.name().to_string());
            }
            for app_name in merged.applications().as_list() {
                applications
                    .entry(app_name.clone())
                    .or_insert_with(Vec::new)
                    .push(merged.name().to_string());
            }
            nodes.insert(
                merged.name().to_string(),
                NodeWithMetadata {
                    node: merged,
                    timestamp: timestamp.to_string(),
                },
            );
        }
        Ok(Self {
            __reclass__: ReclassTimestamp {
                timestamp: timestamp.to_string(),
            },
            nodes,
            classes,
            applications,
        })
    }

    /// Sort node keys, class lists, and application lists alphabetically.
    ///
    /// Call this before serialization if you need deterministic output
    /// order (e.g. for snapshot tests or diff-friendly output).
    pub fn sort_keys(&mut self) {
        let mut sorted_nodes: Vec<(String, NodeWithMetadata)> = self.nodes.drain().collect();
        sorted_nodes.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in sorted_nodes {
            self.nodes.insert(k, v);
        }
        for nodes in self.classes.values_mut() {
            nodes.sort();
        }
        for nodes in self.applications.values_mut() {
            nodes.sort();
        }
    }
}

impl YamlOutput for InventoryOutput {
    fn to_yaml_value(&self, sorted: bool) -> Yaml {
        let mut reclass_map = LinkedHashMap::new();
        reclass_map.insert(
            Yaml::String("timestamp".to_string()),
            Yaml::String(self.__reclass__.timestamp.clone()),
        );
        let nodes_map: LinkedHashMap<Yaml, Yaml> = if sorted {
            let mut entries: Vec<_> = self.nodes.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            entries
                .into_iter()
                .map(|(k, v)| {
                    (
                        Yaml::String(k.clone()),
                        v.node.to_yaml_value_with_reclass(&v.timestamp, sorted),
                    )
                })
                .collect()
        } else {
            self.nodes
                .iter()
                .map(|(k, v)| {
                    (
                        Yaml::String(k.clone()),
                        v.node.to_yaml_value_with_reclass(&v.timestamp, sorted),
                    )
                })
                .collect()
        };
        let classes_map: LinkedHashMap<Yaml, Yaml> = if sorted {
            let mut entries: Vec<_> = self.classes.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            entries
                .into_iter()
                .map(|(k, v)| {
                    let node_list =
                        Yaml::Array(v.iter().map(|s| Yaml::String(s.clone())).collect());
                    (Yaml::String(k.clone()), node_list)
                })
                .collect()
        } else {
            self.classes
                .iter()
                .map(|(k, v)| {
                    let node_list =
                        Yaml::Array(v.iter().map(|s| Yaml::String(s.clone())).collect());
                    (Yaml::String(k.clone()), node_list)
                })
                .collect()
        };
        let applications_map: LinkedHashMap<Yaml, Yaml> = if sorted {
            let mut entries: Vec<_> = self.applications.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            entries
                .into_iter()
                .map(|(k, v)| {
                    let node_list =
                        Yaml::Array(v.iter().map(|s| Yaml::String(s.clone())).collect());
                    (Yaml::String(k.clone()), node_list)
                })
                .collect()
        } else {
            self.applications
                .iter()
                .map(|(k, v)| {
                    let node_list =
                        Yaml::Array(v.iter().map(|s| Yaml::String(s.clone())).collect());
                    (Yaml::String(k.clone()), node_list)
                })
                .collect()
        };
        let mut map = LinkedHashMap::new();
        map.insert(
            Yaml::String("__reclass__".to_string()),
            Yaml::Hash(reclass_map),
        );
        map.insert(Yaml::String("nodes".to_string()), Yaml::Hash(nodes_map));
        map.insert(Yaml::String("classes".to_string()), Yaml::Hash(classes_map));
        map.insert(
            Yaml::String("applications".to_string()),
            Yaml::Hash(applications_map),
        );
        Yaml::Hash(map)
    }
}

#[derive(Debug, serde::Serialize)]
struct ReclassInfo {
    node: String,
    name: String,
    uri: String,
    environment: String,
    timestamp: String,
}

/// Single-node detail output in reclass-compatible format.
///
/// Contains the resolved node with `__reclass__` metadata, classes,
/// applications, parameters, and exports.
#[derive(Debug, serde::Serialize)]
pub struct NodeInfoOutput {
    #[serde(serialize_with = "serialize_reclass_info")]
    __reclass__: ReclassInfo,
    #[serde(serialize_with = "serialize_classes_list")]
    classes: Vec<String>,
    #[serde(serialize_with = "serialize_classes_list")]
    applications: Vec<String>,
    environment: String,
    #[serde(serialize_with = "serialize_parameters")]
    parameters: LinkedHashMap<Key, Value>,
    #[serde(serialize_with = "serialize_parameters")]
    exports: LinkedHashMap<Key, Value>,
}

fn serialize_reclass_info<S: Serializer>(
    value: &ReclassInfo,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(5))?;
    map.serialize_entry("node", &value.node)?;
    map.serialize_entry("name", &value.name)?;
    map.serialize_entry("uri", &value.uri)?;
    map.serialize_entry("environment", &value.environment)?;
    map.serialize_entry("timestamp", &value.timestamp)?;
    map.end()
}

fn serialize_classes_list<S: Serializer>(
    value: &[String],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeSeq;
    let mut seq = serializer.serialize_seq(Some(value.len()))?;
    for item in value {
        seq.serialize_element(item)?;
    }
    seq.end()
}

fn serialize_parameters<S: Serializer>(
    value: &LinkedHashMap<Key, Value>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut map = serializer.serialize_map(Some(value.len()))?;
    for (k, v) in value {
        map.serialize_entry(k, v)?;
    }
    map.end()
}

impl NodeInfoOutput {
    /// Build a node-info output from a single merged node.
    pub fn from_node(node: &inv::Node, timestamp: &str) -> Self {
        let mut parameters = LinkedHashMap::new();
        let (reclass_key, reclass_value) =
            build_reclass_parameter(node.short_name(), node.name(), node.environment());
        parameters.insert(reclass_key, reclass_value);
        for (k, v) in node.parameters() {
            parameters.insert(k.clone(), v.clone());
        }
        Self {
            __reclass__: ReclassInfo {
                node: node.name().to_string(),
                name: node.name().to_string(),
                uri: node.uri().unwrap_or("").to_string(),
                environment: node.environment().to_string(),
                timestamp: timestamp.to_string(),
            },
            classes: node.classes().clone(),
            applications: node.applications().as_list().to_vec(),
            environment: node.environment().to_string(),
            parameters,
            exports: node.exports().clone(),
        }
    }
}

impl YamlOutput for NodeInfoOutput {
    fn to_yaml_value(&self, sorted: bool) -> Yaml {
        use inv::value::Value;
        let mut reclass = LinkedHashMap::new();
        reclass.insert(
            Yaml::String("node".to_string()),
            Yaml::String(self.__reclass__.node.clone()),
        );
        reclass.insert(
            Yaml::String("name".to_string()),
            Yaml::String(self.__reclass__.name.clone()),
        );
        reclass.insert(
            Yaml::String("uri".to_string()),
            Yaml::String(self.__reclass__.uri.clone()),
        );
        reclass.insert(
            Yaml::String("environment".to_string()),
            Yaml::String(self.__reclass__.environment.clone()),
        );
        reclass.insert(
            Yaml::String("timestamp".to_string()),
            Yaml::String(self.__reclass__.timestamp.clone()),
        );
        let mut map = LinkedHashMap::new();
        map.insert(Yaml::String("__reclass__".to_string()), Yaml::Hash(reclass));
        map.insert(
            Yaml::String("classes".to_string()),
            Yaml::Array(
                self.classes
                    .iter()
                    .map(|s| Yaml::String(s.clone()))
                    .collect(),
            ),
        );
        map.insert(
            Yaml::String("applications".to_string()),
            Yaml::Array(
                self.applications
                    .iter()
                    .map(|s| Yaml::String(s.clone()))
                    .collect(),
            ),
        );
        map.insert(
            Yaml::String("environment".to_string()),
            Yaml::String(self.environment.clone()),
        );
        map.insert(
            Yaml::String("parameters".to_string()),
            Value::Hash(std::sync::Arc::new(self.parameters.clone())).to_yaml_value_sorted(sorted),
        );
        map.insert(
            Yaml::String("exports".to_string()),
            Value::Hash(std::sync::Arc::new(self.exports.clone())).to_yaml_value_sorted(sorted),
        );
        Yaml::Hash(map)
    }
}
