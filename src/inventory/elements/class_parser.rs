// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::elements::class::Class;
use crate::inventory::elements::parser;
use crate::inventory::options::ParameterKeyStyle;
use crate::inventory::value::{Environment, Value};
use snafu::prelude::*;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("expected a mapping at the top level"))]
    HashExpected,
    #[snafu(display("class '{class_name}'"))]
    InvalidDefinition {
        source: parser::Error,
        class_name: String,
    },
}

pub(crate) fn parse_class(
    class_name: String,
    definition: Value,
    parameter_key_style: &ParameterKeyStyle,
    default_environment: &Environment,
) -> Result<Class, Error> {
    let data = parser::parse_definition(definition, parameter_key_style).context(
        InvalidDefinitionSnafu {
            class_name: class_name.clone(),
        },
    )?;

    let environment = data
        .environment
        .unwrap_or_else(|| default_environment.clone());
    let mut builder = Class::new(class_name);
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
    fn test_parse_class() {
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
        let class = parse_class(
            "test".into(),
            yaml_definition,
            &Default::default(),
            &Environment::default(),
        )
        .unwrap();

        assert_eq!(class.name(), "test");
        assert_eq!(class.applications().as_list(), &["app1", "app2"]);
        assert_eq!(class.classes(), &["base", "second", "last"]);
        assert_eq!(class.environment(), "production");
        let parameters = class.parameters();
        assert_eq!(parameters.len(), 1);
        assert_eq!(
            parameters.get(&Key::String("os".to_string())),
            Some(&Value::String("linux".to_string()))
        );
    }
}
