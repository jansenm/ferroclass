// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use crate::configuration_file;
use crate::inventory::class_mapping::ClassMapping;
use crate::inventory::options::{Options, OutputOptions, StorageOptions};
use crate::inventory::types::Environment;
use shellexpand::LookupError;
use snafu::prelude::*;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const CONFIG_FILE_NAME: &str = "reclass-config.yml";

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("invalid file {path}"))]
    File {
        path: String,
        source: std::io::Error,
    },
    #[snafu(display("invalid configuration file {path}"))]
    ConfigurationFile {
        source: configuration_file::Error,
        path: String,
    },
    #[cfg(not(test))]
    #[snafu(display("todo"))]
    Io { source: std::io::Error },
    #[snafu(display("invalid configuration encountered: {message}"))]
    Configuration {
        message: String,
        source: LookupError<env::VarError>,
    },
    #[snafu(display("invalid class mapping: {detail}"))]
    ClassMapping { detail: String },
}

#[cfg(test)]
pub fn configuration_paths(path: &Path) -> Result<Vec<PathBuf>, Error> {
    Ok(Vec::from([
        PathBuf::from(path),
        PathBuf::from(path).join("cfg/home"),
        PathBuf::from(path).join("cfg/etc"),
        PathBuf::from(path).join("cfg/exe"),
    ]))
}

#[cfg(not(test))]
#[cfg(not(tarpaulin_include))]
pub fn configuration_paths(path: &Path) -> Result<Vec<PathBuf>, Error> {
    Ok(Vec::from([
        PathBuf::from(path),
        PathBuf::from(&env::home_dir().ok_or(Error::Io {
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Failed to get $HOME directory",
            ),
        })?),
        PathBuf::from(&PathBuf::from("/etc/reclass")),
        PathBuf::from(
            env::current_exe()
                .context(IoSnafu {})?
                .parent()
                .ok_or(Error::Io {
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "error while looking for parent of {}",
                            env::current_exe().unwrap().display()
                        ),
                    ),
                })?,
        ),
    ]))
}

pub fn lookup(path: &Path) -> Result<Option<PathBuf>, Error> {
    for path in configuration_paths(path)? {
        let config_file = path.join(CONFIG_FILE_NAME);
        tracing::debug!("  - checking {}", config_file.display());
        if config_file.exists() {
            tracing::debug!("    found. using it");
            return Ok(Some(config_file));
        }
    }
    Ok(None)
}

#[cfg(not(tarpaulin_include))]
pub fn load(path: &Path) -> Result<Options, Error> {
    tracing::debug!("Looking for configuration file");
    let Some(config_file_path) = lookup(path)? else {
        return Ok(Default::default());
    };
    tracing::debug!("Loading configuration file");
    let mut config_file = File::open(&config_file_path).context(FileSnafu {
        path: config_file_path.to_string_lossy().to_string(),
    })?;
    let mut config_string = String::new();
    config_file
        .read_to_string(&mut config_string)
        .context(FileSnafu {
            path: config_file_path.to_string_lossy().to_string(),
        })?;
    let config =
        configuration_file::load_from_string(&config_string).context(ConfigurationFileSnafu {
            path: config_file_path.to_string_lossy().to_string(),
        })?;

    let mut storage_options = StorageOptions::default();
    if let Some(storage_type) = config.storage_type {
        storage_options.storage_type = storage_type
    };
    if let Some(nodes_uri) = config.nodes_uri {
        storage_options.yaml_fs_options.nodes_uri = nodes_uri
    };
    if let Some(classes_uri) = config.classes_uri {
        storage_options.yaml_fs_options.classes_uri = classes_uri
    };
    if let Some(inventory_base_uri) = config.inventory_base_uri {
        storage_options.yaml_fs_options.inventory_base_uri = inventory_base_uri
    };
    if let Some(parameter_key_style) = config.parameter_key_style {
        storage_options.yaml_fs_options.parameter_key_style = parameter_key_style.clone();
        storage_options.yaml_file_options.parameter_key_style = parameter_key_style;
    };
    if let Some(compose_node_name) = config.compose_node_name {
        storage_options.yaml_fs_options.compose_node_name = compose_node_name;
    };
    if let Some(default_environment) = config.default_environment {
        let env = Environment::from(default_environment);
        storage_options.yaml_fs_options.default_environment = env.clone();
        storage_options.yaml_file_options.default_environment = env;
    };

    let mut output_options = OutputOptions::default();
    if let Some(output) = config.output {
        output_options.output = output
    };
    if let Some(pretty_print) = config.pretty_print {
        output_options.pretty_print = pretty_print
    };
    if let Some(output_sorted) = config.output_sorted {
        output_options.output_sorted = output_sorted
    };
    if let Some(no_refs) = config.no_refs {
        output_options.no_refs = no_refs
    };
    if let Some(group_errors) = config.group_errors {
        output_options.group_errors = group_errors
    };

    let mut ignore_class_notfound = false;
    if let Some(icn) = config.ignore_class_notfound {
        ignore_class_notfound = icn;
    };
    let mut ignore_class_notfound_regexp: Vec<String> = Vec::new();
    if let Some(ref regexps) = config.ignore_class_notfound_regexp {
        ignore_class_notfound_regexp = regexps.clone();
    };

    let class_mappings = parse_class_mappings(config.class_mappings)?;

    let class_mappings_match_path = config.class_mappings_match_path.unwrap_or(false);

    let default_environment = storage_options.yaml_fs_options.default_environment.clone();
    let ignore_overwritten_missing_references =
        config.ignore_overwritten_missing_references.unwrap_or(true);
    let inventory_ignore_failed_node = config.inventory_ignore_failed_node.unwrap_or(false);
    let inventory_ignore_failed_render = config.inventory_ignore_failed_render.unwrap_or(false);

    Ok(Options {
        storage_options,
        output_options,
        class_mappings,
        class_mappings_match_path,
        default_environment,
        ignore_class_notfound,
        ignore_class_notfound_regexp,
        ignore_overwritten_missing_references,
        inventory_ignore_failed_node,
        inventory_ignore_failed_render,
        ..Options::default()
    })
}

fn parse_class_mappings(raw: Option<Vec<String>>) -> Result<Vec<ClassMapping>, Error> {
    let Some(strings) = raw else {
        return Ok(Vec::new());
    };
    strings
        .into_iter()
        .map(|s| {
            ClassMapping::parse(&s).map_err(|e| Error::ClassMapping {
                detail: e.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_lookup_none() {
        assert!(matches!(lookup(&PathBuf::from("/tmp")), Ok(None)));
    }

    #[test]
    fn test_parse_class_mappings_none() {
        let result = parse_class_mappings(None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_class_mappings_empty_vec() {
        let result = parse_class_mappings(Some(vec![])).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_class_mappings_single_glob() {
        let result = parse_class_mappings(Some(vec!["* default".to_string()])).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].class_names(), &["default"]);
    }

    #[test]
    fn test_parse_class_mappings_multiple() {
        let result = parse_class_mappings(Some(vec![
            "* default".to_string(),
            "/^www\\d+$/ webserver".to_string(),
        ]))
        .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].class_names(), &["default"]);
        assert_eq!(result[1].class_names(), &["webserver"]);
    }

    #[test]
    fn test_parse_class_mappings_invalid() {
        let result = parse_class_mappings(Some(vec!["".to_string()]));
        assert!(result.is_err());
    }
}
