// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::inventory::options::{OutputFormat, ParameterKeyStyle, StorageType};
use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Invalid YAML encountered"))]
    InvalidYamlError { source: serde_yml::Error },
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConfigurationFile {
    pub storage_type: Option<StorageType>,
    pub nodes_uri: Option<String>,
    pub classes_uri: Option<String>,
    pub inventory_base_uri: Option<String>,
    pub parameter_key_style: Option<ParameterKeyStyle>,
    pub compose_node_name: Option<bool>,
    pub ignore_class_notfound: Option<bool>,
    pub ignore_class_notfound_regexp: Option<Vec<String>>,

    pub output: Option<OutputFormat>,
    pub pretty_print: Option<bool>,
    pub output_sorted: Option<bool>,
    pub no_refs: Option<bool>,
    pub group_errors: Option<bool>,

    pub class_mappings: Option<Vec<String>>,
    pub class_mappings_match_path: Option<bool>,

    pub default_environment: Option<String>,

    pub ignore_overwritten_missing_references: Option<bool>,

    pub inventory_ignore_failed_node: Option<bool>,
    pub inventory_ignore_failed_render: Option<bool>,
}

pub fn load_from_string(config_string: &str) -> Result<ConfigurationFile, Error> {
    let config: ConfigurationFile =
        serde_yml::from_str(config_string).map_err(|e| Error::InvalidYamlError { source: e })?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_load_invalid_config_multidoc() {
        let result = load_from_string(indoc! {r#"
            ---
            nodes_uri: knoten
            ---
            classes_uri: klassen
        "#});
        let Err(Error::InvalidYamlError { source }) = result else {
            panic!("{:?}", result);
        };
        assert_eq!(
            source.to_string(),
            "deserializing from YAML containing more than one document is not supported"
        );
    }

    #[test]
    fn test_load_invalid_config_list() {
        let result = load_from_string(indoc! {r#"
            ---
            - 1
            - A
            - C
        "#});
        let Err(Error::InvalidYamlError { source }) = result else {
            panic!("{:?}", result);
        };
        assert_eq!(
            source.to_string(),
            "invalid type: sequence, expected struct ConfigurationFile at line 2 column 1"
        );
    }

    #[test]
    fn test_load_empty_config() {
        let config = load_from_string(r#""#).unwrap();
        assert_eq!(config.storage_type, None);
        assert_eq!(config.nodes_uri, None);
        assert_eq!(config.classes_uri, None);
        assert_eq!(config.inventory_base_uri, None);
        assert_eq!(config.output, None);
        assert_eq!(config.pretty_print, None);
        assert_eq!(config.output_sorted, None);
    }

    #[test]
    fn test_load_config() {
        let config = load_from_string(
            r#"
                nodes_uri: knoten
                classes_uri: klassen
                pretty_print: false
                output_sorted: true
                output: json
                "#,
        )
        .unwrap();
        assert_eq!(config.storage_type, None);
        assert_eq!(config.nodes_uri, Some("knoten".to_string()));
        assert_eq!(config.classes_uri, Some("klassen".to_string()));
        assert_eq!(config.inventory_base_uri, None);
        assert_eq!(config.output, Some(OutputFormat::JSON));
        assert_eq!(config.pretty_print, Some(false));
        assert_eq!(config.output_sorted, Some(true));
    }

    #[test]
    fn test_load_config_with_class_mappings() {
        let config = load_from_string(
            r#"
                class_mappings:
                  - '* default'
                  - '/^www\d+/ webserver'
                class_mappings_match_path: true
                "#,
        )
        .unwrap();
        assert_eq!(
            config.class_mappings,
            Some(vec![
                "* default".to_string(),
                "/^www\\d+/ webserver".to_string()
            ])
        );
        assert_eq!(config.class_mappings_match_path, Some(true));
    }

    #[test]
    fn test_load_config_class_mappings_default() {
        let config = load_from_string(r#""#).unwrap();
        assert_eq!(config.class_mappings, None);
        assert_eq!(config.class_mappings_match_path, None);
    }
}
