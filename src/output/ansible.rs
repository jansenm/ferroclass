// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Ansible dynamic inventory output adapter.
//!
//! Produces [`AnsibleInventory`] (full `--list` output) and [`HostVars`]
//! (per-host detail output) in the format expected by Ansible's dynamic
//! inventory protocol.

use crate::inventory as inv;
use crate::inventory::options::Options;
use crate::inventory::value::{Key, Value};
use hashlink::LinkedHashMap;
use serde::ser::{Serialize, SerializeMap, Serializer};
use snafu::prelude::*;
use std::rc::Rc;
use yaml_rust2::Yaml;

use super::{ReclassMap, YamlOutput};

/// Errors that can occur while building Ansible inventory or host-vars output.
#[derive(Debug, Snafu)]
pub enum AnsibleInventoryError {
    #[snafu(transparent)]
    InventoryLoad { source: inv::Error },
}

/// Ansible dynamic inventory, containing groups and host variables.
pub struct AnsibleInventory {
    groups: LinkedHashMap<String, Vec<String>>,
    hostvars: LinkedHashMap<String, AnsibleNodeInfo>,
}

impl Serialize for AnsibleInventory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut group_keys: Vec<&String> = self.groups.keys().collect();
        group_keys.sort();
        let total_entries = group_keys.len() + 1;
        let mut map = serializer.serialize_map(Some(total_entries))?;
        for key in &group_keys {
            let mut nodes = self.groups[*key].clone();
            nodes.sort();
            map.serialize_entry(*key, &GroupHosts { hosts: &nodes })?;
        }
        map.serialize_entry(
            "_meta",
            &MetaHostvars {
                hostvars: &self.hostvars,
            },
        )?;
        map.end()
    }
}

impl YamlOutput for AnsibleInventory {
    fn to_yaml_value(&self, sorted: bool) -> Yaml {
        let mut group_keys: Vec<&String> = self.groups.keys().collect();
        if sorted {
            group_keys.sort();
        }
        let mut map = LinkedHashMap::new();
        for key in &group_keys {
            let mut nodes = self.groups[*key].clone();
            nodes.sort();
            let mut hosts_map = LinkedHashMap::new();
            hosts_map.insert(
                Yaml::String("hosts".to_string()),
                Yaml::Array(nodes.iter().map(|s| Yaml::String(s.clone())).collect()),
            );
            map.insert(Yaml::String(key.to_string()), Yaml::Hash(hosts_map));
        }
        let mut hostvars_keys: Vec<&String> = self.hostvars.keys().collect();
        if sorted {
            hostvars_keys.sort();
        }
        let mut hostvars_map = LinkedHashMap::new();
        for key in &hostvars_keys {
            hostvars_map.insert(
                Yaml::String(key.to_string()),
                self.hostvars[*key].to_yaml_value(sorted),
            );
        }
        let mut meta_map = LinkedHashMap::new();
        meta_map.insert(
            Yaml::String("hostvars".to_string()),
            Yaml::Hash(hostvars_map),
        );
        map.insert(Yaml::String("_meta".to_string()), Yaml::Hash(meta_map));
        Yaml::Hash(map)
    }
}

struct GroupHosts<'a> {
    hosts: &'a [String],
}

impl<'a> Serialize for GroupHosts<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("hosts", &self.hosts)?;
        map.end()
    }
}

struct MetaHostvars<'a> {
    hostvars: &'a LinkedHashMap<String, AnsibleNodeInfo>,
}

impl<'a> Serialize for MetaHostvars<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        let mut node_keys: Vec<&String> = self.hostvars.keys().collect();
        node_keys.sort();
        let hostvars_map = HostvarsMap {
            keys: node_keys,
            hostvars: self.hostvars,
        };
        map.serialize_entry("hostvars", &hostvars_map)?;
        map.end()
    }
}

struct HostvarsMap<'a> {
    keys: Vec<&'a String>,
    hostvars: &'a LinkedHashMap<String, AnsibleNodeInfo>,
}

impl<'a> Serialize for HostvarsMap<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.keys.len()))?;
        for key in &self.keys {
            map.serialize_entry(*key, &self.hostvars[*key])?;
        }
        map.end()
    }
}

/// Per-node metadata and resolved parameters for Ansible host variables.
pub struct AnsibleNodeInfo {
    pub node: String,
    pub uri: String,
    pub environment: String,
    pub timestamp: String,
    pub classes: Vec<String>,
    pub applications: Vec<String>,
    pub parameters: LinkedHashMap<Key, Value>,
    pub exports: LinkedHashMap<Key, Value>,
}

fn build_reclass_map(
    node: &str,
    uri: &str,
    environment: &str,
    timestamp: &str,
    classes: &[String],
    applications: &[String],
) -> LinkedHashMap<Key, Value> {
    let mut reclass = LinkedHashMap::new();
    reclass.insert(
        Key::String("node".to_string()),
        Value::String(node.to_string()),
    );
    reclass.insert(
        Key::String("name".to_string()),
        Value::String(node.to_string()),
    );
    reclass.insert(
        Key::String("uri".to_string()),
        Value::String(uri.to_string()),
    );
    reclass.insert(
        Key::String("environment".to_string()),
        Value::String(environment.to_string()),
    );
    reclass.insert(
        Key::String("timestamp".to_string()),
        Value::String(timestamp.to_string()),
    );
    reclass.insert(
        Key::String("applications".to_string()),
        Value::Array(Rc::new(
            applications
                .iter()
                .map(|s| Value::String(s.clone()))
                .collect(),
        )),
    );
    reclass.insert(
        Key::String("classes".to_string()),
        Value::Array(Rc::new(
            classes.iter().map(|s| Value::String(s.clone())).collect(),
        )),
    );
    reclass
}

impl Serialize for AnsibleNodeInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let reclass = build_reclass_map(
            &self.node,
            &self.uri,
            &self.environment,
            &self.timestamp,
            &self.classes,
            &self.applications,
        );

        let mut map = serializer.serialize_map(Some(self.parameters.len() + 1))?;
        map.serialize_entry("__reclass__", &ReclassMap(&reclass))?;
        for (k, v) in &self.parameters {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl YamlOutput for AnsibleNodeInfo {
    fn to_yaml_value(&self, sorted: bool) -> Yaml {
        let reclass = build_reclass_map(
            &self.node,
            &self.uri,
            &self.environment,
            &self.timestamp,
            &self.classes,
            &self.applications,
        );

        let mut map = LinkedHashMap::new();
        map.insert(
            Yaml::String("__reclass__".to_string()),
            Yaml::Hash(ReclassMap(&reclass).to_yaml_sorted(sorted)),
        );
        for (k, v) in &self.parameters {
            map.insert(k.to_yaml_value(), v.to_yaml_value_sorted(sorted));
        }
        Yaml::Hash(map)
    }
}

/// Errors that can occur while building host variables for a single node.
#[derive(Debug, Snafu)]
pub enum HostVarsError {
    #[snafu(transparent)]
    HostVarsInventoryLoad { source: inv::Error },
    #[snafu(display("node '{node_name}' not found"))]
    NodeNotFound { node_name: String },
    #[snafu(display("error merging node '{node_name}'"))]
    Merge {
        source: Box<inv::Error>,
        node_name: String,
    },
}

/// Resolved parameters for a single Ansible host.
pub struct HostVars {
    pub parameters: LinkedHashMap<Key, Value>,
}

impl Serialize for HostVars {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.parameters.len()))?;
        for (k, v) in &self.parameters {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl YamlOutput for HostVars {
    fn to_yaml_value(&self, sorted: bool) -> Yaml {
        Yaml::Hash(ReclassMap(&self.parameters).to_yaml_sorted(sorted))
    }
}

/// Insert `classes` and `applications` into the `__reclass__` key of a parameter map.
///
/// If the map already contains a `__reclass__` hash, the lists are added into it;
/// otherwise a new `__reclass__` hash is created. The `_reclass_` key from
/// automatic parameters is left untouched.
pub fn inject_classes_and_applications_into_reclass(
    parameters: &mut LinkedHashMap<Key, Value>,
    classes: &[String],
    applications: &[String],
) {
    let reclass_key = Key::String("__reclass__".to_string());
    let mut reclass_hash = match parameters.remove(&reclass_key) {
        Some(Value::Hash(h)) => Rc::try_unwrap(h).unwrap_or_else(|rc| (*rc).clone()),
        _ => LinkedHashMap::new(),
    };
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
    parameters.insert(reclass_key, Value::Hash(Rc::new(reclass_hash)));
}

/// Build a complete Ansible dynamic inventory from a pre-loaded inventory.
///
/// This is the core logic that works with an already-loaded and configured
/// [`Inventory`]. Use this when you already have an inventory (e.g., in an
/// LSP server that keeps one in memory).
///
/// `applications_postfix` is the suffix appended to group names (e.g. `"_grp"`).
/// `timestamp` is included in the `__reclass__` metadata.
/// `ignore_failed_node` and `ignore_failed_render` control error handling.
pub fn build_inventory_from(
    inventory: &inv::Inventory,
    applications_postfix: &str,
    timestamp: &str,
    ignore_failed_node: bool,
    ignore_failed_render: bool,
) -> Result<AnsibleInventory, AnsibleInventoryError> {
    let merge_config = inventory.merge_config();
    let ignore_failed_node = ignore_failed_node || merge_config.inventory_ignore_failed_node;
    let ignore_failed_render = ignore_failed_render || merge_config.inventory_ignore_failed_render;

    let inv_map = inventory
        .build_inventory_map()
        .map_err(AnsibleInventoryError::from)?;

    let mut groups: LinkedHashMap<String, Vec<String>> = LinkedHashMap::new();
    let mut hostvars: LinkedHashMap<String, AnsibleNodeInfo> = LinkedHashMap::new();

    for node in inventory.nodes_iter() {
        let merged = match inventory.merge_node(node.name()) {
            Ok(n) => n,
            Err(e) => {
                if ignore_failed_node {
                    tracing::warn!("Skipping node '{}': {}", node.name(), e);
                    continue;
                }
                return Err(AnsibleInventoryError::InventoryLoad { source: e });
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
                    return Err(AnsibleInventoryError::InventoryLoad { source: e });
                }
            }
        } else {
            merged
        };

        for class_name in merged.classes() {
            groups
                .entry(class_name.clone())
                .or_insert_with(Vec::new)
                .push(merged.name().to_string());
        }
        for app_name in merged.applications().as_list() {
            let group_name = format!("{}{}", app_name, applications_postfix);
            groups
                .entry(group_name)
                .or_insert_with(Vec::new)
                .push(merged.name().to_string());
        }

        let mut parameters = LinkedHashMap::new();
        for (k, v) in merged.parameters() {
            parameters.insert(k.clone(), v.clone());
        }

        let mut exports = LinkedHashMap::new();
        for (k, v) in merged.exports() {
            exports.insert(k.clone(), v.clone());
        }

        hostvars.insert(
            merged.name().to_string(),
            AnsibleNodeInfo {
                node: merged.name().to_string(),
                uri: merged.uri().unwrap_or("").to_string(),
                environment: merged.environment().to_string(),
                timestamp: timestamp.to_string(),
                classes: merged.classes().clone(),
                applications: merged.applications().as_list().to_vec(),
                parameters,
                exports,
            },
        );
    }

    for nodes in groups.values_mut() {
        nodes.sort();
        nodes.dedup();
    }

    Ok(AnsibleInventory { groups, hostvars })
}

/// Build a complete Ansible dynamic inventory from configuration.
///
/// Loads the inventory from disk, configures merge settings and class
/// mappings from the given [`Options`], and produces the full inventory.
/// See [`build_inventory_from`] for a variant that accepts a pre-loaded
/// inventory.
///
/// `applications_postfix` is the suffix appended to group names (e.g. `"_grp"`).
/// `timestamp` is included in the `__reclass__` metadata.
pub fn build_inventory(
    config: &Options,
    applications_postfix: &str,
    timestamp: &str,
) -> Result<AnsibleInventory, AnsibleInventoryError> {
    let merge_config = config.build_merge_config();
    let mut inventory = inv::load(&config.storage_options).map_err(AnsibleInventoryError::from)?;
    inventory.set_class_mappings(config.class_mappings.clone());
    inventory.set_class_mappings_match_path(config.class_mappings_match_path);
    inventory.set_merge_config(merge_config);

    build_inventory_from(
        &inventory,
        applications_postfix,
        timestamp,
        config.inventory_ignore_failed_node,
        config.inventory_ignore_failed_render,
    )
}

/// Build host variables for a single Ansible host from a pre-loaded inventory.
///
/// This is the core logic that works with an already-loaded and configured
/// [`Inventory`]. Use this when you already have an inventory (e.g., in an
/// LSP server that keeps one in memory).
///
/// Merges the node, resolves inventory queries, and returns the
/// `AnsibleNodeInfo` containing parameters, classes, applications, and
/// `__reclass__` metadata.
pub fn build_host_vars_from(
    inventory: &inv::Inventory,
    hostname: &str,
    timestamp: &str,
    ignore_failed_render: bool,
) -> Result<AnsibleNodeInfo, HostVarsError> {
    let ignore_failed_render =
        ignore_failed_render || inventory.merge_config().inventory_ignore_failed_render;

    let _node = inventory
        .get_node(hostname)
        .ok_or_else(|| HostVarsError::NodeNotFound {
            node_name: hostname.to_string(),
        })?;

    let merged = inventory
        .merge_node(hostname)
        .map_err(|e| HostVarsError::Merge {
            source: Box::new(e),
            node_name: hostname.to_string(),
        })?;

    let has_inv_query = {
        let params = merged.parameters();
        params.values().any(|v| v.has_inv_query())
            || merged.exports().values().any(|v| v.has_inv_query())
    };

    let merged = if has_inv_query {
        let inv_map = inventory
            .build_inventory_map()
            .map_err(HostVarsError::from)?;

        match inventory.merge_node_with_inventory(hostname, &inv_map) {
            Ok(n) => n,
            Err(e) => {
                if ignore_failed_render {
                    tracing::warn!(
                        "Failed to render inv queries for node '{}': {}",
                        hostname,
                        e
                    );
                }
                return Err(HostVarsError::Merge {
                    source: Box::new(e),
                    node_name: hostname.to_string(),
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

    let mut exports = LinkedHashMap::new();
    for (k, v) in merged.exports() {
        exports.insert(k.clone(), v.clone());
    }

    Ok(AnsibleNodeInfo {
        node: merged.name().to_string(),
        uri: merged.uri().unwrap_or("").to_string(),
        environment: merged.environment().to_string(),
        timestamp: timestamp.to_string(),
        classes: merged.classes().clone(),
        applications: merged.applications().as_list().to_vec(),
        parameters,
        exports,
    })
}

/// Build host variables for a single Ansible host.
///
/// Loads the inventory from disk, configures merge settings and class
/// mappings from the given [`Options`], and produces the host variables.
/// See [`build_host_vars_from`] for a variant that accepts a pre-loaded
/// inventory.
pub fn build_host_vars(
    config: &Options,
    hostname: &str,
    timestamp: &str,
) -> Result<AnsibleNodeInfo, HostVarsError> {
    let merge_config = config.build_merge_config();
    let mut inventory = inv::load(&config.storage_options).map_err(HostVarsError::from)?;
    inventory.set_class_mappings(config.class_mappings.clone());
    inventory.set_class_mappings_match_path(config.class_mappings_match_path);
    inventory.set_merge_config(merge_config);

    build_host_vars_from(
        &inventory,
        hostname,
        timestamp,
        config.inventory_ignore_failed_render,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_classes_and_applications_into_reclass() {
        let mut params = LinkedHashMap::new();
        let mut reclass_inner = LinkedHashMap::new();
        reclass_inner.insert(Key::from("name"), Value::String("test".to_string()));
        params.insert(
            Key::from("__reclass__"),
            Value::Hash(Rc::new(reclass_inner)),
        );
        params.insert(Key::from("hostname"), Value::String("web-01".to_string()));

        inject_classes_and_applications_into_reclass(
            &mut params,
            &["all".to_string(), "env.prod".to_string()],
            &["web".to_string()],
        );

        let reclass = params.get(&Key::from("__reclass__")).unwrap();
        if let Value::Hash(h) = reclass {
            let h = Rc::try_unwrap(h.clone()).unwrap_or_else(|rc| (*rc).clone());
            assert!(h.contains_key(&Key::from("name")));
            assert!(h.contains_key(&Key::from("classes")));
            assert!(h.contains_key(&Key::from("applications")));
        } else {
            panic!("expected Hash");
        }
    }

    #[test]
    fn test_inject_classes_and_applications_creates_key_when_absent() {
        let mut params = LinkedHashMap::new();
        params.insert(Key::from("hostname"), Value::String("web-01".to_string()));

        inject_classes_and_applications_into_reclass(
            &mut params,
            &["all".to_string()],
            &["web".to_string()],
        );

        assert!(params.contains_key(&Key::from("__reclass__")));
        assert!(!params.contains_key(&Key::from("_reclass_")));

        let reclass = params.get(&Key::from("__reclass__")).unwrap();
        if let Value::Hash(h) = reclass {
            let h = Rc::try_unwrap(h.clone()).unwrap_or_else(|rc| (*rc).clone());
            assert!(h.contains_key(&Key::from("classes")));
            assert!(h.contains_key(&Key::from("applications")));
        } else {
            panic!("expected Hash");
        }
    }
}
