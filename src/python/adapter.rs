// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Salt-facing adapter functions that replicate the Python `reclass.adapters.salt`
//! `ext_pillar()` and `top()` API.
//!
//! These are the primary entry points for Salt integration. They accept
//! the same keyword arguments as the Python reclass adapter and produce
//! the same output format.

use crate::inventory as inv;
use crate::inventory::class_mapping::ClassMapping;
use crate::inventory::options::{Options, StorageOptions, StorageType, YamlFsStorageOptions};
use crate::python::error;
use crate::python::inventory::PyInventory;
use crate::python::value;

use hashlink::LinkedHashMap;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Build `StorageOptions` from keyword arguments matching the Python reclass API.
fn build_storage_options(
    storage_type: &str,
    inventory_base_uri: &str,
    nodes_uri: &str,
    classes_uri: &str,
    compose_node_name: bool,
    default_environment: &str,
) -> Result<StorageOptions, String> {
    let st = match storage_type {
        "yaml_fs" => StorageType::YamlFs,
        "yaml_file" => StorageType::YamlFile,
        other => return Err(format!("unknown storage_type '{other}'")),
    };
    let yaml_fs_options = YamlFsStorageOptions {
        inventory_base_uri: inventory_base_uri.to_string(),
        nodes_uri: nodes_uri.to_string(),
        classes_uri: classes_uri.to_string(),
        compose_node_name,
        default_environment: inv::value::Environment::from(default_environment.to_string()),
        ..YamlFsStorageOptions::default()
    };
    Ok(StorageOptions {
        storage_type: st,
        yaml_fs_options,
        ..StorageOptions::default()
    })
}

/// Return pillar data for a single Salt minion.
///
/// This is the Python `ext_pillar` interface. It loads the inventory,
/// merges the given node, injects `__reclass__` metadata, and returns
/// the parameters as a Python `dict`.
///
/// The `pillarenv` parameter maps to the Python reclass
/// `allow_adapter_env_override` option. When `allow_adapter_env_override`
/// is `False` (the default), `pillarenv` is ignored and the node's
/// own environment is used.
///
/// The `class_mappings` parameter accepts a list of mapping strings
/// (e.g. `["* default", "/^www\\d+/ webserver"]`) that auto-include
/// classes for nodes matching the pattern. Matches Python reclass
/// `class_mappings` option.
///
/// .. note::
///
///    The `pillar` and `propagate_pillar_data_to_reclass` parameters from
///    the Python reclass API are accepted but ignored in this version.
///    Pillar propagation will be implemented in a future release.
#[pyfunction]
#[pyo3(signature = (minion_id, pillar=None, pillarenv=None, storage_type="yaml_fs",
                    inventory_base_uri="/etc/reclass", nodes_uri="nodes",
                    classes_uri="classes", class_mappings=None,
                    class_mappings_match_path=false,
                    compose_node_name=false,
                    default_environment="base",
                    allow_adapter_env_override=false,
                    ignore_class_notfound=false,
                    **kwargs))]
#[allow(clippy::too_many_arguments)]
pub fn ext_pillar(
    py: Python<'_>,
    minion_id: &str,
    pillar: Option<&Bound<'_, PyDict>>,
    pillarenv: Option<&str>,
    storage_type: &str,
    inventory_base_uri: &str,
    nodes_uri: &str,
    classes_uri: &str,
    class_mappings: Option<Vec<String>>,
    class_mappings_match_path: bool,
    compose_node_name: bool,
    default_environment: &str,
    allow_adapter_env_override: bool,
    ignore_class_notfound: bool,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyObject> {
    let _ = pillar; // v1: not propagated to reclass
    let _ = kwargs; // future extensibility

    let effective_pillarenv = if allow_adapter_env_override {
        pillarenv
    } else {
        None
    };

    let storage_options = build_storage_options(
        storage_type,
        inventory_base_uri,
        nodes_uri,
        classes_uri,
        compose_node_name,
        default_environment,
    )
    .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)?;

    let mut opts = Options {
        storage_options,
        ignore_class_notfound,
        ..Options::default()
    };

    if let Some(env) = effective_pillarenv {
        opts.default_environment = inv::value::Environment::from(env.to_string());
    }

    let merge_config = opts.build_merge_config();
    let mut inventory = inv::load(&opts.storage_options).map_err(error::to_py_err)?;
    inventory.set_merge_config(merge_config);

    if let Some(raw_mappings) = class_mappings {
        let mappings = parse_class_mappings(&raw_mappings)?;
        inventory.set_class_mappings(mappings);
        inventory.set_class_mappings_match_path(class_mappings_match_path);
    }

    let node = inventory.merge_node(minion_id).map_err(error::to_py_err)?;

    if !node.is_usable() {
        let msg = node
            .diagnostics()
            .first()
            .map(|d| d.message.as_str())
            .unwrap_or("merge failed");
        return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "node '{}' failed to merge: {}",
            minion_id, msg
        )));
    }

    let has_inv_query = {
        let params = node.parameters();
        params.values().any(|v| v.has_inv_query())
            || node.exports().values().any(|v| v.has_inv_query())
    };

    let node = if has_inv_query {
        let inv_map = inventory.build_inventory_map().map_err(error::to_py_err)?;
        let n = inventory
            .merge_node_with_inventory(minion_id, &inv_map)
            .map_err(error::to_py_err)?;
        if !n.is_usable() {
            let msg = n
                .diagnostics()
                .first()
                .map(|d| d.message.as_str())
                .unwrap_or("inv query render failed");
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "node '{}' failed to render inv queries: {}",
                minion_id, msg
            )));
        }
        n
    } else {
        node
    };

    value::parameters_to_pillar_dict(
        node.parameters(),
        minion_id,
        node.classes(),
        node.applications().as_list(),
        node.environment(),
        py,
    )
}

/// Return top data for the Salt `master_tops` interface.
///
/// When `minion_id` is provided, returns `{environment: [applications]}`
/// for that single minion.
///
/// When `minion_id` is `None`, returns `{environment: {node: [applications]}}`
/// for all nodes in the inventory.
#[pyfunction]
#[pyo3(signature = (minion_id=None, pillarenv=None, storage_type="yaml_fs",
                    inventory_base_uri="/etc/reclass", nodes_uri="nodes",
                    classes_uri="classes", class_mappings=None,
                    class_mappings_match_path=false,
                    compose_node_name=false,
                    default_environment="base",
                    allow_adapter_env_override=false,
                    ignore_class_notfound=false,
                    **kwargs))]
#[allow(clippy::too_many_arguments)]
pub fn top(
    py: Python<'_>,
    minion_id: Option<&str>,
    pillarenv: Option<&str>,
    storage_type: &str,
    inventory_base_uri: &str,
    nodes_uri: &str,
    classes_uri: &str,
    class_mappings: Option<Vec<String>>,
    class_mappings_match_path: bool,
    compose_node_name: bool,
    default_environment: &str,
    allow_adapter_env_override: bool,
    ignore_class_notfound: bool,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyObject> {
    let _ = kwargs;

    let effective_pillarenv = if allow_adapter_env_override {
        pillarenv
    } else {
        None
    };

    let storage_options = build_storage_options(
        storage_type,
        inventory_base_uri,
        nodes_uri,
        classes_uri,
        compose_node_name,
        default_environment,
    )
    .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)?;

    let mut opts = Options {
        storage_options,
        ignore_class_notfound,
        ..Options::default()
    };

    if let Some(env) = effective_pillarenv {
        opts.default_environment = inv::value::Environment::from(env.to_string());
    }

    let merge_config = opts.build_merge_config();
    let ignore_failed_node = merge_config.inventory_ignore_failed_node;
    let ignore_failed_render = merge_config.inventory_ignore_failed_render;
    let mut inventory = inv::load(&opts.storage_options).map_err(error::to_py_err)?;
    inventory.set_merge_config(merge_config);

    if let Some(raw_mappings) = class_mappings {
        let mappings = parse_class_mappings(&raw_mappings)?;
        inventory.set_class_mappings(mappings);
        inventory.set_class_mappings_match_path(class_mappings_match_path);
    }

    match minion_id {
        Some(mid) => {
            // Single minion: return {environment: [applications]}
            let node = inventory.merge_node(mid).map_err(error::to_py_err)?;

            if !node.is_usable() {
                let msg = node
                    .diagnostics()
                    .first()
                    .map(|d| d.message.as_str())
                    .unwrap_or("merge failed");
                return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "node '{}' failed to merge: {}",
                    mid, msg
                )));
            }

            let has_inv_query = {
                let params = node.parameters();
                params.values().any(|v| v.has_inv_query())
                    || node.exports().values().any(|v| v.has_inv_query())
            };

            let node = if has_inv_query {
                let inv_map = inventory.build_inventory_map().map_err(error::to_py_err)?;
                let n = inventory
                    .merge_node_with_inventory(mid, &inv_map)
                    .map_err(error::to_py_err)?;
                if !n.is_usable() {
                    let msg = n
                        .diagnostics()
                        .first()
                        .map(|d| d.message.as_str())
                        .unwrap_or("inv query render failed");
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "node '{}' failed to render inv queries: {}",
                        mid, msg
                    )));
                }
                n
            } else {
                node
            };

            value::single_top_to_py(node.environment(), node.applications().as_list(), py)
        }
        None => {
            // Full inventory: return {environment: {node: [applications]}}
            let mut environments: LinkedHashMap<String, LinkedHashMap<String, Vec<String>>> =
                LinkedHashMap::new();

            for raw_node in inventory.nodes_iter() {
                let merged = match inventory.merge_node(raw_node.name()) {
                    Ok(n) => n,
                    Err(e) => {
                        // Implementation error (I/O, etc.) — propagate
                        return Err(error::to_py_err(e));
                    }
                };

                if !merged.is_usable() {
                    if ignore_failed_node {
                        tracing::warn!(
                            "Skipping failed node '{}': {}",
                            raw_node.name(),
                            merged
                                .diagnostics()
                                .first()
                                .map(|d| d.message.as_str())
                                .unwrap_or("unknown error")
                        );
                        continue;
                    }
                    return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "node '{}' failed to merge",
                        raw_node.name()
                    )));
                }

                let has_inv_query = {
                    let params = merged.parameters();
                    params.values().any(|v| v.has_inv_query())
                        || merged.exports().values().any(|v| v.has_inv_query())
                };

                let merged = if has_inv_query {
                    let inv_map = inventory.build_inventory_map().map_err(error::to_py_err)?;
                    match inventory.merge_node_with_inventory(raw_node.name(), &inv_map) {
                        Ok(n) => {
                            if !n.is_usable() {
                                if ignore_failed_render {
                                    tracing::warn!(
                                        "Skipping node '{}': inv query render failed: {}",
                                        raw_node.name(),
                                        n.diagnostics()
                                            .first()
                                            .map(|d| d.message.as_str())
                                            .unwrap_or("unknown error")
                                    );
                                    continue;
                                }
                                return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                                    "node '{}' failed to render inv queries",
                                    raw_node.name()
                                )));
                            }
                            n
                        }
                        Err(e) => {
                            return Err(error::to_py_err(e));
                        }
                    }
                } else {
                    merged
                };

                let env = merged.environment().to_string();
                let apps: Vec<String> = merged.applications().as_list().to_vec();
                environments
                    .entry(env)
                    .or_insert_with(LinkedHashMap::new)
                    .insert(merged.name().to_string(), apps);
            }

            value::top_to_py(&environments, py)
        }
    }
}

/// Low-level inventory loader.
///
/// Returns a [`PyInventory`] object that can be queried for node names
/// and merged nodes.
#[pyfunction]
#[pyo3(signature = (storage_type="yaml_fs",
                    inventory_base_uri="/etc/reclass",
                    nodes_uri="nodes",
                    classes_uri="classes",
                    class_mappings=None,
                    class_mappings_match_path=false,
                    compose_node_name=false,
                    default_environment="base",
                    ignore_class_notfound=false))]
#[allow(clippy::too_many_arguments)]
pub fn load(
    storage_type: &str,
    inventory_base_uri: &str,
    nodes_uri: &str,
    classes_uri: &str,
    class_mappings: Option<Vec<String>>,
    class_mappings_match_path: bool,
    compose_node_name: bool,
    default_environment: &str,
    ignore_class_notfound: bool,
) -> PyResult<PyInventory> {
    let storage_options = build_storage_options(
        storage_type,
        inventory_base_uri,
        nodes_uri,
        classes_uri,
        compose_node_name,
        default_environment,
    )
    .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)?;

    let opts = Options {
        storage_options,
        ignore_class_notfound,
        ..Options::default()
    };

    let mut inventory = inv::load(&opts.storage_options).map_err(error::to_py_err)?;

    if let Some(raw_mappings) = class_mappings {
        let mappings = parse_class_mappings(&raw_mappings)?;
        inventory.set_class_mappings(mappings);
        inventory.set_class_mappings_match_path(class_mappings_match_path);
    }

    Ok(PyInventory::new(inventory))
}

fn parse_class_mappings(raw: &[String]) -> PyResult<Vec<ClassMapping>> {
    raw.iter()
        .map(|s| {
            ClassMapping::parse(s).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "invalid class_mapping '{}': {e}",
                    s
                ))
            })
        })
        .collect()
}
