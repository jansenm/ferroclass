// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::elements::node::Node;
use crate::inventory::elements::parser;
use crate::inventory::options::ParameterKeyStyle;
use crate::inventory::value::{Environment, Value};
use snafu::prelude::*;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("expected a mapping at the top level"))]
    HashExpected,
    #[snafu(display("node '{node_name}'"))]
    InvalidDefinition {
        source: parser::Error,
        node_name: String,
    },
}

pub(crate) fn parse_node(
    node_name: String,
    definition: Value,
    parameter_key_style: &ParameterKeyStyle,
    default_environment: &Environment,
) -> Result<Node, Error> {
    let data = parser::parse_definition(definition, parameter_key_style).context(
        InvalidDefinitionSnafu {
            node_name: node_name.clone(),
        },
    )?;

    let environment = data
        .environment
        .unwrap_or_else(|| default_environment.clone());
    let mut builder = Node::new(node_name);
    builder.classes(data.classes);
    builder.environment(environment);
    builder.parameters(data.parameters);
    builder.exports(data.exports);
    builder.applications(data.applications);
    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::value::Key;
    use crate::parser::yaml::{Parser, YamlParser};
    use indoc::indoc;

    #[test]
    fn test_parse_node() {
        let definition = indoc! { r#"
                ---
                environment: production
                classes:
                    - base
                    - second
                    - last
                applications:
                    - app1
                    - app2
                parameters:
                    os: linux
                "#};
        let parser = YamlParser::new();
        let yaml_definition = parser.parse(definition).unwrap();
        let node = parse_node(
            "test".into(),
            yaml_definition,
            &Default::default(),
            &Environment::default(),
        )
        .unwrap();

        assert_eq!(node.name(), "test");
        assert_eq!(node.applications().as_list(), &["app1", "app2"]);
        assert_eq!(node.classes(), &["base", "second", "last"]);
        assert_eq!(node.environment(), "production");
        let parameters = node.parameters();
        assert_eq!(parameters.len(), 1);
        assert_eq!(
            parameters.get(&Key::String("os".to_string())),
            Some(&Value::String("linux".to_string()))
        );
    }
}
