// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Python exception types for the ferroclass bindings.

use crate::inventory as inv;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

/// Convert a ferroclass [`inv::Error`](crate::inventory::Error) into a Python exception.
///
/// All ferroclass errors are surfaced as Python `RuntimeError`.
pub fn to_py_err(err: inv::Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}
