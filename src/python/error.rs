// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Python exception types for the ferroclass bindings.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Convert a ferroclass [`inv::Error`](crate::inventory::Error) into a Python exception.
///
/// All ferroclass errors are surfaced as `ferroclass.ReclassError`,
/// a subclass of Python's `RuntimeError`.
pub fn to_py_err(err: crate::inventory::Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}
