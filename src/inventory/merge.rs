// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::Inventory;
use crate::inventory::applications::Applications;
use crate::inventory::diagnostic::{Diagnostic, EntityState};
use crate::inventory::elements::{Class, Node};
use crate::inventory::interpolation;
use crate::inventory::inv_query;
use crate::inventory::options::MergeConfig;
use crate::inventory::value::{
    ClassList, Environment, ParametersType, Value, contains_interpolation,
};
use snafu::Snafu;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("class '{class_name}' not found"))]
    ClassNotFound { class_name: String },
    #[snafu(display("class name '{class_name}' could not be resolved"))]
    ClassNameResolveError {
        class_name: String,
        source: interpolation::Error,
    },
    #[snafu(transparent)]
    Interpolation { source: interpolation::Error },
    #[snafu(transparent)]
    ValueMerge {
        source: crate::inventory::value_merge::Error,
    },
}

/// Create a Failed node with minimal data and a diagnostic.
///
/// Used when a domain error (missing class, interpolation failure, etc.)
/// prevents successful merging. The returned node has:
/// - The original node's name, environment, URI, and pathname
/// - `state: Failed`
/// - A single error diagnostic describing the problem
/// - Empty parameters, exports, classes, and applications
fn make_failed_node(node: &Node, message: String, code: &str) -> Node {
    let mut builder = Node::new(node.name().to_string())
        .environment(node.environment().clone())
        .state(EntityState::Failed)
        .add_diagnostic(
            Diagnostic::error(message)
                .with_code(code)
                .with_subject(node.name().to_string()),
        );
    if let Some(uri) = node.uri() {
        builder = builder.uri(uri.to_string());
    }
    if let Some(pathname) = node.pathname() {
        builder = builder.pathname(pathname.to_string());
    }
    builder.build()
}

/// Create a Failed class with minimal data and a diagnostic.
fn make_failed_class(class: &Class, message: String, code: &str) -> Class {
    let mut builder = Class::new(class.name().to_string())
        .environment(class.environment().clone())
        .state(EntityState::Failed)
        .add_diagnostic(
            Diagnostic::error(message)
                .with_code(code)
                .with_subject(class.name().to_string()),
        );
    if let Some(uri) = class.uri() {
        builder = builder.uri(uri.to_string());
    }
    builder.build()
}

#[derive(Debug, Clone)]
struct MergeAccumulator {
    classes: Vec<String>,
    applications: Applications,
    environment: Environment,
    parameters: ParametersType,
    exports: ParametersType,
}

impl MergeAccumulator {
    fn new() -> Self {
        Self {
            classes: Vec::new(),
            applications: Applications::new(),
            environment: Environment::default(),
            parameters: ParametersType::default(),
            exports: ParametersType::default(),
        }
    }
}

fn merge_unique_classes(into: &mut Vec<String>, from: &[String]) {
    for c in from {
        if !into.contains(c) {
            into.push(c.clone());
        }
    }
}

fn merge_environments(parent: &Environment, child: &Environment) -> Environment {
    if !child.is_empty() {
        child.clone()
    } else if !parent.is_empty() {
        parent.clone()
    } else {
        Environment::default()
    }
}

fn merge_parameters(
    parent: &ParametersType,
    child: &ParametersType,
    config: &MergeConfig,
) -> Result<ParametersType, Error> {
    crate::inventory::value_merge::merge_hash_direct(parent, child, config)
        .map_err(|source| Error::ValueMerge { source })
}

fn merge_descent_into(
    acc: &mut MergeAccumulator,
    descent: &MergeAccumulator,
    class_name: &str,
    config: &MergeConfig,
) -> Result<(), Error> {
    merge_unique_classes(&mut acc.classes, &descent.classes);
    if !acc.classes.contains(&class_name.to_string()) {
        acc.classes.push(class_name.to_string());
    }
    acc.environment = merge_environments(&acc.environment, &descent.environment);
    acc.applications.merge_unique(&descent.applications);
    acc.parameters = merge_parameters(&acc.parameters, &descent.parameters, config)?;
    acc.exports = merge_parameters(&acc.exports, &descent.exports, config)?;
    Ok(())
}

fn merge_entity_fields(
    acc: &mut MergeAccumulator,
    classes: &ClassList,
    applications: &Applications,
    environment: &Environment,
    parameters: &ParametersType,
    exports: &ParametersType,
    config: &MergeConfig,
) -> Result<(), Error> {
    merge_unique_classes(&mut acc.classes, classes);
    acc.environment = merge_environments(&acc.environment, environment);
    acc.applications.merge_unique(applications);
    acc.parameters = merge_parameters(&acc.parameters, parameters, config)?;
    acc.exports = merge_parameters(&acc.exports, exports, config)?;
    Ok(())
}

fn interpolate_class_name(
    class_name: &str,
    parameters: &ParametersType,
    config: &MergeConfig,
) -> Result<String, Error> {
    let mut value = Value::String(class_name.to_string());
    value.detect_references();
    interpolation::interpolate(&mut value, parameters, config).map_err(|source| {
        Error::ClassNameResolveError {
            class_name: class_name.to_string(),
            source,
        }
    })?;
    Ok(value.value_to_string())
}

fn resolve_class_name(
    class_name: &str,
    entity_name: &str,
    parameters: &ParametersType,
    config: &MergeConfig,
) -> Result<String, Error> {
    let resolved = crate::inventory::elements::inheritance_chain::resolve_relative_class_name(
        class_name,
        entity_name,
    );
    if contains_interpolation(&resolved) {
        interpolate_class_name(&resolved, parameters, config)
    } else {
        Ok(resolved)
    }
}

fn recurse_class(
    inventory: &Inventory,
    class: &Class,
    merge_base: Option<MergeAccumulator>,
    seen: &mut HashSet<String>,
    merge_config: &MergeConfig,
) -> Result<MergeAccumulator, Error> {
    let mut acc = merge_base.unwrap_or_else(MergeAccumulator::new);

    for class_name in class.classes() {
        let resolved = resolve_class_name(class_name, class.name(), &acc.parameters, merge_config)?;

        if seen.contains(&resolved) {
            continue;
        }

        let parent_class = match inventory.get_class(&resolved) {
            Some(c) => c,
            None => {
                if merge_config.should_ignore_class(&resolved) {
                    if merge_config.ignore_class_notfound_warning {
                        tracing::warn!("Class not found: '{}'. Skipped!", resolved);
                    }
                    continue;
                }
                return Err(Error::ClassNotFound {
                    class_name: resolved.clone(),
                });
            }
        };

        let descent = recurse_class(inventory, parent_class, None, seen, merge_config)?;

        merge_descent_into(&mut acc, &descent, &resolved, merge_config)?;
        seen.insert(resolved);
    }

    merge_entity_fields(
        &mut acc,
        class.classes(),
        class.applications(),
        class.environment(),
        class.parameters(),
        class.exports(),
        merge_config,
    )?;

    Ok(acc)
}

/// Convert a merge::Error into a descriptive message and diagnostic code.
fn error_to_parts(error: &Error) -> (String, &'static str) {
    match error {
        Error::ClassNotFound { class_name } => {
            (format!("class '{}' not found", class_name), "INV-001")
        }
        Error::ClassNameResolveError { class_name, .. } => (
            format!("class name '{}' could not be resolved", class_name),
            "INV-002",
        ),
        Error::Interpolation { .. } => (format_error_chain(error), "REF-001"),
        Error::ValueMerge { .. } => (format_error_chain(error), "MERGE-001"),
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

pub(crate) fn merge_node(
    inventory: &Inventory,
    node: &Node,
    extra_classes: &[String],
    merge_config: &MergeConfig,
    input_data: Option<&ParametersType>,
) -> Result<Node, Error> {
    merge_node_impl(
        inventory,
        node,
        extra_classes,
        merge_config,
        None,
        input_data,
    )
}

pub(crate) fn merge_node_with_inventory(
    inventory: &Inventory,
    node: &Node,
    extra_classes: &[String],
    merge_config: &MergeConfig,
    inv_map: &inv_query::InventoryMap,
    input_data: Option<&ParametersType>,
) -> Result<Node, Error> {
    merge_node_impl(
        inventory,
        node,
        extra_classes,
        merge_config,
        Some(inv_map),
        input_data,
    )
}

fn merge_node_impl(
    inventory: &Inventory,
    node: &Node,
    extra_classes: &[String],
    merge_config: &MergeConfig,
    inv_map: Option<&inv_query::InventoryMap>,
    input_data: Option<&ParametersType>,
) -> Result<Node, Error> {
    match merge_node_inner(
        inventory,
        node,
        extra_classes,
        merge_config,
        inv_map,
        input_data,
    ) {
        Ok(result) => Ok(result),
        Err(error) => {
            // Domain errors produce a Failed node with diagnostics.
            // Implementation errors (I/O) are propagated as Err.
            let (message, code) = error_to_parts(&error);
            Ok(make_failed_node(node, message, code))
        }
    }
}

fn merge_node_inner(
    inventory: &Inventory,
    node: &Node,
    extra_classes: &[String],
    merge_config: &MergeConfig,
    inv_map: Option<&inv_query::InventoryMap>,
    input_data: Option<&ParametersType>,
) -> Result<Node, Error> {
    let environment = node.environment().clone();

    let mut seen: HashSet<String> = HashSet::new();
    let mut base_acc = MergeAccumulator::new();

    for class_name in extra_classes {
        let resolved =
            resolve_class_name(class_name, node.name(), &base_acc.parameters, merge_config)?;
        if seen.contains(&resolved) {
            continue;
        }
        let class = match inventory.get_class(&resolved) {
            Some(c) => c,
            None => {
                if merge_config.should_ignore_class(&resolved) {
                    if merge_config.ignore_class_notfound_warning {
                        tracing::warn!("Class not found: '{}'. Skipped!", resolved);
                    }
                    continue;
                }
                return Err(Error::ClassNotFound {
                    class_name: resolved.clone(),
                });
            }
        };
        let descent = recurse_class(inventory, class, None, &mut seen, merge_config)?;

        merge_descent_into(&mut base_acc, &descent, &resolved, merge_config)?;
        seen.insert(resolved);
    }

    if let Some(data) = input_data {
        let merged = merge_parameters(&base_acc.parameters, data, merge_config)?;
        base_acc.parameters = merged;
    }

    if merge_config.automatic_parameters {
        let auto_params = crate::inventory::create_automatic_parameters(node.name(), &environment);
        let merged = merge_parameters(&base_acc.parameters, &auto_params, merge_config)?;
        base_acc.parameters = merged;
    }

    let mut seen_phase2 = seen;
    let mut node_acc = base_acc;

    for class_name in node.classes() {
        let resolved =
            resolve_class_name(class_name, node.name(), &node_acc.parameters, merge_config)?;

        if seen_phase2.contains(&resolved) {
            continue;
        }

        let class = match inventory.get_class(&resolved) {
            Some(c) => c,
            None => {
                if merge_config.should_ignore_class(&resolved) {
                    if merge_config.ignore_class_notfound_warning {
                        tracing::warn!("Class not found: '{}'. Skipped!", resolved);
                    }
                    continue;
                }
                return Err(Error::ClassNotFound {
                    class_name: resolved.clone(),
                });
            }
        };

        let descent = recurse_class(inventory, class, None, &mut seen_phase2, merge_config)?;

        merge_descent_into(&mut node_acc, &descent, &resolved, merge_config)?;
        seen_phase2.insert(resolved);
    }

    merge_entity_fields(
        &mut node_acc,
        node.classes(),
        node.applications(),
        node.environment(),
        node.parameters(),
        node.exports(),
        merge_config,
    )?;

    let mut result = node.clone();
    if !node_acc.environment.is_empty() {
        *result.environment_mut() = node_acc.environment.clone();
    }
    *result.classes_mut() = node_acc.classes;
    *result.applications_mut() = node_acc.applications;
    *result.parameters_mut() = node_acc.parameters;

    let params = std::mem::take(result.parameters_mut());
    let params_rc = Arc::new(params);
    let mut params_value = Value::Hash(Arc::clone(&params_rc));
    let params = &*params_rc;

    let params_result = if let Some(inv) = inv_map {
        interpolation::interpolate_with_inventory(
            &mut params_value,
            params,
            merge_config,
            inv,
            result.environment(),
        )
    } else {
        interpolation::interpolate(&mut params_value, params, merge_config)
    };

    let exports = std::mem::take(&mut node_acc.exports);
    let mut exports_value = Value::Hash(Arc::new(exports));
    let exports_result = if let Some(inv) = inv_map {
        interpolation::interpolate_with_inventory(
            &mut exports_value,
            params,
            merge_config,
            inv,
            result.environment(),
        )
    } else {
        interpolation::interpolate(&mut exports_value, params, merge_config)
    };

    match (params_result, exports_result) {
        (Ok(()), Ok(())) => {}
        (Err(e), Ok(())) => {
            return Err(Error::Interpolation { source: e });
        }
        (Ok(()), Err(e)) => {
            return Err(Error::Interpolation { source: e });
        }
        (Err(params_err), Err(exports_err)) => match (params_err, exports_err) {
            (
                interpolation::Error::ResolveErrorList {
                    errors: mut combined,
                },
                interpolation::Error::ResolveErrorList { errors: el },
            ) => {
                combined.merge(el);
                return Err(Error::Interpolation {
                    source: interpolation::Error::ResolveErrorList { errors: combined },
                });
            }
            (params_err, _) => {
                return Err(Error::Interpolation { source: params_err });
            }
        },
    }

    match params_value {
        Value::Hash(h) => {
            *result.parameters_mut() = Arc::try_unwrap(h).unwrap_or_else(|rc| (*rc).clone());
        }
        _ => unreachable!("interpolation of Hash should return Hash"),
    }

    match exports_value {
        Value::Hash(h) => {
            *result.exports_mut() = Arc::try_unwrap(h).unwrap_or_else(|rc| (*rc).clone());
        }
        _ => unreachable!("interpolation of Hash should return Hash"),
    }

    Ok(result)
}

pub(crate) fn merge_class(
    inventory: &Inventory,
    class: &Class,
    merge_config: &MergeConfig,
) -> Result<Class, Error> {
    match merge_class_inner(inventory, class, merge_config) {
        Ok(result) => Ok(result),
        Err(error) => {
            let (message, code) = error_to_parts(&error);
            Ok(make_failed_class(class, message, code))
        }
    }
}

fn merge_class_inner(
    inventory: &Inventory,
    class: &Class,
    merge_config: &MergeConfig,
) -> Result<Class, Error> {
    let mut seen: HashSet<String> = HashSet::new();

    let acc = recurse_class(inventory, class, None, &mut seen, merge_config)?;

    let mut result = Class::new(class.name().to_string())
        .environment(acc.environment)
        .classes(acc.classes)
        .applications(acc.applications)
        .parameters(acc.parameters)
        .exports(acc.exports)
        .build();

    if let Some(uri) = class.uri() {
        result.set_uri(uri.to_string());
    }

    Ok(result)
}
