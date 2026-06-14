// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::applications::Applications;
use crate::inventory::value::{ClassList, Environment, Key, ParametersType, Value};
use serde::ser::{Serialize, SerializeMap, Serializer};
use yaml_rust2::Yaml as YamlValue;

#[derive(Debug, PartialEq, Clone)]
/// A reclass class with name, applications, classes, parameters, exports, and URI.
pub struct Class {
    name: String,
    applications: Applications,
    classes: ClassList,
    environment: Environment,
    parameters: ParametersType,
    exports: ParametersType,
    uri: Option<String>,
}

impl Class {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(name: String) -> ClassBuilder {
        ClassBuilder::new(name)
    }
}

impl Class {
    pub fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    pub fn applications_mut(&mut self) -> &mut Applications {
        &mut self.applications
    }

    pub fn classes_mut(&mut self) -> &mut ClassList {
        &mut self.classes
    }

    pub fn environment_mut(&mut self) -> &mut Environment {
        &mut self.environment
    }

    pub fn parameters_mut(&mut self) -> &mut ParametersType {
        &mut self.parameters
    }

    pub fn exports(&self) -> &ParametersType {
        &self.exports
    }

    pub fn exports_mut(&mut self) -> &mut ParametersType {
        &mut self.exports
    }

    pub fn name(&self) -> &str {
        &self.name
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

    pub fn parameters(&self) -> &ParametersType {
        &self.parameters
    }

    pub fn uri(&self) -> Option<&str> {
        self.uri.as_deref()
    }

    pub fn set_uri(&mut self, uri: impl Into<String>) {
        self.uri = Some(uri.into());
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct ClassBuilder {
    applications: Applications,
    classes: ClassList,
    environment: Environment,
    name: String,
    parameters: ParametersType,
    exports: ParametersType,
    uri: Option<String>,
}

impl ClassBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            applications: Applications::new(),
            classes: vec![],
            environment: Environment::default(),
            parameters: ParametersType::default(),
            exports: ParametersType::default(),
            uri: None,
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

    pub fn build(self) -> Class {
        Class {
            name: self.name,
            applications: self.applications,
            classes: self.classes,
            environment: self.environment,
            parameters: self.parameters,
            exports: self.exports,
            uri: self.uri,
        }
    }
}

impl Serialize for Class {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(6))?;
        map.serialize_entry("environment", &self.environment)?;
        map.serialize_entry("classes", &self.classes)?;
        map.serialize_entry("applications", &self.applications)?;
        map.serialize_entry("parameters", &self.parameters)?;
        map.serialize_entry("exports", &self.exports)?;
        map.end()
    }
}

impl Class {
    pub fn to_yaml_value(&self) -> YamlValue {
        use hashlink::LinkedHashMap;
        use std::rc::Rc;
        let mut map = LinkedHashMap::new();
        map.insert(
            Key::String("environment".to_string()),
            Value::String(self.environment.to_string()),
        );
        map.insert(
            Key::String("classes".to_string()),
            Value::Array(Rc::new(
                self.classes
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            )),
        );
        map.insert(
            Key::String("applications".to_string()),
            Value::Array(Rc::new(
                self.applications
                    .as_list()
                    .iter()
                    .map(|s| Value::String(s.clone()))
                    .collect(),
            )),
        );
        map.insert(
            Key::String("parameters".to_string()),
            Value::Hash(Rc::new(self.parameters.clone())),
        );
        map.insert(
            Key::String("exports".to_string()),
            Value::Hash(Rc::new(self.exports.clone())),
        );
        Value::Hash(Rc::new(map)).to_yaml_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashlink::LinkedHashMap;
    use indoc::indoc;
    use yaml_rust2::{YamlEmitter, YamlLoader};

    fn yaml_to_string(yaml: &YamlValue) -> String {
        let mut output = String::new();
        let mut emitter = YamlEmitter::new(&mut output);
        emitter.dump(yaml).unwrap();
        output
    }

    #[test]
    fn test_yaml_serialization_class_with_all_fields() {
        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("param1".to_string()),
            Value::String("value1".to_string()),
        );
        parameters.insert(Key::String("count".to_string()), Value::Integer(42));

        let class = Class::new("test.class".to_string())
            .environment("production".to_string())
            .classes(vec!["base.class".to_string()])
            .applications(Applications::from_vec(vec!["app1".to_string()]))
            .parameters(parameters)
            .build();

        let yaml_value = class.to_yaml_value();
        let yaml_str = yaml_to_string(&yaml_value);

        let expected = indoc! { r#"
            ---
            environment: production
            classes:
              - base.class
            applications:
              - app1
            parameters:
              param1: value1
              count: 42
            exports: {}
        "#}
        .trim_end();

        assert_eq!(yaml_str, expected);
    }

    #[test]
    fn test_yaml_serialization_class_empty_fields() {
        let class = Class::new("empty.class".to_string()).build();

        let yaml_value = class.to_yaml_value();
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
    fn test_yaml_serialization_class_nested_parameters() {
        use std::rc::Rc;
        let mut inner_map: ParametersType = LinkedHashMap::new();
        inner_map.insert(Key::String("enabled".to_string()), Value::Boolean(true));

        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(
            Key::String("config".to_string()),
            Value::Hash(Rc::new(inner_map)),
        );

        let class = Class::new("nested.class".to_string())
            .parameters(parameters)
            .build();

        let yaml_value = class.to_yaml_value();
        let yaml_str = yaml_to_string(&yaml_value);

        let expected = indoc! { r#"
            ---
            environment: base
            classes: []
            applications: []
            parameters:
              config:
                enabled: true
            exports: {}
        "#}
        .trim_end();

        assert_eq!(yaml_str, expected);
    }

    #[test]
    fn test_yaml_serialization_class_integer_keys() {
        let mut parameters: ParametersType = LinkedHashMap::new();
        parameters.insert(Key::Integer(1), Value::String("first".to_string()));
        parameters.insert(Key::Integer(2), Value::String("second".to_string()));

        let class = Class::new("int-key.class".to_string())
            .parameters(parameters)
            .build();

        let yaml_value = class.to_yaml_value();
        let yaml_str = yaml_to_string(&yaml_value);
        let parsed = YamlLoader::load_from_str(&yaml_str).unwrap();
        let params = &parsed[0]["parameters"];

        assert_eq!(params[1].as_str().unwrap(), "first");
        assert_eq!(params[2].as_str().unwrap(), "second");
    }

    #[test]
    fn test_json_serialization_class() {
        let parameters: ParametersType = LinkedHashMap::new();

        let class = Class::new("json.class".to_string())
            .environment("test".to_string())
            .classes(vec!["parent.class".to_string()])
            .applications(Applications::from_vec(vec!["myapp".to_string()]))
            .parameters(parameters)
            .build();

        let json_str = serde_json::to_string_pretty(&class).unwrap();

        let expected = serde_json::json!({
            "environment": "test",
            "classes": ["parent.class"],
            "applications": ["myapp"],
            "parameters": {},
            "exports": {}
        });

        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed, expected);
    }
}
