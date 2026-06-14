// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Python wrapper for ferroclass [`Node`].

use crate::inventory::elements::Node;
use crate::inventory::value::Value;
use crate::python::value;

use pyo3::prelude::*;

/// A merged reclass node.
///
/// Provides read-only access to the fully-merged parameters, classes,
/// applications, and environment of a single inventory node.
#[pyclass(name = "Node")]
pub struct PyNode {
    node: Node,
}

impl PyNode {
    pub fn new(node: Node) -> Self {
        Self { node }
    }
}

#[pymethods]
impl PyNode {
    /// The node name (e.g. `"web01.example.com"`).
    #[getter]
    fn name(&self) -> String {
        self.node.name().to_string()
    }

    /// The list of classes this node inherits from.
    #[getter]
    fn classes(&self) -> Vec<String> {
        self.node.classes().clone()
    }

    /// The list of applications (states) for this node.
    #[getter]
    fn applications(&self) -> Vec<String> {
        self.node.applications().as_list().to_vec()
    }

    /// The node's environment (e.g. `"base"`, `"production"`).
    #[getter]
    fn environment(&self) -> String {
        self.node.environment().to_string()
    }

    /// The node's merged parameters as a Python `dict`.
    ///
    /// Insertion order of keys is preserved.
    #[getter]
    fn parameters(&self, py: Python<'_>) -> PyResult<PyObject> {
        value::value_to_py(
            &Value::Hash(std::sync::Arc::new(self.node.parameters().clone())),
            py,
        )
    }

    /// The node's exports as a Python `dict`.
    #[getter]
    fn exports(&self, py: Python<'_>) -> PyResult<PyObject> {
        value::value_to_py(
            &Value::Hash(std::sync::Arc::new(self.node.exports().clone())),
            py,
        )
    }
}
