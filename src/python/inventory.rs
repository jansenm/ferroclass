// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Python wrapper for ferroclass [`Inventory`].

use crate::inventory as inv;
use crate::python::error;
use crate::python::node::PyNode;

use pyo3::prelude::*;

/// A loaded reclass inventory.
///
/// Create with [`load`] or [`ferroclass.load`], then query individual
/// nodes via [`merge_node`].
#[pyclass(name = "Inventory")]
pub struct PyInventory {
    inventory: inv::Inventory,
}

impl PyInventory {
    pub fn new(inventory: inv::Inventory) -> Self {
        Self { inventory }
    }
}

#[pymethods]
impl PyInventory {
    /// Fully merge a single node and return the result.
    ///
    /// This resolves class inheritance, interpolation, and inventory queries.
    /// Raises `RuntimeError` if the node does not exist or merging fails.
    fn merge_node(&self, _py: Python<'_>, name: &str) -> PyResult<PyNode> {
        let node = self.inventory.merge_node(name).map_err(error::to_py_err)?;
        Ok(PyNode::new(node))
    }

    /// Return the names of all loaded nodes in insertion order.
    fn node_names(&self) -> Vec<String> {
        self.inventory.node_names().map(|s| s.to_string()).collect()
    }

    /// Return the names of all loaded classes in insertion order.
    fn class_names(&self) -> Vec<String> {
        self.inventory
            .class_names()
            .map(|s| s.to_string())
            .collect()
    }

    /// Find all nodes that include the given class in their declared class list.
    ///
    /// Returns a list of node names. This only checks declared classes,
    /// not inherited ones. Use `find_nodes_by_resolved_class` to search
    /// the full resolved class hierarchy.
    fn find_nodes_by_class(&mut self, class_name: &str) -> Vec<String> {
        self.inventory
            .find_nodes_by_class(class_name)
            .map(|n| n.name().to_string())
            .collect()
    }

    /// Find all nodes whose resolved class list includes the given class.
    ///
    /// This merges each node to resolve the full inheritance chain.
    /// Nodes that fail to merge are skipped. Slower than
    /// `find_nodes_by_class` but more thorough.
    fn find_nodes_by_resolved_class(&self, class_name: &str) -> Vec<String> {
        self.inventory
            .find_nodes_by_resolved_class(class_name)
            .map(|n| n.name().to_string())
            .collect()
    }

    /// Find all nodes in the given environment.
    fn find_nodes_by_environment(&mut self, environment: &str) -> Vec<String> {
        self.inventory
            .find_nodes_by_environment(environment)
            .map(|n| n.name().to_string())
            .collect()
    }

    /// Search for nodes whose name contains the pattern (case-insensitive).
    fn search_nodes(&self, pattern: &str) -> Vec<String> {
        self.inventory
            .search_nodes(pattern)
            .map(|n| n.name().to_string())
            .collect()
    }

    /// Build reverse indexes for fast queries.
    ///
    /// Call this after loading if you plan to use `find_nodes_by_class`
    /// or `find_nodes_by_environment` frequently.
    fn build_indexes(&mut self) {
        self.inventory.build_indexes();
    }
}
