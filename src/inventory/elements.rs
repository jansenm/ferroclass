// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Class and node element types.
//!
//! [`Class`] represents a YAML class definition with parameters, exports,
//! applications, and an inheritance chain. [`Node`] represents a node
//! definition with the same fields plus an environment.

pub mod class;
pub use class::Class;

pub mod node;
#[cfg(test)]
use crate::inventory::value::{Key, ParametersType, Value};
pub use node::Node;

pub(crate) mod class_parser;
pub(crate) mod inheritance_chain;
pub(crate) mod node_parser;
pub(crate) mod parser;

#[cfg(test)]
fn get_parameter<'a>(name: &str, params: &'a ParametersType) -> Option<&'a Value> {
    let parts: Vec<&str> = name.split(':').collect();
    get_nested_parameter(&parts, params)
}

#[cfg(test)]
fn get_nested_parameter<'a>(parts: &[&str], params: &'a ParametersType) -> Option<&'a Value> {
    if parts.is_empty() {
        return None;
    }

    if parts.len() == 1 {
        return params.get(&Key::String(parts[0].to_string()));
    }

    let mut current_params = params;
    let mut i = 0;

    while i < parts.len() {
        let current_part = parts[i];
        if let Some(value) = current_params.get(&Key::String(current_part.to_string())) {
            if i == parts.len() - 1 {
                return Some(value);
            }
            if let Value::Hash(hash) = value {
                current_params = hash;
                i += 1;
            } else {
                return None;
            }
        } else {
            return None;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::elements::class::Class;
    use crate::inventory::elements::node::Node;
    use hashlink::LinkedHashMap;

    #[test]
    fn test_get_parameter_simple() {
        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("hostname".to_string()),
            Value::String("test-host".to_string()),
        );

        let class = Class::new("test.class".to_string())
            .parameters(parameters)
            .build();

        assert_eq!(
            get_parameter("hostname", class.parameters()),
            Some(&Value::String("test-host".to_string()))
        );
        assert_eq!(get_parameter("nonexistent", class.parameters()), None);
    }

    #[test]
    fn test_get_parameter_nested() {
        use std::rc::Rc;
        let mut inner_map: ParametersType = LinkedHashMap::new();
        inner_map.insert(
            Key::String("ip".to_string()),
            Value::String("127.0.0.1".to_string()),
        );

        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("host".to_string()),
            Value::Hash(Rc::new(inner_map)),
        );

        let class = Class::new("test.class".to_string())
            .parameters(parameters)
            .build();

        assert_eq!(
            get_parameter("host:ip", class.parameters()),
            Some(&Value::String("127.0.0.1".to_string()))
        );
        assert_eq!(get_parameter("host:nonexistent", class.parameters()), None);
    }

    #[test]
    fn test_get_parameter_deeply_nested() {
        use std::rc::Rc;
        let mut level3: ParametersType = LinkedHashMap::new();
        level3.insert(
            Key::String("value".to_string()),
            Value::String("deep".to_string()),
        );

        let mut level2: ParametersType = LinkedHashMap::new();
        level2.insert(
            Key::String("level3".to_string()),
            Value::Hash(Rc::new(level3)),
        );

        let mut level1: ParametersType = LinkedHashMap::new();
        level1.insert(
            Key::String("level2".to_string()),
            Value::Hash(Rc::new(level2)),
        );

        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("level1".to_string()),
            Value::Hash(Rc::new(level1)),
        );

        let class = Class::new("test.class".to_string())
            .parameters(parameters)
            .build();

        assert_eq!(
            get_parameter("level1:level2:level3:value", class.parameters()),
            Some(&Value::String("deep".to_string()))
        );
    }

    #[test]
    fn test_get_parameter_node() {
        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("hostname".to_string()),
            Value::String("node-host".to_string()),
        );

        let node = Node::new("test-node".to_string())
            .parameters(parameters)
            .build();

        assert_eq!(
            get_parameter("hostname", node.parameters()),
            Some(&Value::String("node-host".to_string()))
        );
    }
}
