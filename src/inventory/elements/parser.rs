// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::options::ParameterKeyStyle;
use crate::inventory::value;
use crate::inventory::value::{
    Applications, ClassList, Environment, Key, ParametersType, Value, format_segments,
};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Failed to parse the classes definition"))]
    InvalidClassesDefinition,
    #[snafu(display("Failed to parse the parameter definition"))]
    InvalidParameterDefinition,
    #[snafu(display("Failed to parse the environment"))]
    InvalidEnvironment { source: value::Error },
    #[snafu(display("Unexpected key found {key}"))]
    UnexpectedKey { key: String },
    #[snafu(display("Expected a hash"))]
    HashExpected,
    #[snafu(display(
        "Invalid parameter key '{key}' found. Key must match ansible variable name rules (letters, numbers, underscores only)"
    ))]
    InvalidParameterKey { key: String },
}

#[derive(Debug, Default, Clone)]
pub struct Definition {
    pub classes: ClassList,
    pub environment: Option<Environment>,
    pub parameters: ParametersType,
    pub exports: ParametersType,
    pub applications: Applications,
}

pub(super) fn parse_application_list(definition: Value) -> Result<Applications, Error> {
    let mut apps = Applications::new();
    match definition {
        Value::Array(array) => {
            for element in array.iter() {
                apps.append_if_new(&parse_string_item(element.clone())?);
            }
            Ok(apps)
        }
        Value::Null => Ok(apps),
        _ => Err(Error::InvalidClassesDefinition),
    }
}

pub(super) fn parse_class_list(definition: Value) -> Result<ClassList, Error> {
    let mut list = ClassList::new();
    match definition {
        Value::Array(array) => {
            for element in array.iter() {
                list.push(parse_string_item(element.clone())?);
            }
            Ok(list)
        }
        Value::Null => Ok(list),
        _ => Err(Error::InvalidClassesDefinition),
    }
}

pub(super) fn parse_string_item(definition: Value) -> Result<String, Error> {
    match definition {
        Value::String(string) => Ok(string.to_owned()),
        Value::Reference(segments) => Ok(format!("${{{}}}", format_segments(&segments))),
        Value::StringWithReference(parts) => {
            let s = parts
                .iter()
                .map(|p| match p {
                    value::ReferencePart::Literal(lit) => lit.clone(),
                    value::ReferencePart::Reference(segments) => {
                        format!("${{{}}}", format_segments(segments))
                    }
                })
                .collect::<String>();
            Ok(s)
        }
        _ => Err(Error::InvalidClassesDefinition),
    }
}

pub(super) fn parse_environment(definition: Value) -> Result<Environment, Error> {
    let environment: Environment = definition.try_into().context(InvalidEnvironmentSnafu {})?;
    Ok(environment)
}

pub(super) fn parse_parameters(
    definition: Value,
    parameter_key_style: &ParameterKeyStyle,
) -> Result<ParametersType, Error> {
    match definition {
        Value::Hash(hash) => {
            for key in hash.keys() {
                if let Key::String(key_str) = key
                    && !is_valid_parameter_key(key_str, parameter_key_style)
                {
                    return Err(Error::InvalidParameterKey {
                        key: key_str.clone(),
                    });
                }
            }
            Ok(std::rc::Rc::try_unwrap(hash).unwrap_or_else(|rc| (*rc).clone()))
        }
        Value::Null => Ok(ParametersType::new()),
        _ => Err(Error::InvalidParameterDefinition),
    }
}

fn is_valid_parameter_key(key: &str, style: &ParameterKeyStyle) -> bool {
    match style {
        ParameterKeyStyle::None => true,
        ParameterKeyStyle::Ansible => key.chars().all(|c| c.is_alphanumeric() || c == '_'),
    }
}

pub(super) fn parse_definition(
    definition: Value,
    parameter_key_style: &ParameterKeyStyle,
) -> Result<Definition, Error> {
    match definition {
        Value::Hash(hash) => {
            let mut data = Definition::default();
            for (key, value) in hash.iter() {
                match key {
                    Key::String(key) => match key.to_lowercase().as_str() {
                        "classes" => {
                            data.classes = parse_class_list(value.clone())?;
                        }
                        "environment" => {
                            data.environment = Some(parse_environment(value.clone())?);
                        }
                        "parameters" => {
                            data.parameters = parse_parameters(value.clone(), parameter_key_style)?;
                        }
                        "exports" => {
                            data.exports = parse_parameters(value.clone(), parameter_key_style)?;
                        }
                        "applications" => {
                            data.applications = parse_application_list(value.clone())?;
                        }
                        _ => {
                            return Err(Error::UnexpectedKey {
                                key: key.to_lowercase().to_string(),
                            });
                        }
                    },
                    _ => unreachable!(),
                }
            }
            Ok(data)
        }
        Value::Null => Ok(Definition::default()),
        _ => Err(Error::HashExpected),
    }
}
