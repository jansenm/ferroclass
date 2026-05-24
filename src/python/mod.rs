// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Python bindings for ferroclass, exposed via PyO3.
//!
//! This module provides a `ferroclass` Python package with two primary
//! entry points matching the Salt external pillar and master tops interfaces:
//!
//! - [`ext_pillar`] — returns pillar data for a single Salt minion
//! - [`top`] — returns top data (environment → node → applications)
//!
//! It also exposes [`PyInventory`] and [`PyNode`] for direct programmatic use.

mod adapter;
mod error;
mod inventory;
mod node;
mod value;

use pyo3::prelude::*;

/// The `ferroclass` Python module.
///
/// Usage from Python:
///
/// ```python
/// import ferroclass
///
/// # Salt ext_pillar interface
/// pillar = ferroclass.ext_pillar(
///     "web01.example.com",
///     inventory_base_uri="/srv/reclass",
/// )
///
/// # Salt master_tops interface (single minion)
/// top_data = ferroclass.top(
///     minion_id="web01.example.com",
///     inventory_base_uri="/srv/reclass",
/// )
///
/// # Salt master_tops interface (full inventory)
/// top_data = ferroclass.top(
///     minion_id=None,
///     inventory_base_uri="/srv/reclass",
/// )
///
/// # Low-level inventory access
/// inv = ferroclass.load(inventory_base_uri="/srv/reclass")
/// node = inv.merge_node("web01.example.com")
/// ```
#[pymodule]
fn ferroclass(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<inventory::PyInventory>()?;
    m.add_class::<node::PyNode>()?;
    m.add_function(wrap_pyfunction!(adapter::ext_pillar, m)?)?;
    m.add_function(wrap_pyfunction!(adapter::top, m)?)?;
    m.add_function(wrap_pyfunction!(adapter::load, m)?)?;
    m.add("ReclassError", m.getattr("ReclassError")?)?;
    Ok(())
}
