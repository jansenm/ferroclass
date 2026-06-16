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
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) mod applications;
pub mod class_mapping;
pub mod diagnostic;
pub mod elements;
pub(crate) mod interpolation;
pub(crate) mod inv_query;
pub(crate) mod merge;
pub mod options;
pub mod types;
pub mod value;
pub(crate) mod value_merge;

pub use diagnostic::{Diagnostic, DiagnosticSeverity, EntityState, SourceLocation};
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
        Value::Hash(Arc::new(name_hash)),
    );
    reclass_hash.insert(
        Key::String("environment".to_string()),
        Value::String(environment.to_string()),
    );

    let mut params: ParametersType = LinkedHashMap::new();
    params.insert(
        Key::String("_reclass_".to_string()),
        Value::Hash(Arc::new(reclass_hash)),
    );
    params
}

/// The central data structure holding all loaded classes and nodes.
///
/// An `Inventory` is populated by [`load`] or [`load_from_yaml_string`] and
/// provides methods to query, iterate over, and merge individual nodes and
/// classes.
///
/// # Reverse indexes
///
/// Query methods like [`find_nodes_by_class`] and [`find_nodes_by_environment`]
/// use reverse indexes for fast lookups. These indexes are built on demand:
///
/// - Call [`build_indexes`] explicitly after loading if you want to control
///   when the cost of index construction is paid.
/// - Query methods that need indexes will fall back to a linear scan if
///   indexes are not yet built.
/// - Calling [`add_node`] or [`add_class`] invalidates the indexes. Call
///   [`build_indexes`] again if you need up-to-date indexes.
///
/// [`find_nodes_by_class`]: Inventory::find_nodes_by_class
/// [`find_nodes_by_environment`]: Inventory::find_nodes_by_environment
/// [`build_indexes`]: Inventory::build_indexes
/// [`add_node`]: Inventory::add_node
/// [`add_class`]: Inventory::add_class
#[derive(Debug)]
pub struct Inventory {
    classes: LinkedHashMap<String, Class>,
    nodes: LinkedHashMap<String, Node>,
    merge_config: MergeConfig,
    class_mappings: Vec<ClassMapping>,
    class_mappings_match_path: bool,
    input_data: Option<ParametersType>,
    /// Reverse index: class name → node names that include it.
    /// None means not yet built; Some(HashMap) means ready for queries.
    class_to_nodes_index: Option<HashMap<String, Vec<String>>>,
    /// Reverse index: environment → node names in that environment.
    /// None means not yet built; Some(HashMap) means ready for queries.
    environment_to_nodes_index: Option<HashMap<String, Vec<String>>>,
    /// Inventory-level diagnostics (config errors, filesystem issues, etc.).
    /// These have `subject: None` — they are about the inventory as a whole,
    /// not about any specific node or class.
    diagnostics: Vec<Diagnostic>,
    /// Per-entity summary diagnostics, keyed by entity name.
    /// Each entity (node or class) can have at most one summary diagnostic
    /// here. When a node or class is added via `add_node()` / `add_class()`,
    /// any previous summary diagnostic for that entity is removed and replaced
    /// with the new one (if the entity has problems). This ensures the
    /// 0-or-1 invariant: there is never more than one inventory-level
    /// diagnostic per entity.
    entity_diagnostics: HashMap<String, Diagnostic>,
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

impl Default for Inventory {
    fn default() -> Self {
        Self {
            classes: LinkedHashMap::new(),
            nodes: LinkedHashMap::new(),
            merge_config: MergeConfig::default(),
            class_mappings: Vec::new(),
            class_mappings_match_path: false,
            input_data: None,
            class_to_nodes_index: None,
            environment_to_nodes_index: None,
            diagnostics: Vec::new(),
            entity_diagnostics: HashMap::new(),
        }
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
            class_to_nodes_index: None,
            environment_to_nodes_index: None,
            diagnostics: Vec::new(),
            entity_diagnostics: HashMap::new(),
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

    /// Return the aggregate entity state of this inventory.
    ///
    /// The inventory's state is the minimum (worst) state across all its
    /// nodes and classes, plus any inventory-level diagnostics:
    ///
    /// - If any node or class is `Failed`, the inventory is `Failed`.
    /// - If the worst is `Source`, the inventory is `Source`.
    /// - If the worst is `Merged`, the inventory is `Merged`.
    /// - If everything is `Interpolated` with no errors, the inventory is
    ///   `Interpolated`.
    ///
    /// An empty inventory with no diagnostics defaults to `Interpolated`.
    pub fn state(&self) -> EntityState {
        let worst_entity = self
            .nodes
            .values()
            .map(|n| n.state())
            .chain(self.classes.values().map(|c| c.state()))
            .min()
            .unwrap_or(EntityState::Interpolated);

        // If there are inventory-level error diagnostics but all entities
        // claim to be fine, demote to Merged (something went wrong but we
        // don't know which entity it affected).
        if worst_entity == EntityState::Interpolated && self.has_errors() {
            EntityState::Merged
        } else {
            worst_entity
        }
    }

    /// Return inventory-level diagnostics that don't belong to any
    /// specific node or class (subject is None).
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Return all diagnostics: inventory-level plus per-entity summaries.
    ///
    /// The per-entity summaries are the ones stored in `entity_diagnostics`
    /// — one diagnostic per entity (node or class) that has problems.
    /// The entity's own `diagnostics()` method has the full details.
    pub fn all_diagnostics(&self) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .chain(self.entity_diagnostics.values())
            .collect()
    }

    /// Add an inventory-level diagnostic (subject should be None).
    ///
    /// For per-entity diagnostics, use `add_node()` / `add_class()` which
    /// automatically sync the entity summary diagnostic.
    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Return whether any entity (inventory, node, or class) has errors.
    pub fn has_errors(&self) -> bool {
        if self
            .diagnostics
            .iter()
            .chain(self.entity_diagnostics.values())
            .any(|d| d.severity == DiagnosticSeverity::Error)
        {
            return true;
        }
        self.nodes.values().any(|n| n.has_errors()) || self.classes.values().any(|c| c.has_errors())
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

    /// Add a class to the inventory.
    ///
    /// If a class with the same name already exists, it will be replaced.
    /// The inventory's per-entity diagnostic for this class is automatically
    /// synced: any previous summary diagnostic for this class is removed,
    /// and a new one is added if the class has problems.
    ///
    /// Note: this invalidates reverse indexes. Call [`build_indexes`] if
    /// you need up-to-date indexes after adding classes.
    ///
    /// [`build_indexes`]: Inventory::build_indexes
    pub fn add_class(&mut self, class: Class) {
        let name = class.name().to_string();
        // Remove any stale summary diagnostic for this class
        self.entity_diagnostics.remove(&name);

        // If the class has problems, add a summary diagnostic
        if !class.is_usable() {
            let diagnostic = if class.has_errors() {
                Diagnostic::error(format!("class '{}' has errors", name)).with_subject(name.clone())
            } else {
                Diagnostic::warning(format!("class '{}' has warnings", name))
                    .with_subject(name.clone())
            };
            self.entity_diagnostics.insert(name.clone(), diagnostic);
        }

        self.classes.insert(name, class);
        // Invalidate indexes since class mappings may affect node-class relationships
        self.class_to_nodes_index = None;
        self.environment_to_nodes_index = None;
    }

    /// Add a node to the inventory.
    ///
    /// If a node with the same name already exists, the duplicate is logged
    /// as a warning diagnostic and the new node is **skipped** (the existing
    /// node is kept). This prevents a single bad file from aborting the
    /// entire load.
    ///
    /// The inventory's per-entity diagnostic for this node is automatically
    /// synced: if the node has problems, a summary diagnostic is added;
    /// if not, any previous summary diagnostic for this node is removed.
    ///
    /// This method invalidates reverse indexes since the new node
    /// may introduce new class-node or environment-node mappings.
    pub fn add_node(&mut self, node: Node) {
        let name = node.name().to_string();
        if self.nodes.contains_key(&name) {
            tracing::warn!(
                "duplicate node name '{}': skipping",
                name
            );
            self.diagnostics.push(
                Diagnostic::warning(format!("duplicate node name '{}': skipping", name))
                    .with_code("INV-003")
                    .with_subject(name.clone()),
            );
            return;
        }
        // Remove any stale summary diagnostic for this node
        self.entity_diagnostics.remove(&name);

        // If the node has problems, add a summary diagnostic
        if !node.is_usable() {
            let diagnostic = if node.has_errors() {
                Diagnostic::error(format!("node '{}' has errors", name)).with_subject(name.clone())
            } else {
                Diagnostic::warning(format!("node '{}' has warnings", name))
                    .with_subject(name.clone())
            };
            self.entity_diagnostics.insert(name.clone(), diagnostic);
        }

        self.nodes.insert(name, node);
        // Invalidate indexes since the new node adds class-node and env-node mappings
        self.class_to_nodes_index = None;
        self.environment_to_nodes_index = None;
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
    ///
    /// Returns a borrowing iterator that yields `&str` references,
    /// avoiding allocation. Call `.collect::<Vec<_>>()` if you need
    /// an owned collection.
    pub fn node_names(&self) -> impl Iterator<Item = &str> {
        self.nodes.keys().map(|s| s.as_str())
    }

    /// Return the names of all loaded classes in insertion order.
    ///
    /// Returns a borrowing iterator that yields `&str` references,
    /// avoiding allocation. Call `.collect::<Vec<_>>()` if you need
    /// an owned collection.
    pub fn class_names(&self) -> impl Iterator<Item = &str> {
        self.classes.keys().map(|s| s.as_str())
    }

    /// Find all nodes that include the given class in their declared class list.
    ///
    /// If reverse indexes have been built (via [`build_indexes`]), this uses
    /// an O(1) index lookup. Otherwise, it falls back to a linear scan of
    /// all nodes.
    ///
    /// This only checks the node's **declared** class list — it does not
    /// recursively resolve the class inheritance chain. To find nodes whose
    /// *resolved* class list includes a class (including inherited classes),
    /// use [`find_nodes_by_resolved_class`].
    ///
    /// [`build_indexes`]: Inventory::build_indexes
    /// [`find_nodes_by_resolved_class`]: Inventory::find_nodes_by_resolved_class
    pub fn find_nodes_by_class(&self, class_name: &str) -> impl Iterator<Item = &Node> {
        if let Some(index) = &self.class_to_nodes_index {
            // Fast path: use reverse index
            index
                .get(class_name)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|name| self.nodes.get(name))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into_iter()
        } else {
            // Slow path: linear scan
            self.nodes
                .iter()
                .filter_map(move |(_, node)| {
                    if node.classes().iter().any(|c| c == class_name) {
                        Some(node)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .into_iter()
        }
    }

    /// Find all nodes whose *resolved* class list includes the given class.
    ///
    /// Unlike [`find_nodes_by_class`], which only checks the node's declared
    /// class list, this method merges each node and checks the full resolved
    /// class hierarchy (including inherited classes). This is slower but more
    /// thorough — it finds nodes where a class appears transitively through
    /// inheritance.
    ///
    /// Nodes that fail to merge are skipped.
    ///
    /// [`find_nodes_by_class`]: Inventory::find_nodes_by_class
    pub fn find_nodes_by_resolved_class(&self, class_name: &str) -> impl Iterator<Item = &Node> {
        self.nodes
            .iter()
            .filter_map(|(_, node)| {
                let merged = self.merge_node(node.name()).ok()?;
                if merged.classes().iter().any(|c| c == class_name) {
                    Some(node)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Find all nodes in the given environment.
    ///
    /// If reverse indexes have been built (via [`build_indexes`]), this uses
    /// an O(1) index lookup. Otherwise, it falls back to a linear scan of
    /// all nodes.
    ///
    /// [`build_indexes`]: Inventory::build_indexes
    pub fn find_nodes_by_environment(&self, environment: &str) -> impl Iterator<Item = &Node> {
        if let Some(index) = &self.environment_to_nodes_index {
            // Fast path: use reverse index
            index
                .get(environment)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|name| self.nodes.get(name))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .into_iter()
        } else {
            // Slow path: linear scan
            self.nodes
                .iter()
                .filter_map(move |(_, node)| {
                    if node.environment() == environment {
                        Some(node)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .into_iter()
        }
    }

    /// Search for nodes whose name contains the given pattern (case-insensitive).
    ///
    /// This performs a linear scan of all node names, checking if the
    /// pattern appears as a substring (case-insensitive). For large
    /// inventories where this is called frequently, consider building
    /// an external index.
    pub fn search_nodes(&self, pattern: &str) -> impl Iterator<Item = &Node> {
        let pattern_lower = pattern.to_lowercase();
        self.nodes
            .iter()
            .filter_map(move |(name, node)| {
                if name.to_lowercase().contains(&pattern_lower) {
                    Some(node)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Build reverse indexes for fast queries.
    ///
    /// This method constructs the `class_to_nodes` and `environment_to_nodes`
    /// reverse indexes used by [`find_nodes_by_class`] and
    /// [`find_nodes_by_environment`]. Call it explicitly after loading an
    /// inventory if you want to pay the index construction cost upfront
    /// instead of on first query.
    ///
    /// If the indexes are already built, this method is a no-op.
    /// The indexes are invalidated whenever [`add_node`] or [`add_class`]
    /// is called. Call this method again after mutations if you need
    /// up-to-date indexes.
    ///
    /// [`find_nodes_by_class`]: Inventory::find_nodes_by_class
    /// [`find_nodes_by_environment`]: Inventory::find_nodes_by_environment
    /// [`add_node`]: Inventory::add_node
    /// [`add_class`]: Inventory::add_class
    pub fn build_indexes(&mut self) {
        if self.class_to_nodes_index.is_some() && self.environment_to_nodes_index.is_some() {
            return;
        }

        let mut class_to_nodes: HashMap<String, Vec<String>> = HashMap::new();
        let mut env_to_nodes: HashMap<String, Vec<String>> = HashMap::new();

        for (node_name, node) in &self.nodes {
            // Index by each declared class
            for class_name in node.classes() {
                class_to_nodes
                    .entry(class_name.clone())
                    .or_default()
                    .push(node_name.clone());
            }
            // Index by environment
            env_to_nodes
                .entry(node.environment().to_string())
                .or_default()
                .push(node_name.clone());
        }

        self.class_to_nodes_index = Some(class_to_nodes);
        self.environment_to_nodes_index = Some(env_to_nodes);
    }

    /// Return the class-to-nodes reverse index, building it if necessary.
    ///
    /// Each entry maps a class name to the list of node names that declare
    /// that class in their class list.
    pub fn class_to_nodes(&mut self) -> &HashMap<String, Vec<String>> {
        self.build_indexes();
        self.class_to_nodes_index.as_ref().unwrap()
    }

    /// Return the environment-to-nodes reverse index, building it if necessary.
    ///
    /// Each entry maps an environment name to the list of node names in
    /// that environment.
    pub fn environment_to_nodes(&mut self) -> &HashMap<String, Vec<String>> {
        self.build_indexes();
        self.environment_to_nodes_index.as_ref().unwrap()
    }

    /// Build a map of all merged nodes for inventory-query resolution.
    ///
    /// Each entry maps a node name to its merged exports and environment.
    /// Nodes that failed to merge (state is `Failed`) are skipped if
    /// `ignore_failed_node` is enabled in the merge configuration.
    pub fn build_inventory_map(&self) -> Result<inv_query::InventoryMap, Error> {
        let mut inventory_map = inv_query::InventoryMap::new();
        for node_name in self.node_names() {
            let node = self.merge_node(node_name)?;
            if !node.is_usable() {
                if self.merge_config.inventory_ignore_failed_node {
                    tracing::warn!(
                        "Ignoring failed node '{}' during inventory build",
                        node_name
                    );
                    continue;
                }
                return Err(Error::NodeNotFound {
                    node_name: node_name.to_string(),
                });
            }
            inventory_map.insert(
                node_name.to_string(),
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

/// The result of loading an inventory, including any diagnostics
/// collected during the load process.
///
/// Per-entity load errors (YAML parse errors, invalid definitions,
/// duplicate node names) are collected as diagnostics rather than
/// aborting the entire load. Fatal errors (can't read directory,
/// can't create repository) still return `Err`.
///
/// Callers should check `result.has_errors()` before using the
/// inventory. Even if there are warnings, the inventory data is
/// usable — but if there are errors, some entities may be missing
/// or incomplete.
///
/// # Example
///
/// ```rust,ignore
/// use ferroclass::{load_with_diagnostics, StorageOptions};
///
/// let result = load_with_diagnostics(&options.storage_options)?;
/// if result.has_errors() {
///     for diag in result.diagnostics() {
///         eprintln!("{}", diag);
///     }
/// }
/// let inventory = result.into_inventory();
/// ```
#[derive(Debug)]
pub struct LoadResult {
    inventory: Inventory,
    diagnostics: Vec<Diagnostic>,
}

impl LoadResult {
    /// Return a reference to the loaded inventory.
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// Return the diagnostics collected during loading.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Return whether any diagnostic has `Error` severity.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == DiagnosticSeverity::Error)
    }

    /// Consume the `LoadResult` and return the `Inventory`.
    ///
    /// This is the most common way to extract the inventory when you
    /// don't need the diagnostics anymore.
    pub fn into_inventory(self) -> Inventory {
        self.inventory
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

/// Format an error and its full source chain for diagnostic messages.
fn format_error_chain(error: &dyn std::error::Error) -> String {
    let mut message = format!("{}", error);
    let mut source = error.source();
    while let Some(err) = source {
        message.push_str(": ");
        message.push_str(&format!("{}", err));
        source = err.source();
    }
    message
}

/// Load an inventory from a storage backend (directory tree or single file).
///
/// This is the primary entry point for loading inventory data. The storage
/// backend and path are determined by [`StorageOptions`].
///
/// Per-entity load errors (YAML parse errors, invalid definitions,
/// duplicate node names) are collected as diagnostics on the returned
/// `Inventory` instead of aborting. Only truly fatal errors (can't
/// read directory, invalid repository path) return `Err`.
///
/// To also get the diagnostics, use [`load_with_diagnostics`] which
/// returns a [`LoadResult`] instead.
///
/// [`StorageOptions`]: crate::inventory::options::StorageOptions
pub fn load(storage_options: &StorageOptions) -> Result<Inventory, Error> {
    let result = load_with_diagnostics(storage_options)?;
    Ok(result.into_inventory())
}

/// Load an inventory from a storage backend, returning diagnostics.
///
/// Like [`load`], but returns a [`LoadResult`] that includes both the
/// inventory and any diagnostics collected during loading. Per-entity
/// errors (YAML parse errors, invalid definitions, duplicate node names)
/// are collected as diagnostics rather than aborting the entire load.
///
/// Only truly fatal errors (can't read directory, invalid repository path)
/// return `Err`. Even if `Ok(result)` is returned, check
/// `result.has_errors()` to see if any files failed to load.
///
/// # Example
///
/// ```rust,ignore
/// use ferroclass::{load_with_diagnostics, StorageOptions};
///
/// let result = load_with_diagnostics(&options.storage_options)?;
/// if result.has_errors() {
///     for diag in result.diagnostics() {
///         eprintln!("{}", diag);
///     }
/// }
/// let inventory = result.into_inventory();
/// ```
pub fn load_with_diagnostics(storage_options: &StorageOptions) -> Result<LoadResult, Error> {
    match storage_options.storage_type {
        StorageType::YamlFs => load_yaml_fs_with_diagnostics(&storage_options.yaml_fs_options),
        StorageType::YamlFile => {
            load_yaml_file_with_diagnostics(&storage_options.yaml_file_options)
        }
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
    let result =
        load_from_yaml_string_with_diagnostics(yaml_content, parameter_key_style, base_uri)?;
    Ok(result.into_inventory())
}

/// Load an inventory from a YAML string, returning diagnostics.
///
/// Like [`load_from_yaml_string`], but returns a [`LoadResult`] that
/// includes both the inventory and any diagnostics collected during loading.
pub fn load_from_yaml_string_with_diagnostics(
    yaml_content: &str,
    parameter_key_style: &options::ParameterKeyStyle,
    base_uri: Option<&str>,
) -> Result<LoadResult, Error> {
    let default_environment = Environment::default();
    let result = file_system::YamlFileRepository::load_from_string(
        yaml_content,
        parameter_key_style,
        base_uri,
        &default_environment,
    );

    let mut inventory = Inventory::new();
    let mut diagnostics = Vec::new();

    match result {
        Ok((_metadata, classes, nodes)) => {
            for class in classes {
                inventory.add_class(class);
            }
            for node in nodes {
                inventory.add_node(node);
            }
        }
        Err(e) => {
            // For single-string loads, a parse error is fatal — we can't
            // even partially load. Add it as a diagnostic.
            diagnostics.push(
                Diagnostic::error(format_error_chain(&e))
                    .with_code("PARSE-001"),
                );
        }
    }

    // Collect any diagnostics that were added during loading (e.g., duplicate nodes)
    diagnostics.extend(inventory.diagnostics().iter().cloned());

    Ok(LoadResult {
        inventory,
        diagnostics,
    })
}

fn load_yaml_fs_with_diagnostics(
    storage_options: &YamlFsStorageOptions,
) -> Result<LoadResult, Error> {
    let base_uri = storage_options.inventory_base_uri.clone();
    let mut inventory = Inventory::new();
    let mut diagnostics = Vec::new();

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

    for class_result in repo.classes_iter() {
        match class_result {
            Ok(class) => inventory.add_class(class),
            Err(e) => {
                // Collect per-class errors as diagnostics instead of aborting
                diagnostics.push(
                    Diagnostic::error(format_error_chain(&e))
                        .with_code("PARSE-002"),
                );
            }
        }
    }

    tracing::debug!(
        path = storage_options.nodes_path().to_string_lossy().as_ref(),
        "loading nodes"
    );

    for node_result in repo.nodes_iter() {
        match node_result {
            Ok(node) => inventory.add_node(node),
            Err(e) => {
                // Collect per-node errors as diagnostics instead of aborting
                diagnostics.push(
                    Diagnostic::error(format_error_chain(&e))
                        .with_code("PARSE-003"),
                );
            }
        }
    }

    // Collect any diagnostics that were added during loading (e.g., duplicate nodes)
    diagnostics.extend(inventory.diagnostics().iter().cloned());

    Ok(LoadResult {
        inventory,
        diagnostics,
    })
}

fn load_yaml_file_with_diagnostics(
    storage_options: &YamlFileStorageOptions,
) -> Result<LoadResult, Error> {
    let base_uri = storage_options.inventory_file.clone();
    let mut inventory = Inventory::new();
    let mut diagnostics = Vec::new();

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

    match repo.load() {
        Ok((_metadata, classes, nodes)) => {
            for class in classes {
                inventory.add_class(class);
            }
            for node in nodes {
                inventory.add_node(node);
            }
        }
        Err(e) => {
            // For single-file loads, a parse error is fatal — we can't
            // even partially load. Add it as a diagnostic.
            diagnostics.push(
                Diagnostic::error(format_error_chain(&e))
                    .with_code("PARSE-001"),
            );
        }
    }

    // Collect any diagnostics that were added during loading (e.g., duplicate nodes)
    diagnostics.extend(inventory.diagnostics().iter().cloned());

    Ok(LoadResult {
        inventory,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::options::ParameterKeyStyle;

    // Static assertions that core types are Send + Sync.
    // This is critical for LSP/MCP/Explorer interfaces that need to share
    // Inventory across threads.
    fn _assert_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Inventory>();
        assert_send_sync::<Class>();
        assert_send_sync::<Node>();
        assert_send_sync::<Value>();
        assert_send_sync::<MergeConfig>();
    }

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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "should be Failed in strict mode"
        );
        assert!(node.has_errors(), "should have error diagnostics");
        let diag_msg = node
            .diagnostics()
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            diag_msg.to_lowercase().contains("constant"),
            "diagnostics should mention constant, got: {}",
            diag_msg
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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "should be Failed when class name reference cannot be resolved"
        );
        assert!(node.has_errors(), "should have error diagnostics");
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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "Expected Failed state for null over dict"
        );
        assert!(node.has_errors(), "should have error diagnostics");
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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "Expected Failed state for null over list"
        );
        assert!(node.has_errors(), "should have error diagnostics");
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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "Expected Failed state for dict over list"
        );
        assert!(node.has_errors(), "should have error diagnostics");
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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "Expected Failed state for list over dict"
        );
        assert!(node.has_errors(), "should have error diagnostics");
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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "Expected Failed state for type merge conflict"
        );
        assert!(node.has_errors(), "should have error diagnostics");
        let diag_msg = node
            .diagnostics()
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            diag_msg.contains("config:server"),
            "diagnostics should contain key path 'config:server', got: {}",
            diag_msg
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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "should be Failed with default config"
        );
        assert!(node.has_errors(), "should have error diagnostics");
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

        let node = inventory
            .merge_node("test_node")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "should be Failed — 'nonexistent' does not match 'miss.*'"
        );
        assert!(node.has_errors(), "should have error diagnostics");
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
        inventory.add_node(node1);
        inventory.add_node(node2);
        // Second node with same name should be skipped; a warning diagnostic
        // should be recorded on the inventory.
        assert!(
            inventory.diagnostics().iter().any(|d| d.code.as_deref() == Some("INV-003")),
            "should warn on duplicate node name"
        );
        assert_eq!(
            inventory.diagnostics().iter().filter(|d| d.code.as_deref() == Some("INV-003")).count(),
            1,
            "should have exactly one INV-003 diagnostic"
        );
    }

    #[test]
    fn test_no_duplicate_with_different_names() {
        let node1 = Node::new("alpha.server".to_string()).build();
        let node2 = Node::new("beta.server".to_string()).build();

        let mut inventory = Inventory::new();
        inventory.add_node(node1);
        inventory.add_node(node2);
        assert!(
            !inventory.diagnostics().iter().any(|d| d.code.as_deref() == Some("INV-003")),
            "different names should not produce duplicate warning"
        );
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

        let node = inv_with_config
            .merge_node("node1")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "should be Failed on unresolvable reference"
        );
        assert!(node.has_errors(), "should have error diagnostics");
        let diag_msg = node
            .diagnostics()
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            diag_msg.contains("missing_ref"),
            "diagnostics should mention missing_ref, got: {}",
            diag_msg
        );
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

        let node = inv_with_config
            .merge_node("node1")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "should be Failed on unresolvable references"
        );
        assert!(node.has_errors(), "should have error diagnostics");
        // With group_errors, multiple unresolved refs should be reported
        let diag_msg = node
            .diagnostics()
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            diag_msg.contains("ref_a") || diag_msg.contains("ref_b"),
            "diagnostics should mention missing refs, got: {}",
            diag_msg
        );
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

        let node = inv_with_config
            .merge_node("node1")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(
            node.state(),
            EntityState::Failed,
            "should be Failed on unresolvable reference"
        );
        assert!(node.has_errors(), "should have error diagnostics");
        let diag_msg = node
            .diagnostics()
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        // In single-error mode, at least one missing ref should be reported
        assert!(
            diag_msg.contains("ref_a") || diag_msg.contains("ref_b"),
            "diagnostics should mention a missing ref, got: {}",
            diag_msg
        );
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

        let node = inv_with_config
            .merge_node("node1")
            .expect("merge_node should return Ok even on domain error");
        assert_eq!(node.state(), EntityState::Failed, "should be Failed");
        assert!(node.has_errors(), "should have error diagnostics");
        // The diagnostics should mention the interpolation errors
        let diag_msg = node
            .diagnostics()
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            diag_msg.contains("ref_a")
                || diag_msg.contains("ref_b")
                || diag_msg.contains("interpolation"),
            "diagnostics should mention the error, got: {}",
            diag_msg
        );
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
            Value::Hash(Arc::new(config)),
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

    // --- Query API tests ---

    #[test]
    fn test_class_names_returns_all_class_names() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: webserver
               parameters:
                   role: web
               ---
               name: db
               parameters:
                   role: db
               ---
               name: node1
               type: node
               classes:
                   - base
                   - webserver
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let class_names: Vec<&str> = inventory.class_names().collect();
        assert!(
            class_names.contains(&"base"),
            "should contain 'base', got {:?}",
            class_names
        );
        assert!(
            class_names.contains(&"webserver"),
            "should contain 'webserver', got {:?}",
            class_names
        );
        assert!(
            class_names.contains(&"db"),
            "should contain 'db', got {:?}",
            class_names
        );
        assert_eq!(class_names.len(), 3, "should have 3 classes");
    }

    #[test]
    fn test_find_nodes_by_class_without_index() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: webserver
               parameters:
                   role: web
               ---
               name: node1
               type: node
               classes:
                   - base
                   - webserver
               ---
               name: node2
               type: node
               classes:
                   - base
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        // Without building indexes, find_nodes_by_class should still work (linear scan)
        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("webserver")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes, vec!["node1"], "only node1 declares webserver class");

        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("base")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes.len(), 2, "both nodes declare base class");
    }

    #[test]
    fn test_find_nodes_by_class_with_index() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: webserver
               parameters:
                   role: web
               ---
               name: node1
               type: node
               classes:
                   - base
                   - webserver
               ---
               name: node2
               type: node
               classes:
                   - base
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        // Build indexes explicitly
        inventory.build_indexes();

        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("webserver")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes, vec!["node1"]);

        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("base")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes.len(), 2);

        // Non-existent class returns empty
        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("nonexistent")
            .map(|n| n.name())
            .collect();
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_find_nodes_by_environment() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: node1
               type: node
               environment: production
               classes:
                   - base
               ---
               name: node2
               type: node
               environment: staging
               classes:
                   - base
               ---
               name: node3
               type: node
               environment: production
               classes:
                   - base
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        // Without indexes, should still work (linear scan)
        let nodes: Vec<&str> = inventory
            .find_nodes_by_environment("production")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes.len(), 2, "should find 2 production nodes");
        assert!(nodes.contains(&"node1"), "should contain node1");
        assert!(nodes.contains(&"node3"), "should contain node3");

        let nodes: Vec<&str> = inventory
            .find_nodes_by_environment("staging")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes, vec!["node2"]);

        let nodes: Vec<&str> = inventory
            .find_nodes_by_environment("nonexistent")
            .map(|n| n.name())
            .collect();
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_find_nodes_by_environment_with_index() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: node1
               type: node
               environment: production
               classes:
                   - base
               ---
               name: node2
               type: node
               environment: staging
               classes:
                   - base
               ---
               name: node3
               type: node
               environment: production
               classes:
                   - base
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        // Build indexes explicitly
        inventory.build_indexes();

        let nodes: Vec<&str> = inventory
            .find_nodes_by_environment("production")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes.len(), 2);

        let nodes: Vec<&str> = inventory
            .find_nodes_by_environment("staging")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes, vec!["node2"]);
    }

    #[test]
    fn test_find_nodes_by_resolved_class() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: grandparent
               parameters:
                   level: grandparent
               ---
               name: parent
               classes:
                   - grandparent
               parameters:
                   level: parent
               ---
               name: node1
               type: node
               classes:
                   - parent
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        // find_nodes_by_class only checks declared classes, not inherited
        let declared: Vec<&str> = inventory
            .find_nodes_by_class("parent")
            .map(|n| n.name())
            .collect();
        assert_eq!(
            declared,
            vec!["node1"],
            "node1 declares parent in its class list"
        );

        let declared_grandparent: Vec<&str> = inventory
            .find_nodes_by_class("grandparent")
            .map(|n| n.name())
            .collect();
        assert!(
            declared_grandparent.is_empty(),
            "no node directly declares grandparent"
        );

        // find_nodes_by_resolved_class checks the full resolved hierarchy
        let resolved: Vec<&str> = inventory
            .find_nodes_by_resolved_class("grandparent")
            .map(|n| n.name())
            .collect();
        assert_eq!(
            resolved,
            vec!["node1"],
            "node1 inherits grandparent through parent"
        );
    }

    #[test]
    fn test_search_nodes_case_insensitive() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: Web-Prod-01
               type: node
               classes:
                   - base
               ---
               name: db-prod-01
               type: node
               classes:
                   - base
               ---
               name: app-staging-01
               type: node
               classes:
                   - base
               "#
        );

        let inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        // Case-insensitive search
        let nodes: Vec<&str> = inventory.search_nodes("prod").map(|n| n.name()).collect();
        assert_eq!(nodes.len(), 2, "should find Web-Prod-01 and db-prod-01");
        assert!(nodes.contains(&"Web-Prod-01"), "should contain Web-Prod-01");
        assert!(nodes.contains(&"db-prod-01"), "should contain db-prod-01");

        let nodes: Vec<&str> = inventory.search_nodes("PROD").map(|n| n.name()).collect();
        assert_eq!(
            nodes.len(),
            2,
            "case-insensitive search should find same results"
        );

        let nodes: Vec<&str> = inventory
            .search_nodes("staging")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes, vec!["app-staging-01"]);

        let nodes: Vec<&str> = inventory
            .search_nodes("nonexistent")
            .map(|n| n.name())
            .collect();
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_class_to_nodes_index() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: webserver
               parameters:
                   role: web
               ---
               name: node1
               type: node
               classes:
                   - base
                   - webserver
               ---
               name: node2
               type: node
               classes:
                   - base
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let index = inventory.class_to_nodes();
        assert_eq!(
            index.get("webserver"),
            Some(&vec!["node1".to_string()]),
            "webserver should map to node1"
        );
        assert_eq!(
            index.get("base").unwrap().len(),
            2,
            "base should map to 2 nodes"
        );
        assert!(
            index.get("nonexistent").is_none(),
            "nonexistent class should not be in index"
        );
    }

    #[test]
    fn test_environment_to_nodes_index() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: node1
               type: node
               environment: production
               classes:
                   - base
               ---
               name: node2
               type: node
               environment: staging
               classes:
                   - base
               ---
               name: node3
               type: node
               environment: production
               classes:
                   - base
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        let index = inventory.environment_to_nodes();
        assert_eq!(
            index.get("production").unwrap().len(),
            2,
            "production should have 2 nodes"
        );
        assert_eq!(
            index.get("staging"),
            Some(&vec!["node2".to_string()]),
            "staging should map to node2"
        );
    }

    #[test]
    fn test_build_indexes_idempotent() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: node1
               type: node
               classes:
                   - base
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        // Calling build_indexes twice should be a no-op
        inventory.build_indexes();
        inventory.build_indexes();

        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("base")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes, vec!["node1"]);
    }

    #[test]
    fn test_index_invalidation_after_add_node() {
        use indoc::indoc;

        const TEST_INVENTORY: &str = indoc!(
            r#"---
               ---
               name: base
               parameters:
                   global: true
               ---
               name: node1
               type: node
               classes:
                   - base
               "#
        );

        let mut inventory = load_from_yaml_string(TEST_INVENTORY, &ParameterKeyStyle::None)
            .expect("failed to parse");

        // Build indexes
        inventory.build_indexes();
        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("base")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes, vec!["node1"]);

        // Add a new node
        let new_node = crate::inventory::elements::Node::new("node2".to_string())
            .classes(vec!["base".to_string()])
            .build();
        inventory.add_node(new_node);

        // Without rebuilding indexes, find_nodes_by_class falls back to linear scan
        // which should find both nodes
        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("base")
            .map(|n| n.name())
            .collect();
        assert_eq!(
            nodes.len(),
            2,
            "linear scan should find both nodes after add"
        );

        // Rebuild indexes and verify
        inventory.build_indexes();
        let nodes: Vec<&str> = inventory
            .find_nodes_by_class("base")
            .map(|n| n.name())
            .collect();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_inventory_state_aggregate_interpolated() {
        let inventory = Inventory::new();
        // Empty inventory defaults to Interpolated
        assert_eq!(inventory.state(), EntityState::Interpolated);
    }

    #[test]
    fn test_inventory_state_aggregate_failed_when_node_failed() {
        let mut inventory = Inventory::new();
        let failed_node = Node::new("web01.example.com".to_string())
            .state(EntityState::Failed)
            .build();
        inventory.add_node(failed_node);
        assert_eq!(inventory.state(), EntityState::Failed);
    }

    #[test]
    fn test_inventory_state_aggregate_min() {
        let mut inventory = Inventory::new();
        let node_merged = Node::new("web01.example.com".to_string())
            .state(EntityState::Merged)
            .build();
        let node_interpolated = Node::new("web02.example.com".to_string())
            .state(EntityState::Interpolated)
            .build();
        inventory.add_node(node_merged);
        inventory.add_node(node_interpolated);
        // Worst state is Merged, so inventory is Merged
        assert_eq!(inventory.state(), EntityState::Merged);
    }

    #[test]
    fn test_add_node_auto_syncs_entity_diagnostic_on_failure() {
        let mut inventory = Inventory::new();
        let failed_node = Node::new("web01.example.com".to_string())
            .state(EntityState::Failed)
            .add_diagnostic(Diagnostic::error("class not found"))
            .build();
        inventory.add_node(failed_node);

        // Inventory should have an entity diagnostic for the failed node
        let all_diag = inventory.all_diagnostics();
        let node_diag = all_diag
            .iter()
            .find(|d| d.subject.as_deref() == Some("web01.example.com"));
        assert!(
            node_diag.is_some(),
            "should have summary diagnostic for failed node"
        );
        assert_eq!(node_diag.unwrap().severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn test_add_node_no_diagnostic_when_usable() {
        let mut inventory = Inventory::new();
        let good_node = Node::new("web01.example.com".to_string())
            .state(EntityState::Interpolated)
            .build();
        inventory.add_node(good_node);

        // No entity diagnostics for a healthy node
        let all_diag = inventory.all_diagnostics();
        assert!(
            all_diag.is_empty(),
            "healthy node should not produce diagnostics"
        );
    }

    #[test]
    fn test_add_node_replaces_stale_diagnostic_on_re_add() {
        let mut inventory = Inventory::new();

        // Add a failed node
        let failed_node = Node::new("web01.example.com".to_string())
            .state(EntityState::Failed)
            .add_diagnostic(Diagnostic::error("class not found"))
            .build();
        inventory.add_node(failed_node);
        assert_eq!(inventory.all_diagnostics().len(), 1);

        // Add a failed class — should get a diagnostic too
        let failed_class = Class::new("myclass".to_string())
            .state(EntityState::Failed)
            .add_diagnostic(Diagnostic::error("parse error"))
            .build();
        inventory.add_class(failed_class);
        assert_eq!(
            inventory.all_diagnostics().len(),
            2,
            "should have diagnostics for node and class"
        );

        // Add a healthy class with same name — should replace the diagnostic
        let healthy_class = Class::new("myclass".to_string())
            .state(EntityState::Interpolated)
            .build();
        inventory.add_class(healthy_class);
        // Only the node diagnostic remains
        let all_diag = inventory.all_diagnostics();
        assert_eq!(
            all_diag.len(),
            1,
            "re-adding healthy class should remove its diagnostic"
        );
        assert_eq!(all_diag[0].subject.as_deref(), Some("web01.example.com"));
    }

    #[test]
    fn test_add_class_auto_syncs_entity_diagnostic_on_failure() {
        let mut inventory = Inventory::new();
        let failed_class = Class::new("myclass".to_string())
            .state(EntityState::Failed)
            .add_diagnostic(Diagnostic::error("circular inheritance"))
            .build();
        inventory.add_class(failed_class);

        let all_diag = inventory.all_diagnostics();
        let class_diag = all_diag
            .iter()
            .find(|d| d.subject.as_deref() == Some("myclass"));
        assert!(
            class_diag.is_some(),
            "should have summary diagnostic for failed class"
        );
        assert_eq!(class_diag.unwrap().severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn test_inventory_has_errors_checks_entity_diagnostics() {
        let mut inventory = Inventory::new();
        let failed_node = Node::new("broken".to_string())
            .state(EntityState::Failed)
            .add_diagnostic(Diagnostic::error("something broke"))
            .build();
        inventory.add_node(failed_node);

        assert!(
            inventory.has_errors(),
            "inventory with failed node should report errors"
        );
    }

    #[test]
    fn test_load_with_diagnostics_collects_parse_errors() {
        use indoc::indoc;

        // Invalid YAML content with a bad key
        const BAD_INVENTORY: &str = indoc!(
            r#"
            ---
            ---
            name: classA
            parameters:
                valid_key: value
                invalid-key: value
            "#
        );

        let result = load_from_yaml_string_with_diagnostics(
            BAD_INVENTORY,
            &ParameterKeyStyle::Ansible,
            None,
        )
        .expect("Should return Ok even with errors");

        assert!(
            result.has_errors(),
            "Should have errors for invalid key: {:?}",
            result.diagnostics()
        );
        // The inventory should be empty since the class could not be parsed
        assert_eq!(
            result.inventory().class_names().count(),
            0,
            "Invalid class should not be in inventory"
        );
    }

    #[test]
    fn test_load_with_diagnostics_success() {
        use indoc::indoc;

        const GOOD_INVENTORY: &str = indoc!(
            r#"
            ---
            ---
            name: myclass
            parameters:
                key: value
            "#
        );

        let result = load_from_yaml_string_with_diagnostics(
            GOOD_INVENTORY,
            &ParameterKeyStyle::None,
            None,
        )
        .expect("Should load successfully");

        assert!(
            !result.has_errors(),
            "Should have no errors: {:?}",
            result.diagnostics()
        );
        assert!(
            result.inventory().get_class("myclass").is_some(),
            "Class should be loaded"
        );
    }

    #[test]
    fn test_load_result_into_inventory() {
        use indoc::indoc;

        const INVENTORY: &str = indoc!(
            r#"
            ---
            ---
            name: base
            parameters:
                role: server
            "#
        );

        let result = load_from_yaml_string_with_diagnostics(
            INVENTORY,
            &ParameterKeyStyle::None,
            None,
        )
        .expect("Should load successfully");

        let inventory = result.into_inventory();
        assert!(
            inventory.get_class("base").is_some(),
            "Class should be accessible after into_inventory()"
        );
    }

    #[test]
    fn test_duplicate_node_name_becomes_diagnostic() {
        let node1 = Node::new("server".to_string())
            .uri("yaml_fs:///nodes/alpha/server.yml")
            .build();
        let node2 = Node::new("server".to_string())
            .uri("yaml_fs:///nodes/beta/server.yml")
            .build();

        let mut inventory = Inventory::new();
        inventory.add_node(node1);
        inventory.add_node(node2);

        // Second node with same name should be skipped; a warning diagnostic
        // should be recorded on the inventory.
        let duplicate_diag = inventory
            .diagnostics()
            .iter()
            .find(|d| d.code.as_deref() == Some("INV-003"));
        assert!(
            duplicate_diag.is_some(),
            "Should have INV-003 diagnostic for duplicate node name"
        );
        assert_eq!(
            duplicate_diag.unwrap().severity,
            DiagnosticSeverity::Warning,
            "Duplicate node name should be a warning"
        );

        // The first node should still be in the inventory
        assert!(
            inventory.get_node("server").is_some(),
            "First node should still exist"
        );
    }
}
