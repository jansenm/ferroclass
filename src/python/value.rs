// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Conversion from ferroclass [`Value`] to Python objects.
//!
//! After merging and interpolation, all [`Value`] variants except the
//! plain-YAML ones should be fully resolved. Defensive stringification
//! handles any residual internal variants.

use crate::inventory::value::{Hash, Key, Value};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::sync::Arc;

/// Convert a boolean to a Python bool object.
fn bool_to_py(b: bool, py: Python<'_>) -> PyObject {
    let borrowed = pyo3::types::PyBool::new(py, b);
    let bound = borrowed.to_owned();
    bound.into_any().unbind()
}

/// Convert a [`Key`] to a Python object.
pub fn key_to_py(key: &Key, py: Python<'_>) -> PyResult<PyObject> {
    match key {
        Key::String(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        Key::Integer(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        Key::Boolean(b) => Ok(bool_to_py(*b, py)),
        Key::Null => Ok(py.None()),
    }
}

/// Convert a [`Value`] to a Python object.
///
/// This recursively walks the value tree:
///
/// - [`Value::Hash`] → `dict` (insertion order preserved)
/// - [`Value::Array`] → `list`
/// - [`Value::String`] → `str`
/// - [`Value::Integer`] → `int`
/// - [`Value::Boolean`] → `bool`
/// - [`Value::Null`] → `None`
/// - [`Value::Real`] → `float` (parsed from its string representation)
/// - Internal variants (references, queries, markers) → `str` (via Debug format)
pub fn value_to_py(val: &Value, py: Python<'_>) -> PyResult<PyObject> {
    match val {
        Value::Hash(rc_hash) => hash_to_py(rc_hash, py),
        Value::Array(rc_arr) => array_to_py(rc_arr, py),
        Value::String(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        Value::Integer(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        Value::Boolean(b) => Ok(bool_to_py(*b, py)),
        Value::Null => Ok(py.None()),
        Value::Real(s) => match s.parse::<f64>() {
            Ok(f) => Ok(f.into_pyobject(py)?.into_any().unbind()),
            Err(_) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        },
        // After merge, internal variants should be fully resolved.
        // Defensive: convert to Debug string representation.
        Value::Reference(segments) => {
            let s = format!("{:?}", segments);
            Ok(s.into_pyobject(py)?.into_any().unbind())
        }
        Value::StringWithReference(parts) => {
            let s = format!("{:?}", parts);
            Ok(s.into_pyobject(py)?.into_any().unbind())
        }
        Value::InvQuery(data) => {
            let s = format!("{:?}", data);
            Ok(s.into_pyobject(py)?.into_any().unbind())
        }
        Value::StringWithInvQuery(parts) => {
            let s = format!("{:?}", parts);
            Ok(s.into_pyobject(py)?.into_any().unbind())
        }
        Value::DeferredMerge(vals) => {
            let s = format!("{:?}", vals);
            Ok(s.into_pyobject(py)?.into_any().unbind())
        }
        Value::OverrideMarker(inner) => value_to_py(inner.as_ref(), py),
        Value::ConstantMarker(inner) => value_to_py(inner.as_ref(), py),
    }
}

/// Convert a [`Hash`] (ordered map) to a Python `dict`.
fn hash_to_py(rc_hash: &Arc<Hash>, py: Python<'_>) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    for (k, v) in rc_hash.iter() {
        let py_key = key_to_py(k, py)?;
        let py_val = value_to_py(v, py)?;
        dict.set_item(py_key, py_val)?;
    }
    Ok(dict.into_any().unbind())
}

/// Convert an [`Array`] (Vec<Value>) to a Python `list`.
fn array_to_py(rc_arr: &Arc<Vec<Value>>, py: Python<'_>) -> PyResult<PyObject> {
    let list = PyList::empty(py);
    for v in rc_arr.iter() {
        let py_val = value_to_py(v, py)?;
        list.append(py_val)?;
    }
    Ok(list.into_any().unbind())
}

/// Convert a [`Hash`] (parameters) to a Python `dict`, injecting the
/// `__reclass__` metadata that the Salt adapter expects.
///
/// This is the Python equivalent of [`salt::inject_salt_reclass_fields`]
/// plus [`ansible::inject_classes_and_applications_into_reclass`], producing
/// the same output as `ferroclass.ext_pillar()`.
///
/// [`salt::inject_salt_reclass_fields`]: crate::output::salt::inject_salt_reclass_fields
/// [`ansible::inject_classes_and_applications_into_reclass`]: crate::output::ansible::inject_classes_and_applications_into_reclass
pub fn parameters_to_pillar_dict(
    parameters: &Hash,
    nodename: &str,
    classes: &[String],
    applications: &[String],
    environment: &dyn std::fmt::Display,
    py: Python<'_>,
) -> PyResult<PyObject> {
    let mut params: Hash = parameters.clone();

    // Inject classes and applications into __reclass__
    let reclass_key = Key::String("__reclass__".to_string());
    let mut reclass_hash = match params.remove(&reclass_key) {
        Some(Value::Hash(h)) => Arc::try_unwrap(h).unwrap_or_else(|rc| (*rc).clone()),
        _ => Hash::new(),
    };
    reclass_hash.insert(
        Key::String("nodename".to_string()),
        Value::String(nodename.to_string()),
    );
    reclass_hash.insert(
        Key::String("classes".to_string()),
        Value::Array(Arc::new(
            classes.iter().cloned().map(Value::String).collect(),
        )),
    );
    reclass_hash.insert(
        Key::String("applications".to_string()),
        Value::Array(Arc::new(
            applications.iter().cloned().map(Value::String).collect(),
        )),
    );
    reclass_hash.insert(
        Key::String("environment".to_string()),
        Value::String(environment.to_string()),
    );
    params.insert(reclass_key, Value::Hash(Arc::new(reclass_hash)));

    value_to_py(&Value::Hash(Arc::new(params)), py)
}

/// Convert top data (environment → node → applications) to a Python `dict`.
///
/// For all minions: `{environment: {node: [applications]}}`.
pub fn top_to_py<'py>(
    data: &hashlink::LinkedHashMap<String, hashlink::LinkedHashMap<String, Vec<String>>>,
    py: Python<'py>,
) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    for (env, nodes) in data {
        let nodes_dict = PyDict::new(py);
        for (node, apps) in nodes {
            let py_apps = PyList::new(py, apps)?;
            nodes_dict.set_item(node, py_apps)?;
        }
        dict.set_item(env, nodes_dict)?;
    }
    Ok(dict.into_any().unbind())
}

/// Convert single-minion top data to a Python `dict`.
///
/// Returns `{environment: [applications]}`.
pub fn single_top_to_py<'py>(
    environment: &dyn std::fmt::Display,
    applications: &[String],
    py: Python<'py>,
) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    let py_apps = PyList::new(py, applications)?;
    dict.set_item(environment.to_string(), py_apps)?;
    Ok(dict.into_any().unbind())
}
