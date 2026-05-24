// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Core inventory types, loading, and merging.
//!
//! This module contains [`Inventory`] (the central data structure that holds
//! all loaded classes and nodes), the [`load`] family of functions for reading
//! from disk or YAML strings, and [`Inventory::merge_node`]/[`Inventory::merge_class`]
//! for resolving inheritance and interpolation.
//!
//! Key re-exports:
//!
//! - [`Class`] and [`Node`] — the two fundamental element types
//! - [`MergeError`] and [`ValueMergeError`] — error types from the merge pipeline
//! - [`merge_values`] — the low-level value-merge function used by the pipeline

use crate::inventory::class_mapping::ClassMapping;
use crate::inventory::options::{
    MergeConfig, StorageOptions, StorageType, YamlFileStorageOptions, YamlFsStorageOptions,
};
use crate::inventory::value::{Environment, Key, ParametersType, Value};
use crate::storage::file_system;
use hashlink::LinkedHashMap;
use snafu::{ResultExt, Snafu};
use std::rc::Rc;

pub(crate) mod applications;
pub mod class_mapping;
pub mod elements;
pub(crate) mod interpolation;
pub(crate) mod inv_query;
pub(crate) mod merge;
pub mod options;
pub mod types;
pub mod value;
pub(crate) mod value_merge;

pub use elements::{Class, Node};
pub use merge::Error as MergeError;
pub use value_merge::Error as ValueMergeError;
pub use value_merge::merge as merge_values;

pub fn create_automatic_parameters(nodename: &str, environment: &Environment) -> ParametersType {
    let short_name = nodename.split('.').next().unwrap_or(nodename);
    let mut name_hash: ParametersType = LinkedHashMap::new();
    name_hash.insert(
        Key::String("full".to_string()),
        Value::String(nodename.to_string()),
    );
    name_hash.insert(
        Key::String("short".to_string()),
        Value::String(short_name.to_string()),
    );

    let mut reclass_hash: ParametersType = LinkedHashMap::new();
    reclass_hash.insert(
        Key::String("name".to_string()),
        Value::Hash(Rc::new(name_hash)),
    );
    reclass_hash.insert(
        Key::String("environment".to_string()),
        Value::String(environment.to_string()),
    );

    let mut params: ParametersType = LinkedHashMap::new();
    params.insert(
        Key::String("_reclass_".to_string()),
        Value::Hash(Rc::new(reclass_hash)),
    );
    params
}

/// The central data structure holding all loaded classes and nodes.
///
/// An `Inventory` is populated by [`load`] or [`load_from_yaml_string`] and
/// provides methods to query, iterate over, and merge individual nodes and
/// classes.
#[derive(Debug, Default)]
pub struct Inventory {
    classes: LinkedHashMap<String, Class>,
    nodes: LinkedHashMap<String, Node>,
    merge_config: MergeConfig,
    class_mappings: Vec<ClassMapping>,
    class_mappings_match_path: bool,
    input_data: Option<ParametersType>,
}

#[derive(Debug)]
pub struct Nodes<'a> {
    iterator: hashlink::linked_hash_map::Iter<'a, String, Node>,
}

impl<'a> Iterator for Nodes<'a> {
    type Item = &'a Node;

    fn next(&mut self) -> Option<Self::Item> {
        self.iterator.next().map(|(_, v)| v)
    }
}

#[derive(Debug)]
pub struct Classes<'a> {
    iterator: hashlink::linked_hash_map::Iter<'a, String, Class>,
}

impl<'a> Iterator for Classes<'a> {
    type Item = &'a Class;

    fn next(&mut self) -> Option<Self::Item> {
        self.iterator.next().map(|(_, v)| v)
    }
}

impl Inventory {
    /// Create an empty inventory with default merge configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty inventory with the given merge configuration.
    pub fn new_with_config(merge_config: MergeConfig) -> Self {
        Self {
            classes: LinkedHashMap::new(),
            nodes: LinkedHashMap::new(),
            merge_config,
            class_mappings: Vec::new(),
            class_mappings_match_path: false,
            input_data: None,
        }
    }

    /// Return the current merge configuration.
    pub fn merge_config(&self) -> &MergeConfig {
        &self.merge_config
    }

    /// Set the merge configuration, compiling any regex patterns.
    pub fn set_merge_config(&mut self, mut config: MergeConfig) {
        config.compile_regexps();
        self.merge_config = config;
    }

    /// Return the configured class mappings.
    pub fn class_mappings(&self) -> &[ClassMapping] {
        &self.class_mappings
    }

    /// Set the class mappings (glob/regex patterns that auto-include classes).
    pub fn set_class_mappings(&mut self, mappings: Vec<ClassMapping>) {
        self.class_mappings = mappings;
    }

    /// Whether class mapping patterns match against the node path instead of name.
    pub fn class_mappings_match_path(&self) -> bool {
        self.class_mappings_match_path
    }

    /// Set whether class mapping patterns match against the node path.
    pub fn set_class_mappings_match_path(&mut self, match_path: bool) {
        self.class_mappings_match_path = match_path;
    }

    /// Set extra input data to merge into every node (top-level defaults).
    pub fn set_input_data(&mut self, data: ParametersType) {
        self.input_data = Some(data);
    }

    /// Return the extra input data, if any.
    pub fn input_data(&self) -> Option<&ParametersType> {
        self.input_data.as_ref()
    }

    fn resolve_class_mappings_for_node(&self, node_name: &str) -> Vec<String> {
        let node = match self.get_node(node_name) {
            Some(n) => n,
            None => return Vec::new(),
        };
        class_mapping::resolve_class_mappings(
            &self.class_mappings,
            node.name(),
            node.pathname(),
            self.class_mappings_match_path,
        )
    }

    fn add_class(&mut self, class: Class) {
        self.classes.insert(class.name().to_string(), class);
    }

    fn add_node(&mut self, node: Node) -> Result<(), Error> {
        if let Some(existing) = self.nodes.get(node.name()) {
            return Err(Error::DuplicateNodeName {
                name: node.name().to_string(),
                existing_uri: existing.uri().unwrap_or("").to_string(),
                new_uri: node.uri().unwrap_or("").to_string(),
            });
        }
        self.nodes.insert(node.name().to_string(), node);
        Ok(())
    }

    /// Iterate over all loaded nodes.
    pub fn nodes_iter(&self) -> Nodes<'_> {
        Nodes {
            iterator: self.nodes.iter(),
        }
    }

    /// Iterate over all loaded classes.
    pub fn classes_iter(&self) -> Classes<'_> {
        Classes {
            iterator: self.classes.iter(),
        }
    }

    /// Look up a class by name.
    pub fn get_class(&self, name: &str) -> Option<&Class> {
        self.classes.get(name)
    }

    /// Look up a node by name.
    pub fn get_node(&self, name: &str) -> Option<&Node> {
        self.nodes.get(name)
    }

    /// Resolve the full inheritance chain for a class and return the merged result.
    pub fn merge_class(&self, class_name: &str) -> Result<Class, Error> {
        let class = self
            .get_class(class_name)
            .ok_or_else(|| Error::ClassNotFound {
                class_name: class_name.to_string(),
            })?;
        merge::merge_class(self, class, &self.merge_config).map_err(Into::into)
    }

    /// Resolve the full inheritance and interpolation chain for a node.
    ///
    /// This is the primary method for obtaining a fully-merged node. It applies
    /// class mappings, resolves the inheritance chain, interpolates `$[...]`
    /// references, processes inventory queries, and adds `_reclass_` metadata.
    pub fn merge_node(&self, node_name: &str) -> Result<Node, Error> {
        let node = self
            .get_node(node_name)
            .ok_or_else(|| Error::NodeNotFound {
                node_name: node_name.to_string(),
            })?;
        let extra_classes = self.resolve_class_mappings_for_node(node_name);
        merge::merge_node(
            self,
            node,
            &extra_classes,
            &self.merge_config,
            self.input_data.as_ref(),
        )
        .map_err(Into::into)
    }

    /// Return the names of all loaded nodes in insertion order.
    pub fn node_names(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    /// Build a map of all merged nodes for inventory-query resolution.
    ///
    /// Each entry maps a node name to its merged exports and environment.
    /// Failed nodes are skipped if `ignore_failed_node` is enabled in the
    /// merge configuration.
    pub fn build_inventory_map(&self) -> Result<inv_query::InventoryMap, Error> {
        let mut inventory_map = inv_query::InventoryMap::new();
        for node_name in self.node_names() {
            let node = match self.merge_node(&node_name) {
                Ok(n) => n,
                Err(_) => {
                    if self.merge_config.inventory_ignore_failed_node {
                        tracing::warn!(
                            "Ignoring failed node '{}' during inventory build",
                            node_name
                        );
                        continue;
                    }
                    return Err(Error::NodeNotFound {
                        node_name: node_name.clone(),
                    });
                }
            };
            inventory_map.insert(
                node_name,
                inv_query::NodeInventory {
                    items: node.exports().clone(),
                    environment: node.environment().clone(),
                },
            );
        }
        Ok(inventory_map)
    }

    /// Like [`merge_node`](Inventory::merge_node), but resolves inventory queries
    /// against the provided pre-built inventory map instead of building one on the fly.
    pub fn merge_node_with_inventory(
        &self,
        node_name: &str,
        inventory: &inv_query::InventoryMap,
    ) -> Result<Node, Error> {
        let node = self
            .get_node(node_name)
            .ok_or_else(|| Error::NodeNotFound {
                node_name: node_name.to_string(),
            })?;
        let extra_classes = self.resolve_class_mappings_for_node(node_name);
        merge::merge_node_with_inventory(
            self,
            node,
            &extra_classes,
            &self.merge_config,
            inventory,
            self.input_data.as_ref(),
        )
        .map_err(Into::into)
    }
}

/// Errors that can occur during inventory loading or node/class merging.
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("error loading repository at '{base_uri}'"))]
    Repository {
        source: file_system::Error,
        base_uri: String,
    },
    #[snafu(display("class '{class_name}' not found"))]
    ClassNotFound { class_name: String },
    #[snafu(display("node '{node_name}' not found"))]
    NodeNotFound { node_name: String },
    #[snafu(transparent)]
    Interpolation { source: interpolation::Error },
    #[snafu(transparent)]
    ValueMerge { source: value_merge::Error },
    #[snafu(display("node '{name}' defined in both '{existing_uri}' and '{new_uri}'"))]
    DuplicateNodeName {
        name: String,
        existing_uri: String,
        new_uri: String,
    },
}

impl From<merge::Error> for Error {
    fn from(e: merge::Error) -> Self {
        match e {
            merge::Error::ClassNotFound { class_name } => Error::ClassNotFound { class_name },
            merge::Error::ClassNameResolveError {
                class_name,
                source: _,
            } => Error::ClassNotFound { class_name },
            merge::Error::Interpolation { source } => Error::Interpolation { source },
            merge::Error::ValueMerge { source } => Error::ValueMerge { source },
        }
    }
}

/// Load an inventory from a storage backend (directory tree or single file).
///
/// This is the primary entry point for loading inventory data. The storage
/// backend and path are determined by [`StorageOptions`].
///
/// [`StorageOptions`]: crate::inventory::options::StorageOptions
pub fn load(storage_options: &StorageOptions) -> Result<Inventory, Error> {
    match storage_options.storage_type {
        StorageType::YamlFs => load_yaml_fs(&storage_options.yaml_fs_options),
        StorageType::YamlFile => load_yaml_file(&storage_options.yaml_file_options),
    }
}

/// Load an inventory from a YAML string (for testing or embedded data).
///
/// Uses the default base URI of `<memory>`. See [`load_from_yaml_string_with_uri`]
/// for specifying a custom base URI.
pub fn load_from_yaml_string(
    yaml_content: &str,
    parameter_key_style: &options::ParameterKeyStyle,
) -> Result<Inventory, Error> {
    load_from_yaml_string_with_uri(yaml_content, parameter_key_style, None)
}

/// Load an inventory from a YAML string with an explicit base URI.
///
/// The `base_uri` is used in error messages to identify where the data came
/// from. Pass `None` to use `<memory>` as the base URI.
pub fn load_from_yaml_string_with_uri(
    yaml_content: &str,
    parameter_key_style: &options::ParameterKeyStyle,
    base_uri: Option<&str>,
) -> Result<Inventory, Error> {
    let default_environment = Environment::default();
    let (_metadata, classes, nodes) = file_system::YamlFileRepository::load_from_string(
        yaml_content,
        parameter_key_style,
        base_uri,
        &default_environment,
    )
    .context(RepositorySnafu {
        base_uri: base_uri.unwrap_or("<memory>").to_string(),
    })?;
    let mut inventory = Inventory::new();
    for class in classes {
        inventory.add_class(class);
    }
    for node in nodes {
        inventory.add_node(node)?;
    }
    Ok(inventory)
}

fn load_yaml_fs(storage_options: &YamlFsStorageOptions) -> Result<Inventory, Error> {
    let base_uri = storage_options.inventory_base_uri.clone();
    let mut inventory = Inventory::new();
    let repo = file_system::YamlFsRepository::new(
        storage_options,
        storage_options.parameter_key_style.clone(),
    )
    .context(RepositorySnafu {
        base_uri: base_uri.clone(),
    })?;
    tracing::debug!(
        path = storage_options.classes_path().to_string_lossy().as_ref(),
        "loading classes"
    );
    for class in repo.classes_iter() {
        match class {
            Ok(class) => inventory.add_class(class),
            Err(e) => {
                return Err(Error::Repository {
                    source: e,
                    base_uri: base_uri.clone(),
                });
            }
        }
    }

    tracing::debug!(
        path = storage_options.nodes_path().to_string_lossy().as_ref(),
        "loading nodes"
    );

    for node in repo.nodes_iter() {
        match node {
            Ok(node) => inventory.add_node(node)?,
            Err(e) => {
                return Err(Error::Repository {
                    source: e,
                    base_uri: base_uri.clone(),
                });
            }
        }
    }
    Ok(inventory)
}

fn load_yaml_file(storage_options: &YamlFileStorageOptions) -> Result<Inventory, Error> {
    let base_uri = storage_options.inventory_file.clone();
    let mut inventory = Inventory::new();
    let repo = file_system::YamlFileRepository::new(
        storage_options,
        storage_options.parameter_key_style.clone(),
    )
    .context(RepositorySnafu {
        base_uri: base_uri.clone(),
    })?;
    tracing::debug!(
        path = repo.file_path().to_string_lossy().as_ref(),
        "loading from YAML file"
    );

    let (_metadata, classes, nodes) = repo.load().context(RepositorySnafu { base_uri })?;

    for class in classes {
        inventory.add_class(class);
    }
    for node in nodes {
        inventory.add_node(node)?;
    }
    Ok(inventory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::options::ParameterKeyStyle;

    #[test]
    fn test_create_automatic_parameters_full_name() {
        let env = Environment::from("production");
        let params = create_automatic_parameters("web01.example.com", &env);

        let reclass_key = Key::String("_reclass_".to_string());
        assert!(
            params.contains_key(&reclass_key),
            "should contain _reclass_ key"
        );

        let reclass_val = params.get(&reclass_key).unwrap();
        match reclass_val {
            Value::Hash(h) => {
                let name_key = Key::String("name".to_string());
                assert!(h.contains_key(&name_key), "should contain name key");
                let name_hash = h.get(&name_key).unwrap();
                match name_hash {
                    Value::Hash(nh) => {
                        assert_eq!(
                            nh.get(&Key::String("full".to_string())),
                            Some(&Value::String("web01.example.com".to_string()))
                        );
                        assert_eq!(
                            nh.get(&Key::String("short".to_string())),
                            Some(&Value::String("web01".to_string()))
                        );
                    }
                    _ => panic!("name should be a Hash"),
                }
                assert_eq!(
                    h.get(&Key::String("environment".to_string())),
                    Some(&Value::String("production".to_string()))
                );
            }
            _ => panic!("_reclass_ should be a Hash"),
        }
    }

    #[test]
    fn test_create_automatic_parameters_short_name_no_dots() {
        let env = Environment::from("base");
        let params = create_automatic_parameters("laptop", &env);

        let reclass_key = Key::String("_reclass_".to_string());
        let reclass_val = params.get(&reclass_key).unwrap();
        match reclass_val {
            Value::Hash(h) => {
                let name_hash = h.get(&Key::String("name".to_string())).unwrap();
                match name_hash {
                    Value::Hash(nh) => {
                        assert_eq!(
                            nh.get(&Key::String("full".to_string())),
                            Some(&Value::String("laptop".to_string()))
                        );
                        assert_eq!(
                            nh.get(&Key::String("short".to_string())),
                            Some(&Value::String("laptop".to_string()))
                        );
                    }
                    _ => panic!("name should be a Hash"),
                }
            }
            _ => panic!("_reclass_ should be a Hash"),
        }
    }

    #[test]
    fn test_merge_node_includes_automatic_parameters() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: all
               parameters:
                   global: true
               ---
               name: node
               type: node
               classes:
                   - all
               parameters:
                   hostname: myhost
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let node = inventory.merge_node("node").expect("failed to merge node");

        let params = node.parameters();
        let reclass_key = Key::String("_reclass_".to_string());
        assert!(
            params.contains_key(&reclass_key),
            "merged node should contain _reclass_ automatic parameter"
        );
        assert!(
            params.contains_key(&Key::String("hostname".to_string())),
            "merged node should contain hostname"
        );
        assert!(
            params.contains_key(&Key::String("global".to_string())),
            "merged node should contain global from inherited class"
        );
    }

    #[test]
    fn test_merge_node_automatic_parameters_overridden_by_class() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: custom_reclass
               parameters:
                   _reclass_:
                       custom: value
               ---
               name: node
               type: node
               classes:
                   - custom_reclass
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let node = inventory.merge_node("node").expect("failed to merge node");

        let params = node.parameters();
        let reclass_key = Key::String("_reclass_".to_string());
        let reclass_val = params.get(&reclass_key).unwrap();

        match reclass_val {
            Value::Hash(h) => {
                assert!(
                    h.contains_key(&Key::String("custom".to_string())),
                    "class-provided _reclass_.custom should be present"
                );
                assert!(
                    h.contains_key(&Key::String("name".to_string())),
                    "automatic _reclass_.name should also be present"
                );
            }
            _ => panic!("_reclass_ should be a Hash"),
        }
    }

    #[test]
    fn test_automatic_parameters_disabled() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: all
               parameters:
                   global: true
               ---
               name: node
               type: node
               classes:
                   - all
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let config = MergeConfig {
            automatic_parameters: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory.merge_node("node").expect("failed to merge node");

        let params = node.parameters();
        let reclass_key = Key::String("_reclass_".to_string());
        assert!(
            !params.contains_key(&reclass_key),
            "merged node should NOT contain _reclass_ when automatic_parameters is disabled"
        );
    }

    #[test]
    fn test_class_mappings_glob_with_merge_node() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: default
               parameters:
                   default_setting: true
               ---
               name: webserver
               parameters:
                   is_webserver: true
               ---
               name: node1
               type: node
               parameters:
                   hostname: node1
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let mapping =
            crate::inventory::class_mapping::ClassMapping::parse("node* default webserver")
                .expect("failed to parse mapping");
        inventory.set_class_mappings(vec![mapping]);

        let node = inventory.merge_node("node1").expect("failed to merge node");

        let params = node.parameters();
        assert!(
            params.contains_key(&Key::String("hostname".to_string())),
            "should contain node's own parameter"
        );
        assert!(
            params.contains_key(&Key::String("default_setting".to_string())),
            "should contain parameter from mapped 'default' class"
        );
        assert!(
            params.contains_key(&Key::String("is_webserver".to_string())),
            "should contain parameter from mapped 'webserver' class"
        );
    }

    #[test]
    fn test_class_mappings_regex_with_backreference() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: env-production
               parameters:
                   env: prod
               ---
               name: www1.production
               type: node
               parameters:
                   role: web
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let mapping = crate::inventory::class_mapping::ClassMapping::parse("/\\.(\\S+)$/ env-\\1")
            .expect("failed to parse mapping");
        inventory.set_class_mappings(vec![mapping]);

        let node = inventory
            .merge_node("www1.production")
            .expect("failed to merge node");

        let params = node.parameters();
        assert!(
            params.contains_key(&Key::String("env".to_string())),
            "should contain parameter from mapped 'env-production' class"
        );
        assert!(
            params.contains_key(&Key::String("role".to_string())),
            "should contain node's own parameter"
        );
    }

    #[test]
    fn test_class_mappings_no_match() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: default
               parameters:
                   default_setting: true
               ---
               name: node1
               type: node
               parameters:
                   hostname: node1
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let mapping = crate::inventory::class_mapping::ClassMapping::parse("*.ch swiss").unwrap();
        inventory.set_class_mappings(vec![mapping]);

        let node = inventory.merge_node("node1").expect("failed to merge node");

        let params = node.parameters();
        assert!(
            !params.contains_key(&Key::String("default_setting".to_string())),
            "should NOT contain parameter from unmapped class"
        );
        assert!(
            params.contains_key(&Key::String("hostname".to_string())),
            "should contain node's own parameter"
        );
    }

    #[test]
    fn test_class_mappings_mapped_classes_before_node_classes() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   priority: low
               ---
               name: override_class
               parameters:
                   priority: high
               ---
               name: node1
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let mapping = crate::inventory::class_mapping::ClassMapping::parse("node* base").unwrap();
        inventory.set_class_mappings(vec![mapping]);

        let node = inventory.merge_node("node1").expect("failed to merge node");

        let params = node.parameters();
        assert_eq!(
            params.get(&Key::String("priority".to_string())),
            Some(&Value::String("high".to_string())),
            "node's own class (override_class) should override mapped class (base)"
        );
    }

    // --- Tilde override integration tests ---
    // These test the full merge pipeline (multiple classes inheriting)
    // to expose the bug where ~key override semantics are lost across merges.

    #[test]
    fn test_tilde_override_dict_in_multi_class_merge() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   "~accounts":
                       local: true
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        match accounts {
            Value::Hash(h) => {
                assert_eq!(
                    h.len(),
                    1,
                    "accounts should have exactly 1 key after ~override, got {}: {:?}",
                    h.len(),
                    h
                );
                assert!(
                    h.contains_key(&Key::String("local".to_string())),
                    "accounts should contain 'local' key, got: {:?}",
                    h
                );
                assert!(
                    !h.contains_key(&Key::String("ssh".to_string())),
                    "accounts should NOT contain 'ssh' key after ~override"
                );
                assert!(
                    !h.contains_key(&Key::String("ldap".to_string())),
                    "accounts should NOT contain 'ldap' key after ~override"
                );
            }
            _ => panic!("accounts should be a Hash, got {:?}", accounts),
        }
    }

    #[test]
    fn test_tilde_override_empty_dict_in_multi_class_merge() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   "~accounts": {}
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        match accounts {
            Value::Hash(h) => {
                assert_eq!(
                    h.len(),
                    0,
                    "accounts should be empty after ~accounts: {{}}, got {} keys: {:?}",
                    h.len(),
                    h
                );
            }
            _ => panic!("accounts should be a Hash, got {:?}", accounts),
        }
    }

    #[test]
    fn test_tilde_override_list_in_multi_class_merge() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   ports:
                       - 80
                       - 443
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   "~ports":
                       - 8080
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let ports = params
            .get(&Key::String("ports".to_string()))
            .expect("ports key should exist");
        match ports {
            Value::Array(arr) => {
                assert_eq!(
                    arr.len(),
                    1,
                    "ports should have exactly 1 element after ~override, got {}: {:?}",
                    arr.len(),
                    arr
                );
                assert_eq!(arr[0], Value::Integer(8080));
            }
            _ => panic!("ports should be an Array, got {:?}", ports),
        }
    }

    #[test]
    fn test_tilde_override_null_with_allow_none_in_multi_class_merge() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   "~accounts": null
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: true,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        assert_eq!(
            accounts,
            &Value::Null,
            "accounts should be Null after ~accounts: null, got {:?}",
            accounts
        );
    }

    #[test]
    fn test_tilde_override_scalar_in_multi_class_merge() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   port: 80
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   "~port": 443
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        assert_eq!(
            params.get(&Key::String("port".to_string())),
            Some(&Value::Integer(443)),
            "port should be 443 after ~port: 443 override"
        );
    }

    #[test]
    fn test_tilde_override_three_class_chain() {
        use indoc::indoc;

        // Class chain: grandparent -> parent -> child
        // grandparent defines accounts with ssh and ldap
        // parent overrides accounts with just local
        // child should see accounts: {local: true} (not deep-merged)
        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: grandparent
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: parent
               classes:
                   - grandparent
               parameters:
                   "~accounts":
                       local: true
               ---
               name: child_class
               classes:
                   - parent
               parameters:
                   extra: true
               ---
               name: test_node
               type: node
               classes:
                   - child_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        match accounts {
            Value::Hash(h) => {
                assert_eq!(
                    h.len(),
                    1,
                    "accounts should have exactly 1 key after ~override in chain, got {}: {:?}",
                    h.len(),
                    h
                );
                assert!(
                    h.contains_key(&Key::String("local".to_string())),
                    "accounts should contain 'local' key"
                );
                assert!(
                    !h.contains_key(&Key::String("ssh".to_string())),
                    "accounts should NOT contain 'ssh' key"
                );
            }
            _ => panic!("accounts should be a Hash, got {:?}", accounts),
        }
    }

    // This test exposes the ACTUAL bug: when a class clears accounts with ~accounts: {}
    // but another sibling class (processed before it) has accounts with content,
    // the empty dict gets deep-merged into the parent accumulator and the override is lost.
    #[test]
    fn test_tilde_override_empty_dict_lost_across_merge_chain() {
        use indoc::indoc;

        // Simulates the real-world scenario:
        // - base_accounts defines accounts: {ssh: true, ldap: true}
        // - ansible_provisioned (includes base_accounts) defines accounts: {admin1: {...}}
        // - domain_class (includes base_accounts) defines ~accounts: {} to clear it
        // - Node inherits: ansible_provisioned, then domain_class
        // - After domain_class ~accounts: {}, all accounts content should be gone
        // But due to the bug, domain_class's empty accounts gets deep-merged into
        // ansible_provisioned's accounts, losing the override.
        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_accounts
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: ansible_provisioned
               classes:
                   - base_accounts
               parameters:
                   accounts:
                        admin1:
                           local: true
               ---
               name: domain_class
               classes:
                   - base_accounts
               parameters:
                   "~accounts": {}
               ---
               name: test_node
               type: node
               classes:
                   - ansible_provisioned
                   - domain_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        match accounts {
            Value::Hash(h) => {
                // The ~accounts: {} should have cleared all account content.
                // domain_class clears accounts to {}, then the node doesn't add any.
                // Expected: accounts is empty {}
                // Bug: accounts still contains admin1 because ~accounts: {}
                // in domain_class gets deep-merged (no-op) with ansible_provisioned's accounts
                assert_eq!(
                    h.len(),
                    0,
                    "accounts should be empty after ~override with empty dict, got {} keys: {:?}",
                    h.len(),
                    h
                );
            }
            _ => panic!("accounts should be a Hash, got {:?}", accounts),
        }
    }

    // Test that ~override with content replaces across sibling merge
    #[test]
    fn test_tilde_override_dict_content_across_sibling_merge() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_accounts
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: ansible_provisioned
               classes:
                   - base_accounts
               parameters:
                   accounts:
                        admin1:
                           local: true
               ---
               name: domain_class
               classes:
                   - base_accounts
               parameters:
                   "~accounts":
                       ad: true
               ---
               name: test_node
               type: node
               classes:
                   - ansible_provisioned
                   - domain_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        match accounts {
            Value::Hash(h) => {
                // domain_class's ~accounts: {ad: true} should completely replace
                // all prior accounts, so result should be {ad: true} only
                assert_eq!(
                    h.len(),
                    1,
                    "accounts should have exactly 1 key after ~override across siblings, got {}: {:?}",
                    h.len(),
                    h
                );
                assert!(
                    h.contains_key(&Key::String("ad".to_string())),
                    "accounts should contain 'ad' key"
                );
                assert!(
                    !h.contains_key(&Key::String("ssh".to_string())),
                    "accounts should NOT contain 'ssh' key"
                );
                assert!(
                    !h.contains_key(&Key::String("admin1".to_string())),
                    "accounts should NOT contain 'admin1' key"
                );
            }
            _ => panic!("accounts should be a Hash, got {:?}", accounts),
        }
    }

    // Test allow_none_override across the merge chain (sibling classes)
    #[test]
    fn test_allow_none_override_across_sibling_merge() {
        use indoc::indoc;

        // Sibling classes: ansible_provisioned sets accounts, domain_class clears with null
        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_accounts
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: ansible_provisioned
               classes:
                   - base_accounts
               parameters:
                   accounts:
                        admin1:
                           local: true
               ---
               name: domain_class
               classes:
                   - base_accounts
               parameters:
                   "~accounts":
               ---
               name: test_node
               type: node
               classes:
                   - ansible_provisioned
                   - domain_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: true,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        // With allow_none_override and ~accounts: null, all account content should be gone
        assert_eq!(
            accounts,
            &Value::Null,
            "accounts should be Null after ~accounts: null with allow_none_override, got {:?}",
            accounts
        );
    }

    // Test that null (without tilde) overrides dict/list with allow_none across sibling merge
    #[test]
    fn test_allow_none_override_no_tilde_across_sibling_merge() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_accounts
               parameters:
                   accounts:
                       ssh: true
               ---
               name: ansible_provisioned
               classes:
                   - base_accounts
               parameters:
                   accounts:
                        admin1:
                           local: true
               ---
               name: clearer
               parameters:
                   accounts:
               ---
               name: test_node
               type: node
               classes:
                   - ansible_provisioned
                   - clearer
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: true,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        // Without tilde, null should still replace dict when allow_none_override is true
        assert_eq!(
            accounts,
            &Value::Null,
            "accounts should be Null after accounts: null with allow_none_override, got {:?}",
            accounts
        );
    }

    // Test: override-clear-set pattern — set a value, ~override to clear, then set a new value
    // class base: accounts: {ssh: true, ldap: true}
    // class override_class: ~accounts: {} (clear)
    // class new_accounts: accounts: {local: true} (new value after clear)
    // node inherits: override_class, new_accounts
    // Expected: accounts: {local: true} (new_accounts replaces the cleared value)
    #[test]
    fn test_tilde_override_clear_then_set_new_value() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: override_class
               classes:
                   - base
               parameters:
                   "~accounts": {}
               ---
               name: new_accounts
               parameters:
                   accounts:
                       local: true
               ---
               name: test_node
               type: node
               classes:
                   - override_class
                   - new_accounts
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        match accounts {
            Value::Hash(h) => {
                assert_eq!(
                    h.len(),
                    1,
                    "accounts should have exactly 1 key, got {}: {:?}",
                    h.len(),
                    h
                );
                assert!(
                    h.contains_key(&Key::String("local".to_string())),
                    "accounts should contain 'local' key, got {:?}",
                    h
                );
                assert!(
                    !h.contains_key(&Key::String("ssh".to_string())),
                    "accounts should NOT contain 'ssh' key after override+new set"
                );
                assert!(
                    !h.contains_key(&Key::String("ldap".to_string())),
                    "accounts should NOT contain 'ldap' key after override+new set"
                );
            }
            _ => panic!("accounts should be a Hash, got {:?}", accounts),
        }
    }

    // Test: three-class chain — base sets key, mid clears with ~override, top sets new value
    // class base: key: {a: 1}
    // class mid (inherits base): ~key: {} (clear)
    // class top (inherits mid): key: {b: 2} (new value)
    // Expected: key: {b: 2} (top's value should replace the cleared value)
    #[test]
    fn test_tilde_override_three_class_chain_clear_then_set() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   key:
                       a: 1
               ---
               name: mid
               classes:
                   - base
               parameters:
                   "~key": {}
               ---
               name: top
               classes:
                   - mid
               parameters:
                   key:
                       b: 2
               ---
               name: test_node
               type: node
               classes:
                   - top
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let key = params
            .get(&Key::String("key".to_string()))
            .expect("key should exist");
        match key {
            Value::Hash(h) => {
                assert_eq!(
                    h.len(),
                    1,
                    "key should have exactly 1 key, got {}: {:?}",
                    h.len(),
                    h
                );
                assert!(
                    h.contains_key(&Key::String("b".to_string())),
                    "key should contain 'b', got {:?}",
                    h
                );
                assert!(
                    !h.contains_key(&Key::String("a".to_string())),
                    "key should NOT contain 'a' after override+new set"
                );
            }
            _ => panic!("key should be a Hash, got {:?}", key),
        }
    }

    // Test: list override followed by new list — does it append or replace?
    // class base: ports: [22, 80]
    // class override_class (inherits base): ~ports: [443] (replace)
    // class new_ports: ports: [8080] (new value, no tilde)
    // Expected: ports: [443, 8080] (override replaces, then new value merges/appends)
    // In Python reclass: after ~ports: [443], ports is [443]; then ports: [8080] appends → [443, 8080]
    #[test]
    fn test_tilde_override_list_then_append() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   ports:
                       - 22
                       - 80
               ---
               name: override_class
               classes:
                   - base
               parameters:
                   "~ports":
                       - 443
               ---
               name: new_ports
               parameters:
                   ports:
                       - 8080
               ---
               name: test_node
               type: node
               classes:
                   - override_class
                   - new_ports
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let ports = params
            .get(&Key::String("ports".to_string()))
            .expect("ports key should exist");
        match ports {
            Value::Array(arr) => {
                assert!(
                    !arr.iter().any(|v| v == &Value::Integer(22)),
                    "ports should NOT contain 22 after ~override, got {:?}",
                    arr
                );
                assert!(
                    !arr.iter().any(|v| v == &Value::Integer(80)),
                    "ports should NOT contain 80 after ~override, got {:?}",
                    arr
                );
                assert!(
                    arr.iter().any(|v| v == &Value::Integer(443)),
                    "ports should contain 443, got {:?}",
                    arr
                );
                assert!(
                    arr.iter().any(|v| v == &Value::Integer(8080)),
                    "ports should contain 8080 from new_ports, got {:?}",
                    arr
                );
            }
            _ => panic!("ports should be an Array, got {:?}", ports),
        }
    }

    // Test: deeply nested override — override at nested key level
    // class base: config: {db: {host: localhost, port: 3306}}
    // class override_class: ~config: {db: {host: prod-db}} (replace entire config)
    // Expected: config: {db: {host: prod-db}} — no port, no localhost
    #[test]
    fn test_tilde_override_deeply_nested() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   config:
                       db:
                           host: localhost
                           port: 3306
               ---
               name: override_class
               classes:
                   - base
               parameters:
                   "~config":
                       db:
                           host: prod-db
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let config_val = params
            .get(&Key::String("config".to_string()))
            .expect("config key should exist");
        match config_val {
            Value::Hash(h) => {
                let db = h
                    .get(&Key::String("db".to_string()))
                    .expect("db should exist");
                match db {
                    Value::Hash(db_h) => {
                        assert!(
                            !db_h.contains_key(&Key::String("port".to_string())),
                            "db should NOT contain 'port' after ~override, got {:?}",
                            db_h
                        );
                        assert_eq!(
                            db_h.get(&Key::String("host".to_string())),
                            Some(&Value::String("prod-db".to_string())),
                            "db.host should be prod-db, got {:?}",
                            db_h
                        );
                    }
                    _ => panic!("db should be a Hash, got {:?}", db),
                }
            }
            _ => panic!("config should be a Hash, got {:?}", config_val),
        }
    }

    // Test: list values from different sibling classes get appended (Python reclass behavior).
    // This is expected: lists append across sibling merges.
    #[test]
    fn test_list_merge_appends_across_sibling_classes() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_vm
               parameters:
                   pve_vm:
                       description:
                           - "Base VM description"
               ---
               name: vlan_class
               parameters:
                   pve_vm:
                       vlan_tag: "11"
               ---
               name: node_class
               parameters:
                   pve_vm:
                       description:
                           - "Node-specific description"
               ---
               name: test_node
               type: node
               classes:
                   - base_vm
                   - vlan_class
                   - node_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let pve_vm = params
            .get(&Key::String("pve_vm".to_string()))
            .expect("pve_vm key should exist");
        match pve_vm {
            Value::Hash(h) => {
                let desc = h
                    .get(&Key::String("description".to_string()))
                    .expect("description should exist");
                match desc {
                    Value::Array(arr) => {
                        // Lists from sibling classes append (Python reclass behavior)
                        assert_eq!(
                            arr.len(),
                            2,
                            "description should have 2 elements (appended from both classes), got {}: {:?}",
                            arr.len(),
                            arr
                        );
                    }
                    _ => panic!("description should be an Array, got {:?}", desc),
                }
            }
            _ => panic!("pve_vm should be a Hash, got {:?}", pve_vm),
        }
    }

    // Test: the SAME list value should not be duplicated when it comes through
    // a class that is inherited by multiple sibling classes (diamond inheritance).
    // class root: pve_vm: {description: ["Root desc"]}
    // class left (inherits root): (adds nothing to pve_vm)
    // class right (inherits root): (adds nothing to pve_vm)
    // node inherits: left, right
    // Expected: description has 1 element (root), NOT 2 (left+right both copy root)
    #[test]
    fn test_diamond_inheritance_does_not_duplicate_list_values() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: root
               parameters:
                   pve_vm:
                       description:
                           - "Root description"
               ---
               name: left
               classes:
                   - root
               ---
               name: right
               classes:
                   - root
               ---
               name: test_node
               type: node
               classes:
                   - left
                   - right
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let pve_vm = params
            .get(&Key::String("pve_vm".to_string()))
            .expect("pve_vm key should exist");
        match pve_vm {
            Value::Hash(h) => {
                let desc = h
                    .get(&Key::String("description".to_string()))
                    .expect("description should exist");
                match desc {
                    Value::Array(arr) => {
                        assert_eq!(
                            arr.len(),
                            1,
                            "description should have 1 element (root counted once despite diamond), got {}: {:?}",
                            arr.len(),
                            arr
                        );
                        assert_eq!(
                            arr[0],
                            Value::String("Root description".to_string()),
                            "description should be 'Root description', got {:?}",
                            arr
                        );
                    }
                    _ => panic!("description should be an Array, got {:?}", desc),
                }
            }
            _ => panic!("pve_vm should be a Hash, got {:?}", pve_vm),
        }
    }

    // Test: real-world-style accounts override — the real bug scenario
    // class accounts: accounts: {}
    // class ansible_provisioned (includes people): accounts: {admin1: {local: true}}
    // class domain_class (includes accounts): ~accounts: {} (clear all accounts)
    // node inherits: ansible_provisioned, domain_class
    // Expected: accounts: {} (domain_class clears everything)
    #[test]
    fn test_tilde_override_accounts_scenario() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: accounts
               parameters:
                   accounts: {}
                   account_groups: {}
               ---
               name: people
               parameters:
                   people: {}
               ---
               name: ansible_provisioned
               classes:
                   - accounts
                   - people
               parameters:
                   accounts:
                       admin1:
                           local: true
                           ssh-key: "ssh-ed25519 AAAA"
                       deploy1:
                           local: true
               ---
               name: domain_class
               classes:
                   - accounts
               parameters:
                   "~accounts": {}
                   ad_domain:
                       name: dc1.example.com
               ---
               name: test_node
               type: node
               classes:
                   - ansible_provisioned
                   - domain_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        match accounts {
            Value::Hash(h) => {
                assert_eq!(
                    h.len(),
                    0,
                    "accounts should be empty after ~accounts: {{}} override, got {} keys: {:?}",
                    h.len(),
                    h
                );
                assert!(
                    !h.contains_key(&Key::String("admin1".to_string())),
                    "accounts should NOT contain 'admin1' after override"
                );
                assert!(
                    !h.contains_key(&Key::String("deploy1".to_string())),
                    "accounts should NOT contain 'deploy1' after override"
                );
            }
            _ => panic!("accounts should be a Hash, got {:?}", accounts),
        }

        let ad_domain = params
            .get(&Key::String("ad_domain".to_string()))
            .expect("ad_domain key should exist");
        match ad_domain {
            Value::Hash(h) => {
                assert_eq!(
                    h.get(&Key::String("name".to_string())),
                    Some(&Value::String("dc1.example.com".to_string())),
                    "ad_domain.name should be dc1.example.com"
                );
            }
            _ => panic!("ad_domain should be a Hash, got {:?}", ad_domain),
        }
    }

    // Test: node parameters with list values should not be duplicated
    // when the same key exists in both the accumulator and the node.
    // This was a bug where merge_accumulator_into_node merged node
    // parameters a second time, causing list values to be appended
    // with themselves.
    #[test]
    fn test_node_list_values_not_duplicated() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   pve_vm:
                       vlan_tag: "11"
               ---
               name: test_node
               type: node
               classes:
                   - base_class
               parameters:
                   pve_vm:
                       description:
                           - "My Server"
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let pve_vm = params
            .get(&Key::String("pve_vm".to_string()))
            .expect("pve_vm should exist");
        match pve_vm {
            Value::Hash(h) => {
                let desc = h
                    .get(&Key::String("description".to_string()))
                    .expect("description should exist");
                match desc {
                    Value::Array(arr) => {
                        assert_eq!(
                            arr.len(),
                            1,
                            "description should have exactly 1 element, not duplicated, got {}: {:?}",
                            arr.len(),
                            arr
                        );
                        assert_eq!(
                            arr[0],
                            Value::String("My Server".to_string()),
                            "description should be 'My Server', got {:?}",
                            arr
                        );
                    }
                    _ => panic!("description should be an Array, got {:?}", desc),
                }
            }
            _ => panic!("pve_vm should be a Hash, got {:?}", pve_vm),
        }
    }

    // Test: =key prefix marks a parameter as constant.
    // Once constant, later classes cannot change the value.
    // class base: port: 80
    // class constant_class (inherits base): =port: 443
    // class later_class: port: 9090
    // Expected: port: 443 (constant blocks later_class)
    #[test]
    fn test_constant_parameter_blocks_later_class() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   port: 80
               ---
               name: constant_class
               classes:
                   - base
               parameters:
                   "=port": 443
               ---
               name: later_class
               parameters:
                   port: 9090
               ---
               name: test_node
               type: node
               classes:
                   - constant_class
                   - later_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: true,
            strict_constant_parameters: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let port = params
            .get(&Key::String("port".to_string()))
            .expect("port should exist");
        assert_eq!(
            port,
            &Value::Integer(443),
            "port should be 443 (constant blocks later), got {:?}",
            port
        );
    }

    // Test: =key in strict mode raises error when later class tries to change it
    #[test]
    fn test_constant_parameter_strict_mode_error() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   port: 80
               ---
               name: constant_class
               classes:
                   - base
               parameters:
                   "=port": 443
               ---
               name: later_class
               parameters:
                   port: 9090
               ---
               name: test_node
               type: node
               classes:
                   - constant_class
                   - later_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: true,
            strict_constant_parameters: true,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let result = inventory.merge_node("test_node");
        assert!(
            result.is_err(),
            "should fail with ChangedConstantParameter error in strict mode"
        );
        let err = result.unwrap_err();
        let mut chain = err.to_string();
        let mut source = std::error::Error::source(&err);
        while let Some(e) = source {
            chain.push_str(&format!("\n  caused by: {}", e));
            source = std::error::Error::source(e);
        }
        assert!(
            chain.to_lowercase().contains("constant"),
            "error should mention constant, got: {}",
            chain
        );
    }

    // Test: =key on a dict value (replaces entire dict and marks constant)
    #[test]
    fn test_constant_dict_parameter() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: constant_class
               classes:
                   - base
               parameters:
                   "=accounts":
                       local: true
               ---
               name: test_node
               type: node
               classes:
                   - constant_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_constant_prefix: Some("=".to_string()),
            feature_value_constant: true,
            strict_constant_parameters: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts should exist");
        match accounts {
            Value::Hash(h) => {
                assert_eq!(h.len(), 1, "accounts should have 1 key");
                assert!(
                    h.contains_key(&Key::String("local".to_string())),
                    "accounts should contain 'local'"
                );
                assert!(
                    !h.contains_key(&Key::String("ssh".to_string())),
                    "accounts should NOT contain 'ssh' (constant replaced)"
                );
            }
            _ => panic!("accounts should be a Hash, got {:?}", accounts),
        }
    }

    // Test: class name interpolation — ${environment}.prod resolves to staging.prod
    #[test]
    fn test_class_name_interpolation() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: staging.prod
               parameters:
                   role: production
               ---
               name: env_setup
               parameters:
                   environment: staging
               ---
               name: test_node
               type: node
               classes:
                   - env_setup
                   - "${environment}.prod"
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");
        inventory.set_merge_config(MergeConfig::default());

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let role = params
            .get(&Key::String("role".to_string()))
            .expect("role should exist from interpolated class");
        assert_eq!(
            role,
            &Value::String("production".to_string()),
            "role should be 'production' from staging.prod class"
        );
    }

    // Test: class name interpolation with string containing reference
    // debian.${role} resolves to debian.web
    #[test]
    fn test_class_name_interpolation_string_with_ref() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: debian.web
               parameters:
                   webserver: nginx
               ---
               name: role_setup
               parameters:
                   role: web
               ---
               name: test_node
               type: node
               classes:
                   - role_setup
                   - "debian.${role}"
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");
        inventory.set_merge_config(MergeConfig::default());

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let webserver = params
            .get(&Key::String("webserver".to_string()))
            .expect("webserver should exist from debian.web class");
        assert_eq!(
            webserver,
            &Value::String("nginx".to_string()),
            "webserver should be nginx from debian.web class"
        );
    }

    // Test: class name interpolation with non-string parameter (integer coercion)
    // ${num} where num is 42 resolves to "42"
    #[test]
    fn test_class_name_interpolation_non_string_coercion() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: zone.42
               parameters:
                   zone_type: numbered
               ---
               name: num_setup
               parameters:
                   num: 42
               ---
               name: test_node
               type: node
               classes:
                   - num_setup
                   - "zone.${num}"
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");
        inventory.set_merge_config(MergeConfig::default());

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let zone_type = params
            .get(&Key::String("zone_type".to_string()))
            .expect("zone_type should exist from zone.42 class");
        assert_eq!(
            zone_type,
            &Value::String("numbered".to_string()),
            "zone_type should be 'numbered' from zone.42 class"
        );
    }

    // Test: class name interpolation with unresolvable reference fails
    #[test]
    fn test_class_name_interpolation_unresolvable() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: test_node
               type: node
               classes:
                   - "${missing_param}.prod"
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");
        inventory.set_merge_config(MergeConfig::default());

        let result = inventory.merge_node("test_node");
        assert!(
            result.is_err(),
            "should fail when class name reference cannot be resolved"
        );
    }

    // Test: class name interpolation uses parameters from previously-processed classes
    // (feedback loop: resolved class feeds back into accumulator)
    #[test]
    fn test_class_name_interpolation_feedback_loop() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: region.europe
               parameters:
                   tier: frontend
               ---
               name: tier_setup
               parameters:
                   region: europe
               ---
               name: test_node
               type: node
               classes:
                   - tier_setup
                   - "region.${region}"
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");
        inventory.set_merge_config(MergeConfig::default());

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let tier = params
            .get(&Key::String("tier".to_string()))
            .expect("tier should exist from region.europe class");
        assert_eq!(
            tier,
            &Value::String("frontend".to_string()),
            "tier should be 'frontend' from region.europe class"
        );

        let region = params
            .get(&Key::String("region".to_string()))
            .expect("region should exist from tier_setup class");
        assert_eq!(
            region,
            &Value::String("europe".to_string()),
            "region should be 'europe' from tier_setup class"
        );
    }

    #[test]
    fn test_type_merge_error_null_over_dict() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   accounts: null
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let result = inventory.merge_node("test_node");
        assert!(
            result.is_err(),
            "Expected TypeMerge error for null over dict"
        );
    }

    #[test]
    fn test_type_merge_error_null_over_list() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   ports:
                       - 80
                       - 443
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   ports: null
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let result = inventory.merge_node("test_node");
        assert!(
            result.is_err(),
            "Expected TypeMerge error for null over list"
        );
    }

    #[test]
    fn test_type_merge_null_over_dict_with_allow_none() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   accounts: null
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            allow_none_override: true,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        assert_eq!(
            accounts,
            &Value::Null,
            "accounts should be Null with allow_none_override=true, got {:?}",
            accounts
        );
    }

    #[test]
    fn test_type_merge_null_over_list_with_allow_none() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   ports:
                       - 80
                       - 443
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   ports: null
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            allow_none_override: true,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let ports = params
            .get(&Key::String("ports".to_string()))
            .expect("ports key should exist");
        assert_eq!(
            ports,
            &Value::Null,
            "ports should be Null with allow_none_override=true, got {:?}",
            ports
        );
    }

    #[test]
    fn test_type_merge_tilde_null_over_dict_bypasses_type_check() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   "~accounts": null
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            value_override_prefix: Some("~".to_string()),
            feature_value_override: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("should succeed — tilde override bypasses type check");
        let params = node.parameters();

        let accounts = params
            .get(&Key::String("accounts".to_string()))
            .expect("accounts key should exist");
        assert_eq!(
            accounts,
            &Value::Null,
            "accounts should be Null after ~accounts: null override, got {:?}",
            accounts
        );
    }

    #[test]
    fn test_type_merge_error_dict_over_list() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   ports:
                       - 80
                       - 443
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   ports:
                       ssh: true
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let result = inventory.merge_node("test_node");
        assert!(
            result.is_err(),
            "Expected TypeMerge error for dict over list"
        );
    }

    #[test]
    fn test_type_merge_error_list_over_dict() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   accounts:
                       ssh: true
                       ldap: true
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   accounts:
                       - 80
                       - 443
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        inventory.set_merge_config(config);

        let result = inventory.merge_node("test_node");
        assert!(
            result.is_err(),
            "Expected TypeMerge error for list over dict"
        );
    }

    #[test]
    fn test_exports_merge_across_classes() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   hostname: server1
               exports:
                   role: webserver
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   hostname: server2
               exports:
                   role: dbserver
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");

        assert_eq!(
            node.exports().get(&Key::String("role".to_string())),
            Some(&Value::String("dbserver".to_string())),
            "exports should deep-merge, with later class overriding"
        );
    }

    #[test]
    fn test_exports_interpolation_against_parameters() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   hostname: myserver
                   port: 8080
               exports:
                   endpoint: "${hostname}:${port}"
               ---
               name: test_node
               type: node
               classes:
                   - base_class
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");

        assert_eq!(
            node.exports().get(&Key::String("endpoint".to_string())),
            Some(&Value::String("myserver:8080".to_string())),
            "exports should interpolate references against node parameters"
        );
    }

    #[test]
    fn test_type_merge_error_includes_key_path() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base_class
               parameters:
                   config:
                       server:
                           host: localhost
               ---
               name: override_class
               classes:
                   - base_class
               parameters:
                   config:
                       server: 8080
               ---
               name: test_node
               type: node
               classes:
                   - override_class
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let mut inventory = inventory;
        inventory.set_merge_config(config);

        let result = inventory.merge_node("test_node");
        assert!(result.is_err());
        let err = result.unwrap_err();
        let mut err_chain = err.to_string();
        let mut source = std::error::Error::source(&err);
        while let Some(e) = source {
            err_chain.push_str(&format!("\n  caused by: {}", e));
            source = std::error::Error::source(e);
        }
        assert!(
            err_chain.contains("config:server"),
            "error should contain key path 'config:server', got: {}",
            err_chain
        );
    }

    #[test]
    fn test_ignore_class_notfound_default_errors() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   key: value
               ---
               name: test_node
               type: node
               classes:
                   - base
                   - nonexistent
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig::default();
        let mut inventory = inventory;
        inventory.set_merge_config(config);

        let result = inventory.merge_node("test_node");
        assert!(result.is_err(), "should error with default config");
    }

    #[test]
    fn test_ignore_class_notfound_skips_with_default_regexp() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   key: value
               ---
               name: test_node
               type: node
               classes:
                   - base
                   - nonexistent
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            ignore_class_notfound: true,
            ignore_class_notfound_warning: false,
            ..MergeConfig::default()
        };
        let mut inventory = inventory;
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("should succeed with ignore_class_notfound");

        let params = node.parameters();
        assert_eq!(
            params.get(&Key::String("key".to_string())),
            Some(&Value::String("value".to_string())),
            "should still have base class params"
        );
    }

    #[test]
    fn test_ignore_class_notfound_with_specific_regexp() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   key: value
               ---
               name: test_node
               type: node
               classes:
                   - base
                   - missing_class
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            ignore_class_notfound: true,
            ignore_class_notfound_regexp: vec!["miss.*".to_string()],
            ignore_class_notfound_warning: false,
            ..MergeConfig::default()
        };
        let mut inventory = inventory;
        inventory.set_merge_config(config);

        let node = inventory
            .merge_node("test_node")
            .expect("should succeed — regexp matches 'missing_class'");

        let params = node.parameters();
        assert_eq!(
            params.get(&Key::String("key".to_string())),
            Some(&Value::String("value".to_string())),
        );
    }

    #[test]
    fn test_ignore_class_notfound_regexp_no_match_still_errors() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   key: value
               ---
               name: test_node
               type: node
               classes:
                   - base
                   - nonexistent
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let config = MergeConfig {
            ignore_class_notfound: true,
            ignore_class_notfound_regexp: vec!["miss.*".to_string()],
            ignore_class_notfound_warning: false,
            ..MergeConfig::default()
        };
        let mut inventory = inventory;
        inventory.set_merge_config(config);

        let result = inventory.merge_node("test_node");
        assert!(
            result.is_err(),
            "should still error — 'nonexistent' does not match 'miss.*'"
        );
    }

    #[test]
    fn test_duplicate_node_name_error() {
        let node1 = Node::new("server".to_string())
            .uri("yaml_fs:///nodes/alpha/server.yml")
            .build();
        let node2 = Node::new("server".to_string())
            .uri("yaml_fs:///nodes/beta/server.yml")
            .build();

        let mut inventory = Inventory::new();
        inventory.add_node(node1).unwrap();
        let result = inventory.add_node(node2);
        assert!(result.is_err(), "should error on duplicate node name");
        match result.unwrap_err() {
            Error::DuplicateNodeName {
                name,
                existing_uri,
                new_uri,
            } => {
                assert_eq!(name, "server");
                assert_eq!(existing_uri, "yaml_fs:///nodes/alpha/server.yml");
                assert_eq!(new_uri, "yaml_fs:///nodes/beta/server.yml");
            }
            e => panic!("Expected DuplicateNodeName, got {:?}", e),
        }
    }

    #[test]
    fn test_no_duplicate_with_different_names() {
        let node1 = Node::new("alpha.server".to_string()).build();
        let node2 = Node::new("beta.server".to_string()).build();

        let mut inventory = Inventory::new();
        inventory.add_node(node1).unwrap();
        let result = inventory.add_node(node2);
        assert!(result.is_ok(), "different names should not collide");
    }

    // --- group_errors integration tests ---

    #[test]
    fn test_group_errors_single_resolve_error() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: node1
               type: node
               parameters:
                   alpha: ${missing_ref}
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let config = MergeConfig {
            group_errors: true,
            ..MergeConfig::default()
        };
        let mut inv_with_config = inventory;
        inv_with_config.set_merge_config(config);

        let result = inv_with_config.merge_node("node1");
        assert!(result.is_err(), "should error on unresolvable reference");
        match result.unwrap_err() {
            Error::Interpolation { source } => match source {
                interpolation::Error::ReferenceNotFound { path } => {
                    assert_eq!(path, "missing_ref");
                }
                other => panic!(
                    "Expected ReferenceNotFound for single error, got {:?}",
                    other
                ),
            },
            e => panic!("Expected Interpolation, got {:?}", e),
        }
    }

    #[test]
    fn test_group_errors_multiple_resolve_errors_grouped() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: node1
               type: node
               parameters:
                   alpha: ${ref_a}
                   beta: ${ref_b}
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let config = MergeConfig {
            group_errors: true,
            ..MergeConfig::default()
        };
        let mut inv_with_config = inventory;
        inv_with_config.set_merge_config(config);

        let result = inv_with_config.merge_node("node1");
        assert!(result.is_err(), "should error on unresolvable references");
        match result.unwrap_err() {
            Error::Interpolation { source } => match source {
                interpolation::Error::ResolveErrorList { errors } => {
                    assert!(
                        errors.len() >= 2,
                        "should have at least 2 errors, got {}",
                        errors.len()
                    );
                }
                other => panic!(
                    "Expected ResolveErrorList for multiple errors, got {:?}",
                    other
                ),
            },
            e => panic!("Expected Interpolation, got {:?}", e),
        }
    }

    #[test]
    fn test_group_errors_single_error_mode() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: node1
               type: node
               parameters:
                   alpha: ${ref_a}
                   beta: ${ref_b}
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        let config = MergeConfig {
            group_errors: false,
            ..MergeConfig::default()
        };
        let mut inv_with_config = inventory;
        inv_with_config.set_merge_config(config);

        let result = inv_with_config.merge_node("node1");
        assert!(result.is_err(), "should error on unresolvable reference");
        match result.unwrap_err() {
            Error::Interpolation { source } => match source {
                interpolation::Error::ReferenceNotFound { path } => {
                    assert!(
                        path == "ref_a" || path == "ref_b",
                        "should be one of the two missing refs, got {}",
                        path
                    );
                }
                other => panic!(
                    "Expected ReferenceNotFound for single-error mode, got {:?}",
                    other
                ),
            },
            e => panic!("Expected Interpolation, got {:?}", e),
        }
    }

    #[test]
    fn test_group_errors_non_resolve_error_stops_immediately() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   alpha: ${ref_a}
               ---
               name: node1
               type: node
               classes:
                   - base
               parameters:
                   beta: ${ref_b}
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::Ansible)
            .expect("failed to parse inventory");

        // With group_errors=true, circular references and type merge errors still stop immediately
        // This test verifies that a TypeMerge error is NOT collected into ResolveErrorList
        let config = MergeConfig {
            group_errors: true,
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let mut inv_with_config = inventory;
        inv_with_config.set_merge_config(config);

        let result = inv_with_config.merge_node("node1");
        assert!(result.is_err());
        // The error should be ReferenceNotFound (ResolveErrorList with collected errors,
        // since both alpha and beta reference missing keys)
        match result.unwrap_err() {
            Error::Interpolation { source } => match source {
                interpolation::Error::ResolveErrorList { errors } => {
                    assert!(
                        errors.len() >= 2,
                        "should have collected both resolve errors"
                    );
                }
                interpolation::Error::ReferenceNotFound { .. } => {
                    // Single error is also acceptable (if both resolve to same key)
                }
                other => panic!(
                    "Expected ResolveErrorList or ReferenceNotFound, got {:?}",
                    other
                ),
            },
            e => panic!("Expected Interpolation, got {:?}", e),
        }
    }

    #[test]
    fn test_inv_query_value_simple() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: web_node
               type: node
               exports:
                   ip: 10.0.0.1
               ---
               name: db_node
               type: node
               exports:
                   ip: 10.0.0.2
               ---
               name: test_node
               type: node
               parameters:
                   peers: "$[exports:ip]"
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let inv_map = inventory
            .build_inventory_map()
            .expect("failed to build inventory map");

        let node = inventory
            .merge_node_with_inventory("test_node", &inv_map)
            .expect("failed to merge node with inventory");

        let peers = node
            .parameters()
            .get(&Key::String("peers".to_string()))
            .expect("peers should exist");

        match peers {
            Value::Hash(h) => {
                assert_eq!(
                    h.len(),
                    2,
                    "should have 2 nodes with ip exports, got: {:?}",
                    h
                );
                assert_eq!(
                    h.get(&Key::String("web_node".to_string())),
                    Some(&Value::String("10.0.0.1".to_string())),
                    "web_node ip should be in results"
                );
                assert_eq!(
                    h.get(&Key::String("db_node".to_string())),
                    Some(&Value::String("10.0.0.2".to_string())),
                    "db_node ip should be in results"
                );
            }
            _ => panic!("Expected Hash for VALUE query result, got {:?}", peers),
        }
    }

    #[test]
    fn test_inv_query_list_test() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: web1
               type: node
               exports:
                   cluster: web
               ---
               name: db1
               type: node
               exports:
                   cluster: db
               ---
               name: web2
               type: node
               exports:
                   cluster: web
               ---
               name: requester
               type: node
               parameters:
                   my_cluster: web
                   web_nodes: "$[if exports:cluster == self:my_cluster]"
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let inv_map = inventory
            .build_inventory_map()
            .expect("failed to build inventory map");

        let node = inventory
            .merge_node_with_inventory("requester", &inv_map)
            .expect("failed to merge node");

        let web_nodes = node
            .parameters()
            .get(&Key::String("web_nodes".to_string()))
            .expect("web_nodes should exist");

        match web_nodes {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 2, "should have 2 web nodes");
                let names: Vec<&str> = arr
                    .iter()
                    .filter_map(|v| match v {
                        Value::String(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect();
                assert!(names.contains(&"web1"), "should contain web1");
                assert!(names.contains(&"web2"), "should contain web2");
            }
            _ => panic!("Expected Array for LIST_TEST result, got {:?}", web_nodes),
        }
    }

    #[test]
    fn test_inv_query_test_type() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: web1
               type: node
               exports:
                   cluster: web
                   ip: 10.0.0.1
               ---
               name: db1
               type: node
               exports:
                   cluster: db
                   ip: 10.0.0.2
               ---
               name: web2
               type: node
               exports:
                   cluster: web
                   ip: 10.0.0.3
               ---
               name: requester
               type: node
               parameters:
                   my_cluster: web
                   web_ips: "$[exports:ip if exports:cluster == self:my_cluster]"
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let inv_map = inventory
            .build_inventory_map()
            .expect("failed to build inventory map");

        let node = inventory
            .merge_node_with_inventory("requester", &inv_map)
            .expect("failed to merge node");

        let web_ips = node
            .parameters()
            .get(&Key::String("web_ips".to_string()))
            .expect("web_ips should exist");

        match web_ips {
            Value::Hash(h) => {
                assert_eq!(
                    h.len(),
                    2,
                    "should have 2 web nodes with ip exports, got {:?}",
                    h
                );
            }
            _ => panic!("Expected Hash for TEST query result, got {:?}", web_ips),
        }
    }

    #[test]
    fn test_inv_query_without_inventory_stays_unresolved() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: web1
               exports:
                   ip: 10.0.0.1
               ---
               name: requester
               type: node
               parameters:
                   peers: "$[exports:ip]"
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let node = inventory
            .merge_node("requester")
            .expect("failed to merge node");

        let peers = node
            .parameters()
            .get(&Key::String("peers".to_string()))
            .expect("peers should exist");

        assert!(
            matches!(peers, Value::InvQuery(_)),
            "without inventory, InvQuery should remain unresolved, got {:?}",
            peers
        );
    }

    #[test]
    fn test_inv_query_has_inv_query_detection() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: web1
               exports:
                   ip: 10.0.0.1
               ---
               name: requester
               type: node
               parameters:
                   peers: "$[exports:ip]"
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let node = inventory
            .merge_node("requester")
            .expect("failed to merge node");

        let peers = node
            .parameters()
            .get(&Key::String("peers".to_string()))
            .expect("peers should exist");

        assert!(peers.has_inv_query(), "peers should have inv query");
    }

    // --- input_data tests ---
    // input_data provides low-priority defaults that classes and node params can override.
    // Merge order: class_mappings → input_data → automatic_params → classes → node

    #[test]
    fn test_input_data_provides_defaults() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   role: webserver
               ---
               name: test_node
               type: node
               classes:
                   - base
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let mut input_data = ParametersType::new();
        input_data.insert(
            Key::String("default_host".to_string()),
            Value::String("localhost".to_string()),
        );
        input_data.insert(
            Key::String("role".to_string()),
            Value::String("unknown".to_string()),
        );
        inventory.set_input_data(input_data);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        assert_eq!(
            params.get(&Key::String("default_host".to_string())),
            Some(&Value::String("localhost".to_string())),
            "input_data key should be present when not overridden"
        );
        assert_eq!(
            params.get(&Key::String("role".to_string())),
            Some(&Value::String("webserver".to_string())),
            "class param should override input_data"
        );
    }

    #[test]
    fn test_input_data_overridden_by_node_params() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: test_node
               type: node
               parameters:
                   host: myhost
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let mut input_data = ParametersType::new();
        input_data.insert(
            Key::String("host".to_string()),
            Value::String("input_host".to_string()),
        );
        inventory.set_input_data(input_data);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        assert_eq!(
            params.get(&Key::String("host".to_string())),
            Some(&Value::String("myhost".to_string())),
            "node param should override input_data"
        );
    }

    #[test]
    fn test_input_data_deep_merge_with_classes() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   config:
                       host: prod-server
                       port: 443
               ---
               name: test_node
               type: node
               classes:
                   - base
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let mut input_data = ParametersType::new();
        let mut config = ParametersType::new();
        config.insert(
            Key::String("host".to_string()),
            Value::String("input-host".to_string()),
        );
        config.insert(Key::String("debug".to_string()), Value::Boolean(true));
        input_data.insert(
            Key::String("config".to_string()),
            Value::Hash(Rc::new(config)),
        );
        inventory.set_input_data(input_data);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        let config_val = params
            .get(&Key::String("config".to_string()))
            .expect("config should exist");
        match config_val {
            Value::Hash(h) => {
                assert_eq!(
                    h.get(&Key::String("host".to_string())),
                    Some(&Value::String("prod-server".to_string())),
                    "class config.host should override input_data config.host"
                );
                assert_eq!(
                    h.get(&Key::String("port".to_string())),
                    Some(&Value::Integer(443)),
                    "class config.port should be present"
                );
                assert_eq!(
                    h.get(&Key::String("debug".to_string())),
                    Some(&Value::Boolean(true)),
                    "input_data config.debug should be present when not overridden"
                );
            }
            _ => panic!("config should be a Hash, got {:?}", config_val),
        }
    }

    #[test]
    fn test_input_data_none_means_no_merge() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   key: from_class
               ---
               name: test_node
               type: node
               classes:
                   - base
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        assert_eq!(
            params.get(&Key::String("key".to_string())),
            Some(&Value::String("from_class".to_string())),
            "without input_data, class param should be present"
        );
    }

    #[test]
    fn test_input_data_overrides_class_mappings_params() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: mapped_class
               parameters:
                   region: eu-west
               ---
               name: node1
               type: node
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let mapping =
            crate::inventory::class_mapping::ClassMapping::parse("node* mapped_class").unwrap();
        inventory.set_class_mappings(vec![mapping]);

        let mut input_data = ParametersType::new();
        input_data.insert(
            Key::String("region".to_string()),
            Value::String("us-east".to_string()),
        );
        inventory.set_input_data(input_data);

        let node = inventory.merge_node("node1").expect("failed to merge node");
        let params = node.parameters();

        assert_eq!(
            params.get(&Key::String("region".to_string())),
            Some(&Value::String("us-east".to_string())),
            "input_data should override class_mappings params (input_data merges on top of base)"
        );
    }

    #[test]
    fn test_input_data_overridden_by_node_classes() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: server_class
               parameters:
                   role: webserver
                   tier: frontend
               ---
               name: test_node
               type: node
               classes:
                   - server_class
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let mut input_data = ParametersType::new();
        input_data.insert(
            Key::String("role".to_string()),
            Value::String("unknown".to_string()),
        );
        input_data.insert(
            Key::String("default_zone".to_string()),
            Value::String("a".to_string()),
        );
        inventory.set_input_data(input_data);

        let node = inventory
            .merge_node("test_node")
            .expect("failed to merge node");
        let params = node.parameters();

        assert_eq!(
            params.get(&Key::String("role".to_string())),
            Some(&Value::String("webserver".to_string())),
            "node class should override input_data"
        );
        assert_eq!(
            params.get(&Key::String("tier".to_string())),
            Some(&Value::String("frontend".to_string())),
            "node class param should be present"
        );
        assert_eq!(
            params.get(&Key::String("default_zone".to_string())),
            Some(&Value::String("a".to_string())),
            "input_data param not overridden by classes should be present"
        );
    }
}
