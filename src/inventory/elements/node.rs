// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::applications::Applications;
use crate::inventory::create_automatic_parameters;
use crate::inventory::diagnostic::{Diagnostic, DiagnosticSeverity, EntityState};
use crate::inventory::value::{ClassList, Environment, Key, ParametersType, Value};
use serde::ser::{Serialize, SerializeMap, Serializer};
use yaml_rust2::Yaml as YamlValue;

#[derive(Debug, PartialEq, Clone)]
/// A reclass node with name, applications, classes, parameters, exports, environment, and URI.
///
/// The `state` field indicates whether the node's data is trustworthy:
/// - [`EntityState::Valid`] — merging succeeded; all data is correct
/// - [`EntityState::Failed`] — merging failed; parameters, exports, classes,
///   and applications are empty. Only name, URI, state, and diagnostics
///   are populated.
///
/// The `diagnostics` field collects errors, warnings, and informational
/// messages produced during loading and merging.
pub struct Node {
    applications: Applications,
    classes: ClassList,
    environment: Environment,
    name: String,
    parameters: ParametersType,
    exports: ParametersType,
    uri: Option<String>,
    short_name: Option<String>,
    pathname: Option<String>,
    /// Whether this node's data is trustworthy.
    state: EntityState,
    /// Diagnostics (errors, warnings, info, hints) for this node.
    diagnostics: Vec<Diagnostic>,
}

impl Node {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(name: String) -> NodeBuilder {
        NodeBuilder::new(name)
    }
}

impl Node {
    pub fn applications_mut(&mut self) -> &mut Applications {
        &mut self.applications
    }

    pub fn classes_mut(&mut self) -> &mut ClassList {
        &mut self.classes
    }

    pub fn environment_mut(&mut self) -> &mut Environment {
        &mut self.environment
    }

    pub fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    pub fn parameters_mut(&mut self) -> &mut ParametersType {
        &mut self.parameters
    }

    pub fn applications(&self) -> &Applications {
        &self.applications
    }

    pub fn classes(&self) -> &ClassList {
        &self.classes
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parameters(&self) -> &ParametersType {
        &self.parameters
    }

    pub fn exports(&self) -> &ParametersType {
        &self.exports
    }

    pub fn exports_mut(&mut self) -> &mut ParametersType {
        &mut self.exports
    }

    pub fn uri(&self) -> Option<&str> {
        self.uri.as_deref()
    }

    pub fn set_uri(&mut self, uri: impl Into<String>) {
        self.uri = Some(uri.into());
    }

    pub fn short_name(&self) -> &str {
        self.short_name.as_deref().unwrap_or(&self.name)
    }

    pub fn set_short_name(&mut self, short_name: impl Into<String>) {
        self.short_name = Some(short_name.into());
    }

    pub fn pathname(&self) -> Option<&str> {
        self.pathname.as_deref()
    }

    pub fn set_pathname(&mut self, pathname: impl Into<String>) {
        self.pathname = Some(pathname.into());
    }

    /// Return the entity state (Valid or Failed).
    ///
    /// A `Valid` node has trustworthy data. A `Failed` node should not
    /// be used for anything except reporting diagnostics — its
    /// parameters, exports, classes, and applications are empty.
    pub fn state(&self) -> EntityState {
        self.state
    }

    /// Set the entity state.
    pub fn set_state(&mut self, state: EntityState) {
        self.state = state;
    }

    /// Return the diagnostics for this node.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Add a diagnostic to this node.
    pub fn add_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Return whether this node has any error-severity diagnostics.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error)
    }

    /// Return whether this node's data is trustworthy (state is Valid
    /// and no error-severity diagnostics).
    pub fn is_valid(&self) -> bool {
        self.state == EntityState::Valid && !self.has_errors()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct NodeBuilder {
    name: String,
    applications: Applications,
    classes: ClassList,
    environment: Environment,
    parameters: ParametersType,
    exports: ParametersType,
    uri: Option<String>,
    short_name: Option<String>,
    pathname: Option<String>,
    state: EntityState,
    diagnostics: Vec<Diagnostic>,
}

impl NodeBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            applications: Applications::new(),
            classes: vec![],
            environment: Environment::default(),
            parameters: ParametersType::default(),
            exports: ParametersType::default(),
            uri: None,
            short_name: None,
            pathname: None,
            state: EntityState::Valid,
            diagnostics: Vec::new(),
        }
    }

    pub fn applications(mut self, applications: Applications) -> Self {
        self.applications = applications;
        self
    }

    pub fn classes(mut self, classes: ClassList) -> Self {
        self.classes = classes;
        self
    }

    pub fn environment(mut self, environment: impl Into<Environment>) -> Self {
        self.environment = environment.into();
        self
    }

    pub fn parameters(mut self, parameters: ParametersType) -> Self {
        self.parameters = parameters;
        self
    }

    pub fn exports(mut self, exports: ParametersType) -> Self {
        self.exports = exports;
        self
    }

    pub fn uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }

    pub fn pathname(mut self, pathname: impl Into<String>) -> Self {
        self.pathname = Some(pathname.into());
        self
    }

    /// Set the entity state (Valid or Failed).
    pub fn state(mut self, state: EntityState) -> Self {
        self.state = state;
        self
    }

    /// Set the diagnostics for this node.
    pub fn diagnostics(mut self, diagnostics: Vec<Diagnostic>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    pub fn build(self) -> Node {
        Node {
            name: self.name,
            applications: self.applications,
            classes: self.classes,
            environment: self.environment,
            parameters: self.parameters,
            exports: self.exports,
            uri: self.uri,
            short_name: self.short_name,
            pathname: self.pathname,
            state: self.state,
            diagnostics: self.diagnostics,
        }
    }
}

impl Serialize for Node {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(5))?;
        map.serialize_entry("environment", &self.environment)?;
        map.serialize_entry("classes", &self.classes)?;
        map.serialize_entry("applications", &self.applications)?;
        map.serialize_entry("parameters", &self.parameters)?;
        map.serialize_entry("exports", &self.exports)?;
        map.end()
    }
}

impl Node {
    pub fn to_yaml_value(&self) -> YamlValue {
        use hashlink::LinkedHashMap;
        use std::sync::Arc;
        let mut map = LinkedHashMap::new();
        map.insert(
            Key::String("environment".to_string()),
            Value::String(self.environment.to_string()),
        );
        map.insert(
            Key::String("classes".to_string()),
            Value::Array(Arc::new(
                self.classes
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            )),
        );
        map.insert(
            Key::String("applications".to_string()),
            Value::Array(Arc::new(
                self.applications
                    .as_list()
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            )),
        );
        map.insert(
            Key::String("parameters".to_string()),
            Value::Hash(Arc::new(self.parameters.clone())),
        );
        map.insert(
            Key::String("exports".to_string()),
            Value::Hash(Arc::new(self.exports.clone())),
        );
        Value::Hash(Arc::new(map)).to_yaml_value()
    }

    pub fn to_yaml_value_with_reclass(&self, timestamp: &str, sorted: bool) -> YamlValue {
        use hashlink::LinkedHashMap;
        use std::sync::Arc;

        let mut parameters: ParametersType = LinkedHashMap::new();

        if !self
            .parameters
            .contains_key(&Key::String("_reclass_".to_string()))
        {
            let auto_params = create_automatic_parameters(self.name.as_str(), &self.environment);
            for (k, v) in auto_params {
                parameters.insert(k, v);
            }
        }

        for (k, v) in &self.parameters {
            parameters.insert(k.clone(), v.clone());
        }

        let mut reclass = LinkedHashMap::new();
        reclass.insert(
            Key::String("node".to_string()),
            Value::String(self.name.clone()),
        );
        reclass.insert(
            Key::String("name".to_string()),
            Value::String(self.name.clone()),
        );
        reclass.insert(
            Key::String("uri".to_string()),
            Value::String(self.uri.clone().unwrap_or_default()),
        );
        reclass.insert(
            Key::String("environment".to_string()),
            Value::String(self.environment.to_string()),
        );
        reclass.insert(
            Key::String("timestamp".to_string()),
            Value::String(timestamp.to_string()),
        );
        let mut map = LinkedHashMap::new();
        map.insert(
            Key::String("__reclass__".to_string()),
            Value::Hash(Arc::new(reclass)),
        );
        map.insert(
            Key::String("environment".to_string()),
            Value::String(self.environment.to_string()),
        );
        map.insert(
            Key::String("classes".to_string()),
            Value::Array(Arc::new(
                self.classes
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            )),
        );
        map.insert(
            Key::String("applications".to_string()),
            Value::Array(Arc::new(
                self.applications
                    .as_list()
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            )),
        );
        map.insert(
            Key::String("parameters".to_string()),
            Value::Hash(Arc::new(parameters)),
        );
        map.insert(
            Key::String("exports".to_string()),
            Value::Hash(Arc::new(self.exports.clone())),
        );
        Value::Hash(Arc::new(map)).to_yaml_value_sorted(sorted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashlink::LinkedHashMap;
    use indoc::{concatdoc, indoc};
    use serde_json::to_string as to_json_string;
    use serde_yml::to_string as to_yaml_string;
    use yaml_rust2::{YamlEmitter, YamlLoader};

    fn yaml_to_string(yaml: &YamlValue) -> String {
        let mut output = String::new();
        let mut emitter = YamlEmitter::new(&mut output);
        emitter.dump(yaml).unwrap();
        output
    }

    #[test]
    fn test_yaml_serialization_node_with_all_fields() {
        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("hostname".to_string()),
            Value::from("test-host".to_string()),
        );
        parameters.insert(Key::String("port".to_string()), Value::from(8080));
        parameters.insert(Key::String("https".to_string()), Value::from(false));

        let node = Node::new("test-node".to_string())
            .environment("production".to_string())
            .classes(vec!["class1".to_string(), "class2".to_string()])
            .applications(Applications::from_vec(vec!["app1".to_string()]))
            .parameters(parameters)
            .build();

        let yaml_value = node.to_yaml_value();
        let yaml_str = yaml_to_string(&yaml_value);

        let expected = indoc! { r#"
            ---
            environment: production
            classes:
              - class1
              - class2
            applications:
              - app1
            parameters:
              hostname: test-host
              port: 8080
              https: false
            exports: {}
        "#}
        .trim_end();

        assert_eq!(
            to_json_string(&node).unwrap(),
            concatdoc! {
                r#"{"environment":"production","classes":["class1","class2"],"#,
                r#""applications":["app1"],"parameters":{"hostname":"test-host","port":8080,"https":false},"exports":{}}"#
            }
        );
        assert_eq!(
            to_yaml_string(&node).unwrap(),
            indoc! {r#"
            environment: production
            classes:
            - class1
            - class2
            applications:
            - app1
            parameters:
              hostname: test-host
              port: 8080
              https: false
            exports: {}
            "#}
        );

        assert_eq!(yaml_str, expected);
    }

    #[test]
    fn test_yaml_serialization_node_empty_fields() {
        let node = Node::new("empty-node".to_string()).build();

        let yaml_value = node.to_yaml_value();
        let yaml_str = yaml_to_string(&yaml_value);

        let expected = indoc! { r#"
            ---
            environment: base
            classes: []
            applications: []
            parameters: {}
            exports: {}
        "#}
        .trim_end();

        assert_eq!(yaml_str, expected);
    }

    #[test]
    fn test_yaml_serialization_node_nested_parameters() {
        use std::sync::Arc;
        let mut inner_map: ParametersType = LinkedHashMap::new();
        inner_map.insert(
            Key::String("key1".to_string()),
            Value::String("value1".to_string()),
        );

        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("outer".to_string()),
            Value::Hash(Arc::new(inner_map)),
        );

        let node = Node::new("nested-node".to_string())
            .parameters(parameters)
            .build();

        let yaml_value = node.to_yaml_value();
        let yaml_str = yaml_to_string(&yaml_value);

        let expected = indoc! { r#"
            ---
            environment: base
            classes: []
            applications: []
            parameters:
              outer:
                key1: value1
            exports: {}
        "#}
        .trim_end();

        assert_eq!(yaml_str, expected);
    }

    #[test]
    fn test_yaml_serialization_node_integer_keys() {
        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(Key::Integer(42), Value::String("answer".to_string()));
        parameters.insert(Key::String("normal_key".to_string()), Value::Integer(100));

        let node = Node::new("int-key-node".to_string())
            .parameters(parameters)
            .build();

        let yaml_value = node.to_yaml_value();
        let yaml_str = yaml_to_string(&yaml_value);
        let parsed = YamlLoader::load_from_str(&yaml_str).unwrap();
        let params = &parsed[0]["parameters"];

        assert_eq!(params[42].as_str().unwrap(), "answer");
        assert_eq!(params["normal_key"].as_i64().unwrap(), 100);
    }

    #[test]
    fn test_json_serialization_node() {
        let parameters: ParametersType = LinkedHashMap::new();

        let node = Node::new("json-node".to_string())
            .environment("test".to_string())
            .classes(vec!["class1".to_string()])
            .applications(Applications::from_vec(vec!["app1".to_string()]))
            .parameters(parameters)
            .build();

        let json_str = serde_json::to_string_pretty(&node).unwrap();

        let expected = serde_json::json!({
            "environment": "test",
            "classes": ["class1"],
            "applications": ["app1"],
            "parameters": {},
            "exports": {}
        });

        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_yaml_value_with_reclass_includes_reclass_parameter() {
        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("hostname".to_string()),
            Value::String("test-host".to_string()),
        );

        let node = Node::new("my-node".to_string())
            .environment("production".to_string())
            .classes(vec!["class1".to_string()])
            .parameters(parameters)
            .build();

        let yaml_value = node.to_yaml_value_with_reclass("Wed Apr 22 12:00:00 2026", false);
        let yaml_str = yaml_to_string(&yaml_value);
        let parsed = YamlLoader::load_from_str(&yaml_str).unwrap();
        let doc = &parsed[0];

        let reclass_param = &doc["parameters"]["_reclass_"];
        assert_eq!(reclass_param["name"]["short"].as_str().unwrap(), "my-node");
        assert_eq!(reclass_param["name"]["full"].as_str().unwrap(), "my-node");
        assert_eq!(reclass_param["environment"].as_str().unwrap(), "production");

        assert_eq!(doc["parameters"]["hostname"].as_str().unwrap(), "test-host");

        let keys: Vec<_> = doc["parameters"].as_hash().unwrap().keys().collect();
        assert_eq!(keys[0], &YamlValue::String("_reclass_".to_string()));

        assert_eq!(doc["exports"].as_hash().unwrap().len(), 0);
    }
}
