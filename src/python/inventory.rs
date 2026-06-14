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
}
