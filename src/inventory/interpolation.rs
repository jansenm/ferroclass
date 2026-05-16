// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::inv_query::InventoryMap;
use crate::inventory::options::MergeConfig;
use crate::inventory::types::Environment;
use crate::inventory::value::{Hash, Key, ReferencePathSegment, Value};
use crate::inventory::value_merge::merge as value_merge;
use snafu::Snafu;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Circular reference detected: {path}"))]
    CircularReference { path: String },
    #[snafu(display("Reference not found: {path}"))]
    ReferenceNotFound { path: String },
    #[snafu(display("Attempt to change constant parameter at {path}"))]
    ChangedConstantParameter { path: String },
    #[snafu(display("Type merge error"))]
    TypeMerge {
        source: crate::inventory::value_merge::Error,
    },
    #[snafu(display("Multiple resolve errors:\n{errors}"))]
    ResolveErrorList { errors: ResolveErrorList },
}

#[derive(Debug)]
pub struct ResolveErrorList {
    errors: Vec<Error>,
}

impl Default for ResolveErrorList {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolveErrorList {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn add(&mut self, error: Error) {
        self.errors.push(error);
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }

    pub fn merge(&mut self, other: ResolveErrorList) {
        self.errors.extend(other.errors);
    }

    pub fn into_result(self) -> Result<(), Error> {
        if self.errors.is_empty() {
            Ok(())
        } else if self.errors.len() == 1 {
            Err(self.errors.into_iter().next().unwrap())
        } else {
            Err(Error::ResolveErrorList { errors: self })
        }
    }
}

impl fmt::Display for ResolveErrorList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, error) in self.errors.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "  {error}")?;
        }
        Ok(())
    }
}

enum InterpolationContext<'a> {
    NoInventory,
    WithInventory {
        inventory: &'a InventoryMap,
        own_environment: &'a Environment,
    },
}

pub fn interpolate(
    value: &mut Value,
    parameters: &Hash,
    config: &MergeConfig,
) -> Result<(), Error> {
    let mut stack: Vec<String> = Vec::new();
    let mut collector = ResolveErrorList::new();
    let context = InterpolationContext::NoInventory;
    interpolate_value_inner(
        value,
        parameters,
        config,
        &mut stack,
        if config.group_errors {
            Some(&mut collector)
        } else {
            None
        },
        &context,
    )?;
    collector.into_result()
}

pub fn interpolate_with_inventory(
    value: &mut Value,
    parameters: &Hash,
    config: &MergeConfig,
    inventory: &InventoryMap,
    own_environment: &Environment,
) -> Result<(), Error> {
    let mut stack: Vec<String> = Vec::new();
    let mut collector = ResolveErrorList::new();
    let context = InterpolationContext::WithInventory {
        inventory,
        own_environment,
    };
    interpolate_value_inner(
        value,
        parameters,
        config,
        &mut stack,
        if config.group_errors {
            Some(&mut collector)
        } else {
            None
        },
        &context,
    )?;
    collector.into_result()
}

fn interpolate_value_inner(
    value: &mut Value,
    parameters: &Hash,
    config: &MergeConfig,
    stack: &mut Vec<String>,
    mut collector: Option<&mut ResolveErrorList>,
    context: &InterpolationContext<'_>,
) -> Result<(), Error> {
    match value {
        Value::Reference(_) => {
            let segments = match std::mem::replace(value, Value::Null) {
                Value::Reference(s) => s,
                _ => unreachable!(),
            };
            match resolve_path_segments(
                &segments,
                parameters,
                stack,
                config,
                collector.as_deref_mut(),
            )? {
                Some(path) => {
                    if stack.contains(&path) {
                        let cycle = stack
                            .iter()
                            .skip_while(|p| **p != path)
                            .cloned()
                            .chain(std::iter::once(path.clone()))
                            .collect::<Vec<_>>()
                            .join(" -> ");
                        return Err(Error::CircularReference { path: cycle });
                    }
                    match lookup_path(&path, parameters) {
                        Some(resolved) => {
                            let mut resolved = resolved.clone();
                            stack.push(path);
                            interpolate_value_inner(
                                &mut resolved,
                                parameters,
                                config,
                                stack,
                                collector.as_deref_mut(),
                                context,
                            )?;
                            stack.pop();
                            *value = resolved;
                        }
                        None => {
                            handle_ref_not_found(path, value, collector.as_deref_mut())?;
                        }
                    }
                }
                None => {
                    *value = Value::Null;
                }
            }
        }
        Value::StringWithReference(_) => {
            let parts = match std::mem::replace(value, Value::Null) {
                Value::StringWithReference(p) => p,
                _ => unreachable!(),
            };
            let mut result = String::new();
            for part in parts.iter() {
                match part {
                    crate::inventory::value::ReferencePart::Literal(lit) => result.push_str(lit),
                    crate::inventory::value::ReferencePart::Reference(segments) => {
                        match resolve_path_segments(
                            segments,
                            parameters,
                            stack,
                            config,
                            collector.as_deref_mut(),
                        )? {
                            Some(path) => {
                                if stack.contains(&path) {
                                    let cycle = stack
                                        .iter()
                                        .skip_while(|p| **p != path)
                                        .cloned()
                                        .chain(std::iter::once(path.clone()))
                                        .collect::<Vec<_>>()
                                        .join(" -> ");
                                    return Err(Error::CircularReference { path: cycle });
                                }
                                match lookup_path(&path, parameters) {
                                    Some(resolved) => {
                                        let mut resolved = resolved.clone();
                                        stack.push(path.clone());
                                        interpolate_value_inner(
                                            &mut resolved,
                                            parameters,
                                            config,
                                            stack,
                                            collector.as_deref_mut(),
                                            context,
                                        )?;
                                        stack.pop();
                                        match &resolved {
                                            Value::String(s) => result.push_str(s),
                                            Value::Integer(i) => result.push_str(&i.to_string()),
                                            Value::Boolean(b) => result.push_str(&b.to_string()),
                                            Value::Real(s) => result.push_str(s),
                                            Value::Null => {}
                                            other => result.push_str(&format!("{:?}", other)),
                                        }
                                    }
                                    None => {
                                        let path_clone = path.clone();
                                        if let Some(c) = &mut collector {
                                            c.add(Error::ReferenceNotFound { path: path_clone });
                                        } else {
                                            return Err(Error::ReferenceNotFound {
                                                path: path_clone,
                                            });
                                        }
                                    }
                                }
                            }
                            None => {
                                // inner path resolution failed, already collected
                            }
                        }
                    }
                }
            }
            *value = Value::String(result);
        }
        Value::InvQuery(data) => match context {
            InterpolationContext::NoInventory => {}
            InterpolationContext::WithInventory {
                inventory,
                own_environment,
            } => {
                let result = data.evaluate(parameters, inventory, own_environment);
                *value = result;
            }
        },
        Value::StringWithInvQuery(parts) => match context {
            InterpolationContext::NoInventory => {}
            InterpolationContext::WithInventory {
                inventory,
                own_environment,
            } => {
                let mut result = String::new();
                for part in parts.iter() {
                    match part {
                        crate::inventory::value::QueryPart::Literal(lit) => result.push_str(lit),
                        crate::inventory::value::QueryPart::InvQuery(data) => {
                            let evaluated = data.evaluate(parameters, inventory, own_environment);
                            match &evaluated {
                                Value::String(s) => result.push_str(s),
                                Value::Integer(i) => result.push_str(&i.to_string()),
                                Value::Boolean(b) => result.push_str(&b.to_string()),
                                Value::Real(s) => result.push_str(s),
                                Value::Null => {}
                                Value::Hash(h) => {
                                    if h.is_empty() {
                                        continue;
                                    }
                                    result.push_str(&format!("{:?}", evaluated));
                                }
                                other => result.push_str(&format!("{:?}", other)),
                            }
                        }
                    }
                }
                *value = Value::String(result);
            }
        },
        Value::DeferredMerge(_) => {
            interpolate_deferred_merge(value, parameters, config, stack, collector, context)?;
        }
        Value::Array(_) => {
            interpolate_array(value, parameters, config, stack, collector, context)?;
        }
        Value::Hash(_) => {
            interpolate_hash(value, parameters, config, stack, collector, context)?;
        }
        Value::OverrideMarker(_) => {
            let inner = match std::mem::replace(value, Value::Null) {
                Value::OverrideMarker(rc) => rc,
                _ => unreachable!(),
            };
            let mut resolved = Rc::try_unwrap(inner).unwrap_or_else(|rc| (*rc).clone());
            interpolate_value_inner(
                &mut resolved,
                parameters,
                config,
                stack,
                collector.as_deref_mut(),
                context,
            )?;
            *value = resolved;
        }
        Value::ConstantMarker(_) => {
            let inner = match std::mem::replace(value, Value::Null) {
                Value::ConstantMarker(rc) => rc,
                _ => unreachable!(),
            };
            let mut resolved = Rc::try_unwrap(inner).unwrap_or_else(|rc| (*rc).clone());
            interpolate_value_inner(&mut resolved, parameters, config, stack, collector, context)?;
            *value = resolved;
        }
        _ => {}
    }
    Ok(())
}

fn interpolate_deferred_merge(
    value: &mut Value,
    parameters: &Hash,
    config: &MergeConfig,
    stack: &mut Vec<String>,
    mut collector: Option<&mut ResolveErrorList>,
    context: &InterpolationContext<'_>,
) -> Result<(), Error> {
    let inner = match value {
        Value::DeferredMerge(values) => Rc::make_mut(values),
        _ => unreachable!("interpolate_deferred_merge called on non-DeferredMerge"),
    };
    let mut output: Option<Value> = None;
    let mut constant = false;
    let mut suppressed_error: Option<Error> = None;
    let total = inner.len();
    for (i, item) in inner.iter().enumerate() {
        let is_last = i == total - 1;
        let output_is_container = matches!(output, Some(Value::Hash(_)) | Some(Value::Array(_)));
        match item {
            Value::OverrideMarker(ov) => {
                let mut resolved = (**ov).clone();
                let item_result = interpolate_value_inner(
                    &mut resolved,
                    parameters,
                    config,
                    stack,
                    None,
                    context,
                );
                match item_result {
                    Ok(()) => {
                        output = Some(resolved);
                    }
                    Err(e) => try_suppress_ref_not_found(
                        e,
                        is_last,
                        output_is_container,
                        config,
                        collector.as_deref_mut(),
                        &mut suppressed_error,
                    )?,
                }
            }
            Value::ConstantMarker(cv) => {
                if constant {
                    if config.strict_constant_parameters {
                        let path_str = stack.join(".");
                        return Err(Error::ChangedConstantParameter { path: path_str });
                    }
                    continue;
                }
                let mut resolved = (**cv).clone();
                let item_result = interpolate_value_inner(
                    &mut resolved,
                    parameters,
                    config,
                    stack,
                    None,
                    context,
                );
                match item_result {
                    Ok(()) => {
                        output = Some(resolved);
                        constant = true;
                    }
                    Err(e) => try_suppress_ref_not_found(
                        e,
                        is_last,
                        output_is_container,
                        config,
                        collector.as_deref_mut(),
                        &mut suppressed_error,
                    )?,
                }
            }
            _ => {
                if constant {
                    if config.strict_constant_parameters {
                        let path_str = stack.join(".");
                        return Err(Error::ChangedConstantParameter { path: path_str });
                    }
                    continue;
                }
                let mut resolved = item.clone();
                let item_result = interpolate_value_inner(
                    &mut resolved,
                    parameters,
                    config,
                    stack,
                    None,
                    context,
                );
                match item_result {
                    Ok(()) => match output {
                        None => {
                            output = Some(resolved);
                        }
                        Some(ref acc) => {
                            output = Some(
                                value_merge(acc, &resolved, config, stack)
                                    .map_err(|source| Error::TypeMerge { source })?,
                            );
                        }
                    },
                    Err(e) => try_suppress_ref_not_found(
                        e,
                        is_last,
                        output_is_container,
                        config,
                        collector.as_deref_mut(),
                        &mut suppressed_error,
                    )?,
                }
            }
        }
    }
    if matches!(output, Some(Value::Hash(_)) | Some(Value::Array(_)))
        && let Some(err) = suppressed_error
    {
        return Err(err);
    }
    *value = output.unwrap_or(Value::Null);
    Ok(())
}

fn interpolate_array(
    value: &mut Value,
    parameters: &Hash,
    config: &MergeConfig,
    stack: &mut Vec<String>,
    mut collector: Option<&mut ResolveErrorList>,
    context: &InterpolationContext<'_>,
) -> Result<(), Error> {
    let arr = match value {
        Value::Array(arr) => Rc::make_mut(arr),
        _ => unreachable!("interpolate_array called on non-Array"),
    };
    for item in arr.iter_mut() {
        interpolate_value_inner(
            item,
            parameters,
            config,
            stack,
            collector.as_deref_mut(),
            context,
        )?;
    }
    Ok(())
}

fn interpolate_hash(
    value: &mut Value,
    parameters: &Hash,
    config: &MergeConfig,
    stack: &mut Vec<String>,
    mut collector: Option<&mut ResolveErrorList>,
    context: &InterpolationContext<'_>,
) -> Result<(), Error> {
    let hash = match value {
        Value::Hash(hash) => Rc::make_mut(hash),
        _ => unreachable!("interpolate_hash called on non-Hash"),
    };
    let mut keys_to_remove: Vec<Key> = Vec::new();
    for (k, v) in hash.iter_mut() {
        let ignore = match context {
            InterpolationContext::NoInventory => false,
            InterpolationContext::WithInventory { .. } => {
                v.has_inv_query()
                    && (v.ignore_failed_render() || config.inventory_ignore_failed_render)
            }
        };
        if ignore {
            match interpolate_value_inner(v, parameters, config, stack, None, context) {
                Ok(()) => {}
                Err(_) => {
                    keys_to_remove.push(k.clone());
                }
            }
        } else {
            interpolate_value_inner(
                v,
                parameters,
                config,
                stack,
                collector.as_deref_mut(),
                context,
            )?;
        }
    }
    for k in keys_to_remove {
        hash.remove(&k);
    }
    Ok(())
}

fn resolve_path_segments(
    segments: &[ReferencePathSegment],
    parameters: &Hash,
    stack: &mut Vec<String>,
    config: &MergeConfig,
    mut collector: Option<&mut ResolveErrorList>,
) -> Result<Option<String>, Error> {
    let mut parts: Vec<String> = Vec::new();
    for segment in segments {
        match segment {
            ReferencePathSegment::Literal(s) => parts.push(s.clone()),
            ReferencePathSegment::Inner(inner) => {
                let key = format_segments_for_cycle(inner);
                if stack.contains(&key) {
                    let cycle = stack
                        .iter()
                        .skip_while(|p| **p == key)
                        .cloned()
                        .chain(std::iter::once(key.clone()))
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    return Err(Error::CircularReference { path: cycle });
                }
                match resolve_path_segments_grouped_or_not(
                    inner,
                    parameters,
                    stack,
                    config,
                    collector.as_deref_mut(),
                )? {
                    Some(inner_path) => match lookup_path(&inner_path, parameters) {
                        Some(resolved) => {
                            let mut resolved = resolved.clone();
                            stack.push(key);
                            interpolate_value_inner(
                                &mut resolved,
                                parameters,
                                config,
                                stack,
                                collector.as_deref_mut(),
                                &InterpolationContext::NoInventory,
                            )?;
                            stack.pop();
                            let resolved_str = match &resolved {
                                Value::String(s) => s.clone(),
                                Value::Integer(i) => i.to_string(),
                                Value::Boolean(b) => b.to_string(),
                                Value::Real(s) => s.clone(),
                                other => format!("{:?}", other),
                            };
                            parts.push(resolved_str);
                        }
                        None => {
                            return if let Some(c) = collector {
                                c.add(Error::ReferenceNotFound {
                                    path: inner_path.clone(),
                                });
                                Ok(None)
                            } else {
                                Err(Error::ReferenceNotFound {
                                    path: inner_path.clone(),
                                })
                            };
                        }
                    },
                    None => {
                        return Ok(None);
                    }
                }
            }
        }
    }
    Ok(Some(parts.join(":")))
}

fn resolve_path_segments_grouped_or_not(
    segments: &[ReferencePathSegment],
    parameters: &Hash,
    stack: &mut Vec<String>,
    config: &MergeConfig,
    collector: Option<&mut ResolveErrorList>,
) -> Result<Option<String>, Error> {
    resolve_path_segments(segments, parameters, stack, config, collector)
}

fn format_segments_for_cycle(segments: &[ReferencePathSegment]) -> String {
    let mut s = String::new();
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            s.push(':');
        }
        match seg {
            ReferencePathSegment::Literal(lit) => s.push_str(lit),
            ReferencePathSegment::Inner(inner) => {
                s.push_str("${");
                s.push_str(&format_segments_for_cycle(inner));
                s.push('}');
            }
        }
    }
    s
}

fn handle_ref_not_found(
    path: String,
    value: &mut Value,
    collector: Option<&mut ResolveErrorList>,
) -> Result<(), Error> {
    if let Some(c) = collector {
        c.add(Error::ReferenceNotFound { path });
        *value = Value::Null;
        Ok(())
    } else {
        Err(Error::ReferenceNotFound { path })
    }
}

fn try_suppress_ref_not_found(
    error: Error,
    is_last: bool,
    output_is_container: bool,
    config: &MergeConfig,
    collector: Option<&mut ResolveErrorList>,
    suppressed_error: &mut Option<Error>,
) -> Result<(), Error> {
    match &error {
        Error::ReferenceNotFound { path } => {
            if config.ignore_overwritten_missing_references && !is_last && !output_is_container {
                tracing::warn!(
                    "Reference '{}' undefined (overwritten by later class)",
                    path
                );
                *suppressed_error = Some(error);
                Ok(())
            } else if let Some(c) = collector {
                c.add(error);
                Ok(())
            } else {
                Err(error)
            }
        }
        Error::ResolveErrorList { .. } => {
            if config.ignore_overwritten_missing_references && !is_last && !output_is_container {
                if let Error::ResolveErrorList { ref errors } = error {
                    for err in &errors.errors {
                        if let Error::ReferenceNotFound { path } = err {
                            tracing::warn!(
                                "Reference '{}' undefined (overwritten by later class)",
                                path
                            );
                        }
                    }
                }
                *suppressed_error = Some(error);
                Ok(())
            } else if let Some(c) = collector {
                if let Error::ResolveErrorList { errors } = error {
                    c.merge(errors);
                }
                Ok(())
            } else {
                Err(error)
            }
        }
        _ => Err(error),
    }
}

fn lookup_path<'a>(path: &str, parameters: &'a Hash) -> Option<&'a Value> {
    let keys: Vec<&str> = path.split(':').collect();
    let first = Key::String(keys[0].to_string());
    let mut current = parameters.get(&first)?;

    for key_str in keys.iter().skip(1) {
        match current {
            Value::Hash(h) => {
                current = h.get(&Key::String(key_str.to_string()))?;
            }
            _ => return None,
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::value::ReferencePart;
    use crate::inventory::value::ReferencePathSegment;
    use hashlink::LinkedHashMap;

    fn default_config() -> MergeConfig {
        MergeConfig::default()
    }

    fn make_params(items: Vec<(&str, Value)>) -> Hash {
        let mut hash: Hash = LinkedHashMap::new();
        for (k, v) in items {
            hash.insert(Key::String(k.to_string()), v);
        }
        hash
    }

    #[test]
    fn test_interpolate_simple_reference() {
        let params = make_params(vec![("name", Value::String("myserver".to_string()))]);
        let mut v = Value::Reference(vec![ReferencePathSegment::Literal("name".to_string())]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::String("myserver".to_string()));
    }

    #[test]
    fn test_interpolate_nested_reference_path() {
        let inner = make_params(vec![("ip", Value::String("127.0.0.1".to_string()))]);
        let params = make_params(vec![("host", Value::Hash(Rc::new(inner)))]);
        let mut v = Value::Reference(vec![
            ReferencePathSegment::Literal("host".to_string()),
            ReferencePathSegment::Literal("ip".to_string()),
        ]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::String("127.0.0.1".to_string()));
    }

    #[test]
    fn test_interpolate_inner_reference() {
        let params = make_params(vec![
            ("alpha_two", Value::String("a".to_string())),
            (
                "beta",
                Value::Hash(Rc::new(make_params(vec![("a", Value::Integer(99))]))),
            ),
        ]);
        let mut v = Value::Reference(vec![
            ReferencePathSegment::Literal("beta".to_string()),
            ReferencePathSegment::Inner(vec![ReferencePathSegment::Literal(
                "alpha_two".to_string(),
            )]),
        ]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::Integer(99));
    }

    #[test]
    fn test_interpolate_string_with_reference() {
        let params = make_params(vec![
            ("name", Value::String("myserver".to_string())),
            ("ip", Value::String("127.0.0.1".to_string())),
        ]);
        let mut v = Value::StringWithReference(vec![
            ReferencePart::Literal("Welcome to ".to_string()),
            ReferencePart::Reference(vec![ReferencePathSegment::Literal("name".to_string())]),
            ReferencePart::Literal(" ".to_string()),
            ReferencePart::Reference(vec![ReferencePathSegment::Literal("ip".to_string())]),
        ]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(
            v,
            Value::String("Welcome to myserver 127.0.0.1".to_string())
        );
    }

    #[test]
    fn test_interpolate_reference_preserves_type() {
        let list = Value::Array(Rc::new(vec![Value::Integer(1), Value::Integer(2)]));
        let params = make_params(vec![("mylist", list.clone())]);
        let mut v = Value::Reference(vec![ReferencePathSegment::Literal("mylist".to_string())]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, list);
    }

    #[test]
    fn test_interpolate_deferred_merge() {
        let one = make_params(vec![("a", Value::Integer(1)), ("b", Value::Integer(2))]);
        let two = make_params(vec![("c", Value::Integer(3)), ("d", Value::Integer(4))]);
        let node_params = make_params(vec![
            ("one", Value::Hash(Rc::new(one))),
            ("two", Value::Hash(Rc::new(two))),
            ("e", Value::Integer(5)),
        ]);
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::Reference(vec![ReferencePathSegment::Literal("one".to_string())]),
            Value::Reference(vec![ReferencePathSegment::Literal("two".to_string())]),
            Value::Hash(Rc::new(make_params(vec![("e", Value::Integer(5))]))),
        ]));
        interpolate(&mut v, &node_params, &default_config()).unwrap();
        match &v {
            Value::Hash(h) => {
                assert_eq!(
                    h.get(&Key::String("a".to_string())),
                    Some(&Value::Integer(1))
                );
                assert_eq!(
                    h.get(&Key::String("b".to_string())),
                    Some(&Value::Integer(2))
                );
                assert_eq!(
                    h.get(&Key::String("c".to_string())),
                    Some(&Value::Integer(3))
                );
                assert_eq!(
                    h.get(&Key::String("d".to_string())),
                    Some(&Value::Integer(4))
                );
                assert_eq!(
                    h.get(&Key::String("e".to_string())),
                    Some(&Value::Integer(5))
                );
            }
            _ => panic!("Expected Hash, got {:?}", v),
        }
    }

    #[test]
    fn test_interpolate_circular_reference() {
        let params = make_params(vec![
            (
                "a",
                Value::Reference(vec![ReferencePathSegment::Literal("b".to_string())]),
            ),
            (
                "b",
                Value::Reference(vec![ReferencePathSegment::Literal("a".to_string())]),
            ),
        ]);
        let mut v = Value::Reference(vec![ReferencePathSegment::Literal("a".to_string())]);
        let result = interpolate(&mut v, &params, &default_config());
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::CircularReference { path } => {
                assert!(path.contains("a"));
                assert!(path.contains("b"));
            }
            e => panic!("Expected CircularReference, got {:?}", e),
        }
    }

    #[test]
    fn test_interpolate_not_found() {
        let params = make_params(vec![]);
        let mut v = Value::Reference(vec![ReferencePathSegment::Literal("missing".to_string())]);
        let result = interpolate(&mut v, &params, &default_config());
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ReferenceNotFound { path } => {
                assert_eq!(path, "missing");
            }
            e => panic!("Expected ReferenceNotFound, got {:?}", e),
        }
    }

    #[test]
    fn test_detect_references_pure() {
        let mut v = Value::String("${host:name}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::Reference(vec![
                ReferencePathSegment::Literal("host".to_string()),
                ReferencePathSegment::Literal("name".to_string()),
            ])
        );
    }

    #[test]
    fn test_detect_references_nested_inner_ref() {
        let mut v = Value::String("${beta:${alpha:two}}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::Reference(vec![
                ReferencePathSegment::Literal("beta".to_string()),
                ReferencePathSegment::Inner(vec![
                    ReferencePathSegment::Literal("alpha".to_string()),
                    ReferencePathSegment::Literal("two".to_string()),
                ]),
            ])
        );
    }

    #[test]
    fn test_detect_references_mixed() {
        let mut v = Value::String("Hello ${name}".to_string());
        v.detect_references();
        assert_eq!(
            v,
            Value::StringWithReference(vec![
                ReferencePart::Literal("Hello ".to_string()),
                ReferencePart::Reference(vec![ReferencePathSegment::Literal("name".to_string()),]),
            ])
        );
    }

    #[test]
    fn test_detect_references_none() {
        let mut v = Value::String("plain text".to_string());
        v.detect_references();
        assert_eq!(v, Value::String("plain text".to_string()));
    }

    #[test]
    fn test_detect_references_nested_in_hash() {
        let mut params = LinkedHashMap::new();
        params.insert(
            Key::String("motd".to_string()),
            Value::String("Welcome to ${host:name}".to_string()),
        );
        params.insert(
            Key::String("host".to_string()),
            Value::Hash(Rc::new({
                let mut h = LinkedHashMap::new();
                h.insert(
                    Key::String("name".to_string()),
                    Value::String("myserver".to_string()),
                );
                h
            })),
        );
        let mut v = Value::Hash(Rc::new(params));
        v.detect_references();
        match &v {
            Value::Hash(h) => {
                assert!(matches!(
                    h.get(&Key::String("motd".to_string())),
                    Some(Value::StringWithReference(_))
                ));
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_interpolate_full_example() {
        let host_hash = make_params(vec![
            ("name", Value::String("myserver".to_string())),
            ("ip-address", Value::String("127.0.0.1".to_string())),
        ]);
        let params = make_params(vec![
            ("host", Value::Hash(Rc::new(host_hash))),
            (
                "motd",
                Value::StringWithReference(vec![
                    ReferencePart::Literal("Welcome to ".to_string()),
                    ReferencePart::Reference(vec![
                        ReferencePathSegment::Literal("host".to_string()),
                        ReferencePathSegment::Literal("name".to_string()),
                    ]),
                    ReferencePart::Literal(" ".to_string()),
                    ReferencePart::Reference(vec![
                        ReferencePathSegment::Literal("host".to_string()),
                        ReferencePathSegment::Literal("ip-address".to_string()),
                    ]),
                ]),
            ),
        ]);
        let mut value = Value::Hash(Rc::new(params.clone()));
        interpolate(&mut value, &params, &default_config()).unwrap();
        match &value {
            Value::Hash(h) => {
                let motd = h.get(&Key::String("motd".to_string())).unwrap();
                assert_eq!(
                    motd,
                    &Value::String("Welcome to myserver 127.0.0.1".to_string())
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_interpolate_string_with_integer_reference() {
        let params = make_params(vec![("port", Value::Integer(8080))]);
        let mut v = Value::StringWithReference(vec![
            ReferencePart::Literal("Listening on port ".to_string()),
            ReferencePart::Reference(vec![ReferencePathSegment::Literal("port".to_string())]),
        ]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::String("Listening on port 8080".to_string()));
    }

    #[test]
    fn test_interpolate_string_with_boolean_reference() {
        let params = make_params(vec![("enabled", Value::Boolean(true))]);
        let mut v = Value::StringWithReference(vec![
            ReferencePart::Literal("Feature: ".to_string()),
            ReferencePart::Reference(vec![ReferencePathSegment::Literal("enabled".to_string())]),
        ]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::String("Feature: true".to_string()));
    }

    #[test]
    fn test_interpolate_string_with_null_reference() {
        let params = make_params(vec![("name", Value::Null)]);
        let mut v = Value::StringWithReference(vec![
            ReferencePart::Literal("Hello ".to_string()),
            ReferencePart::Reference(vec![ReferencePathSegment::Literal("name".to_string())]),
        ]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::String("Hello ".to_string()));
    }

    #[test]
    fn test_interpolate_nested_reference_example() {
        let alpha = make_params(vec![("two", Value::String("a".to_string()))]);
        let beta = make_params(vec![("a", Value::Integer(99))]);
        let params = make_params(vec![
            ("alpha", Value::Hash(Rc::new(alpha))),
            ("beta", Value::Hash(Rc::new(beta))),
            (
                "one",
                Value::Reference(vec![
                    ReferencePathSegment::Literal("beta".to_string()),
                    ReferencePathSegment::Inner(vec![
                        ReferencePathSegment::Literal("alpha".to_string()),
                        ReferencePathSegment::Literal("two".to_string()),
                    ]),
                ]),
            ),
        ]);
        let mut v = Value::Reference(vec![ReferencePathSegment::Literal("one".to_string())]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::Integer(99));
    }

    #[test]
    fn test_lookup_path_deep() {
        let inner = make_params(vec![("deep", Value::String("found".to_string()))]);
        let middle = make_params(vec![("inner", Value::Hash(Rc::new(inner)))]);
        let params = make_params(vec![("outer", Value::Hash(Rc::new(middle)))]);
        let result = lookup_path("outer:inner:deep", &params);
        assert_eq!(result, Some(&Value::String("found".to_string())));
    }

    #[test]
    fn test_lookup_path_missing_key() {
        let params = make_params(vec![("a", Value::Integer(1))]);
        let result = lookup_path("b", &params);
        assert!(result.is_none());
    }

    #[test]
    fn test_lookup_path_non_hash_intermediate() {
        let params = make_params(vec![("a", Value::Integer(1))]);
        let result = lookup_path("a:b", &params);
        assert!(result.is_none());
    }

    #[test]
    fn test_interpolate_string_with_multiple_colon_refs() {
        let person = make_params(vec![
            ("firstname", Value::String("Jane".to_string())),
            ("lastname", Value::String("Doe".to_string())),
        ]);
        let params = make_params(vec![("person", Value::Hash(Rc::new(person)))]);
        let mut v = Value::StringWithReference(vec![
            ReferencePart::Literal("Hello ".to_string()),
            ReferencePart::Reference(vec![
                ReferencePathSegment::Literal("person".to_string()),
                ReferencePathSegment::Literal("firstname".to_string()),
            ]),
            ReferencePart::Literal(" ".to_string()),
            ReferencePart::Reference(vec![
                ReferencePathSegment::Literal("person".to_string()),
                ReferencePathSegment::Literal("lastname".to_string()),
            ]),
        ]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::String("Hello Jane Doe".to_string()));
    }

    #[test]
    fn test_interpolate_greeting_detect_then_resolve() {
        let person_hash = make_params(vec![
            ("firstname", Value::String("Jane".to_string())),
            ("lastname", Value::String("Doe".to_string())),
        ]);
        let params = make_params(vec![
            ("person", Value::Hash(Rc::new(person_hash))),
            (
                "greeting",
                Value::String("Hello ${person:firstname} ${person:lastname}".to_string()),
            ),
        ]);
        let mut v = Value::Hash(Rc::new(params.clone()));
        v.detect_references();
        interpolate(&mut v, &params, &default_config()).unwrap();
        match &v {
            Value::Hash(h) => {
                let greeting = h.get(&Key::String("greeting".to_string())).unwrap();
                assert_eq!(greeting, &Value::String("Hello Jane Doe".to_string()));
            }
            _ => panic!("Expected Hash"),
        }
    }

    #[test]
    fn test_interpolate_escaped_reference() {
        let params = make_params(vec![("colour", Value::String("Blue".to_string()))]);
        let mut v = Value::StringWithReference(vec![ReferencePart::Literal(
            "The colour is ${colour}".to_string(),
        )]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::String("The colour is ${colour}".to_string()));
    }

    #[test]
    fn test_interpolate_double_escaped_then_reference() {
        let params = make_params(vec![("colour", Value::String("Blue".to_string()))]);
        let mut v = Value::StringWithReference(vec![
            ReferencePart::Literal("The colour is \\".to_string()),
            ReferencePart::Reference(vec![ReferencePathSegment::Literal("colour".to_string())]),
        ]);
        interpolate(&mut v, &params, &default_config()).unwrap();
        assert_eq!(v, Value::String("The colour is \\Blue".to_string()));
    }

    #[test]
    fn test_interpolate_spec_example_end_to_end() {
        let params = make_params(vec![("colour", Value::String("Blue".to_string()))]);

        let mut unescaped = Value::String("The colour is ${colour}".to_string());
        unescaped.detect_references();
        interpolate(&mut unescaped, &params, &default_config()).unwrap();
        assert_eq!(unescaped, Value::String("The colour is Blue".to_string()));

        let mut escaped = Value::String(r"The colour is \${colour}".to_string());
        escaped.detect_references();
        interpolate(&mut escaped, &params, &default_config()).unwrap();
        assert_eq!(
            escaped,
            Value::String("The colour is ${colour}".to_string())
        );

        let mut double_escaped = Value::String(r"The colour is \\${colour}".to_string());
        double_escaped.detect_references();
        interpolate(&mut double_escaped, &params, &default_config()).unwrap();
        assert_eq!(
            double_escaped,
            Value::String(r"The colour is \Blue".to_string())
        );
    }

    #[test]
    fn test_type_merge_error_null_over_dict() {
        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let base_dict = make_params(vec![("key", Value::String("value".to_string()))]);
        let mut v =
            Value::DeferredMerge(Rc::new(vec![Value::Hash(Rc::new(base_dict)), Value::Null]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::TypeMerge { source } => {
                assert!(matches!(
                    source,
                    crate::inventory::value_merge::Error::TypeMerge { .. }
                ));
            }
            e => panic!("Expected TypeMerge, got {:?}", e),
        }
    }

    #[test]
    fn test_type_merge_error_null_over_list() {
        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let base_list = Value::Array(Rc::new(vec![Value::Integer(1)]));
        let mut v = Value::DeferredMerge(Rc::new(vec![base_list, Value::Null]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::TypeMerge { source } => match source {
                crate::inventory::value_merge::Error::TypeMerge { existing_type, .. } => {
                    assert_eq!(existing_type, "list");
                }
            },
            e => panic!("Expected TypeMerge, got {:?}", e),
        }
    }

    #[test]
    fn test_type_merge_error_scalar_over_dict() {
        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let base_dict = make_params(vec![("key", Value::String("value".to_string()))]);
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::Hash(Rc::new(base_dict)),
            Value::String("override".to_string()),
        ]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::TypeMerge { source } => match source {
                crate::inventory::value_merge::Error::TypeMerge { existing_type, .. } => {
                    assert_eq!(existing_type, "dictionary");
                }
            },
            e => panic!("Expected TypeMerge, got {:?}", e),
        }
    }

    #[test]
    fn test_type_merge_error_scalar_over_list() {
        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let base_list = Value::Array(Rc::new(vec![Value::Integer(1)]));
        let mut v = Value::DeferredMerge(Rc::new(vec![
            base_list,
            Value::String("override".to_string()),
        ]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::TypeMerge { source } => match source {
                crate::inventory::value_merge::Error::TypeMerge { existing_type, .. } => {
                    assert_eq!(existing_type, "list");
                }
            },
            e => panic!("Expected TypeMerge, got {:?}", e),
        }
    }

    #[test]
    fn test_type_merge_null_over_dict_with_allow_none() {
        let config = MergeConfig {
            allow_none_override: true,
            ..MergeConfig::default()
        };
        let base_dict = make_params(vec![("key", Value::String("value".to_string()))]);
        let mut v =
            Value::DeferredMerge(Rc::new(vec![Value::Hash(Rc::new(base_dict)), Value::Null]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn test_type_merge_null_over_list_with_allow_none() {
        let config = MergeConfig {
            allow_none_override: true,
            ..MergeConfig::default()
        };
        let base_list = Value::Array(Rc::new(vec![Value::Integer(1)]));
        let mut v = Value::DeferredMerge(Rc::new(vec![base_list, Value::Null]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(v, Value::Null);
    }

    #[test]
    fn test_type_merge_dict_over_dict_ok() {
        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let base_dict = make_params(vec![("a", Value::Integer(1))]);
        let other_dict = make_params(vec![("b", Value::Integer(2))]);
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::Hash(Rc::new(base_dict)),
            Value::Hash(Rc::new(other_dict)),
        ]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
    }

    #[test]
    fn test_type_merge_scalar_over_scalar_ok() {
        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::String("old".to_string()),
            Value::String("new".to_string()),
        ]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(v, Value::String("new".to_string()));
    }

    #[test]
    fn test_type_merge_null_over_scalar_ok() {
        let config = MergeConfig {
            allow_none_override: false,
            ..MergeConfig::default()
        };
        let mut v =
            Value::DeferredMerge(Rc::new(vec![Value::String("old".to_string()), Value::Null]));
        let params = make_params(vec![]);
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_ok(), "Expected Ok, got {:?}", result);
        assert_eq!(v, Value::Null);
    }

    // --- group_errors unit tests ---

    #[test]
    fn test_group_errors_single_ref_missing() {
        let config = MergeConfig {
            group_errors: true,
            ..MergeConfig::default()
        };
        let params = make_params(vec![]);
        let mut v = Value::Hash(Rc::new({
            let mut h = LinkedHashMap::new();
            h.insert(
                Key::String("alpha".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("missing".to_string())]),
            );
            h
        }));
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ReferenceNotFound { path } => {
                assert_eq!(path, "missing");
            }
            other => panic!("Expected ReferenceNotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_group_errors_multiple_refs_missing_grouped() {
        let config = MergeConfig {
            group_errors: true,
            ..MergeConfig::default()
        };
        let params = make_params(vec![]);
        let mut v = Value::Hash(Rc::new({
            let mut h = LinkedHashMap::new();
            h.insert(
                Key::String("alpha".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("ref_a".to_string())]),
            );
            h.insert(
                Key::String("beta".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("ref_b".to_string())]),
            );
            h
        }));
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ResolveErrorList { errors } => {
                assert_eq!(errors.len(), 2, "should collect both errors");
            }
            other => panic!("Expected ResolveErrorList, got {:?}", other),
        }
    }

    #[test]
    fn test_group_errors_multiple_refs_missing_single_error_mode() {
        let config = MergeConfig {
            group_errors: false,
            ..MergeConfig::default()
        };
        let params = make_params(vec![]);
        let mut v = Value::Hash(Rc::new({
            let mut h = LinkedHashMap::new();
            h.insert(
                Key::String("alpha".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("ref_a".to_string())]),
            );
            h.insert(
                Key::String("beta".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("ref_b".to_string())]),
            );
            h
        }));
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ReferenceNotFound { path } => {
                assert!(
                    path == "ref_a" || path == "ref_b",
                    "should be one of the refs, got {}",
                    path
                );
            }
            other => panic!("Expected ReferenceNotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_group_errors_resolved_ref_plus_missing_still_grouped() {
        let config = MergeConfig {
            group_errors: true,
            ..MergeConfig::default()
        };
        let params = make_params(vec![("known", Value::String("hello".to_string()))]);
        let mut v = Value::Hash(Rc::new({
            let mut h = LinkedHashMap::new();
            h.insert(
                Key::String("ok".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("known".to_string())]),
            );
            h.insert(
                Key::String("bad".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("unknown".to_string())]),
            );
            h
        }));
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ReferenceNotFound { path } => {
                assert_eq!(path, "unknown");
            }
            other => panic!("Expected ReferenceNotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_group_errors_circular_reference_stops_immediately() {
        let config_grouped = MergeConfig {
            group_errors: true,
            ..MergeConfig::default()
        };
        let config_single = MergeConfig {
            group_errors: false,
            ..MergeConfig::default()
        };
        let params = make_params(vec![
            (
                "a",
                Value::Reference(vec![ReferencePathSegment::Literal("b".to_string())]),
            ),
            (
                "b",
                Value::Reference(vec![ReferencePathSegment::Literal("a".to_string())]),
            ),
        ]);

        let mut v1 = Value::Hash(Rc::new(params.clone()));
        let result1 = interpolate(&mut v1, &params, &config_grouped);
        assert!(result1.is_err());
        match result1.unwrap_err() {
            Error::CircularReference { .. } => {}
            other => panic!("Expected CircularReference, got {:?}", other),
        }

        let mut v2 = Value::Hash(Rc::new(params.clone()));
        let result2 = interpolate(&mut v2, &params, &config_single);
        assert!(result2.is_err());
        match result2.unwrap_err() {
            Error::CircularReference { .. } => {}
            other => panic!("Expected CircularReference, got {:?}", other),
        }
    }

    #[test]
    fn test_group_errors_resolved_values_preserved() {
        let config = MergeConfig {
            group_errors: true,
            ..MergeConfig::default()
        };
        let params = make_params(vec![("known", Value::String("hello".to_string()))]);
        let mut v = Value::Hash(Rc::new({
            let mut h = LinkedHashMap::new();
            h.insert(
                Key::String("ok".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("known".to_string())]),
            );
            h.insert(
                Key::String("bad".to_string()),
                Value::Reference(vec![ReferencePathSegment::Literal("unknown".to_string())]),
            );
            h
        }));
        let result = interpolate(&mut v, &params, &config);
        assert!(result.is_err());
        match &v {
            Value::Hash(h) => {
                assert_eq!(
                    h.get(&Key::String("ok".to_string())),
                    Some(&Value::String("hello".to_string())),
                    "resolved values should be preserved even when other refs fail"
                );
                assert_eq!(
                    h.get(&Key::String("bad".to_string())),
                    Some(&Value::Null),
                    "unresolved values should be replaced with Null"
                );
            }
            _ => panic!("Expected Hash"),
        }
    }

    // --- ignore_overwritten_missing_references unit tests ---

    #[test]
    fn test_ignore_overwritten_missing_refs_suppresses_non_final() {
        let config = MergeConfig {
            ignore_overwritten_missing_references: true,
            ..MergeConfig::default()
        };
        let params = make_params(vec![("known", Value::String("bar".to_string()))]);
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::Reference(vec![ReferencePathSegment::Literal("missing".to_string())]),
            Value::Reference(vec![ReferencePathSegment::Literal("known".to_string())]),
        ]));
        let result = interpolate(&mut v, &params, &config);
        assert!(
            result.is_ok(),
            "should suppress non-final missing ref: {:?}",
            result
        );
        assert_eq!(v, Value::String("bar".to_string()));
    }

    #[test]
    fn test_ignore_overwritten_missing_refs_raises_final() {
        let config = MergeConfig {
            ignore_overwritten_missing_references: true,
            ..MergeConfig::default()
        };
        let params = make_params(vec![]);
        let mut v = Value::DeferredMerge(Rc::new(vec![Value::Reference(vec![
            ReferencePathSegment::Literal("missing".to_string()),
        ])]));
        let result = interpolate(&mut v, &params, &config);
        assert!(
            result.is_err(),
            "should raise error for final (last) missing ref"
        );
        match result.unwrap_err() {
            Error::ReferenceNotFound { path } => {
                assert_eq!(path, "missing");
            }
            other => panic!("Expected ReferenceNotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_ignore_overwritten_missing_refs_disabled_raises_immediately() {
        let config = MergeConfig {
            ignore_overwritten_missing_references: false,
            ..MergeConfig::default()
        };
        let params = make_params(vec![("known", Value::String("bar".to_string()))]);
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::Reference(vec![ReferencePathSegment::Literal("missing".to_string())]),
            Value::Reference(vec![ReferencePathSegment::Literal("known".to_string())]),
        ]));
        let result = interpolate(&mut v, &params, &config);
        assert!(
            result.is_err(),
            "should raise error immediately when feature disabled"
        );
    }

    #[test]
    fn test_ignore_overwritten_missing_refs_raises_when_output_is_dict() {
        let config = MergeConfig {
            ignore_overwritten_missing_references: true,
            ..MergeConfig::default()
        };
        let inner = make_params(vec![("key", Value::String("val".to_string()))]);
        let params = make_params(vec![]);
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::Reference(vec![ReferencePathSegment::Literal("missing".to_string())]),
            Value::Hash(Rc::new(inner)),
        ]));
        let result = interpolate(&mut v, &params, &config);
        assert!(
            result.is_err(),
            "suppressed error should be re-raised when output becomes dict"
        );
    }

    #[test]
    fn test_ignore_overwritten_missing_refs_scalar_overwrite_ok() {
        let config = MergeConfig {
            ignore_overwritten_missing_references: true,
            ..MergeConfig::default()
        };
        let params = make_params(vec![("known", Value::String("final_value".to_string()))]);
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::Reference(vec![ReferencePathSegment::Literal("missing".to_string())]),
            Value::Reference(vec![ReferencePathSegment::Literal("known".to_string())]),
        ]));
        let result = interpolate(&mut v, &params, &config);
        assert!(
            result.is_ok(),
            "scalar overwrite should suppress error: {:?}",
            result
        );
        assert_eq!(v, Value::String("final_value".to_string()));
    }

    #[test]
    fn test_ignore_overwritten_missing_refs_override_marker_overwrites() {
        let config = MergeConfig {
            ignore_overwritten_missing_references: true,
            ..MergeConfig::default()
        };
        let params = make_params(vec![]);
        let mut v = Value::DeferredMerge(Rc::new(vec![
            Value::Reference(vec![ReferencePathSegment::Literal("missing".to_string())]),
            Value::OverrideMarker(Rc::new(Value::String("override".to_string()))),
        ]));
        let result = interpolate(&mut v, &params, &config);
        assert!(
            result.is_ok(),
            "OverrideMarker should overwrite missing ref: {:?}",
            result
        );
        assert_eq!(v, Value::String("override".to_string()));
    }
}
