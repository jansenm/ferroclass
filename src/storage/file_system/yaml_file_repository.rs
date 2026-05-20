// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use super::Error;
use crate::inventory::elements::{Class, Node, class_parser, node_parser};
use crate::inventory::options::{ParameterKeyStyle, YamlFileStorageOptions};
use crate::inventory::types::Environment;
use crate::inventory::value::{Key, Value};
use crate::parser::yaml::{Parser, YamlParser};
use snafu::prelude::*;
use std::path::PathBuf;
use std::rc::Rc;
use std::{fs, path};

#[derive(Debug)]
/// Single-file YAML repository for class and node definitions.
///
/// Reads all classes and nodes from a single YAML multi-document file.
pub struct YamlFileRepository {
    file_path: PathBuf,
    parameter_key_style: ParameterKeyStyle,
    default_environment: Environment,
}

/// Metadata extracted from a YAML file's first document (the class/node URI hints).
#[derive(Debug, Default)]
pub struct InventoryMetadata {
    pub classes_uri: Option<String>,
    pub nodes_uri: Option<String>,
}

impl YamlFileRepository {
    pub fn new(
        options: &YamlFileStorageOptions,
        parameter_key_style: ParameterKeyStyle,
    ) -> Result<Self, Error> {
        let file_path = path::absolute(&options.inventory_file).context(super::IoSnafu {
            path: options.inventory_file.clone(),
        })?;
        Ok(Self {
            file_path,
            parameter_key_style,
            default_environment: options.default_environment.clone(),
        })
    }

    pub fn file_path(&self) -> &PathBuf {
        &self.file_path
    }

    pub fn load(&self) -> Result<(InventoryMetadata, Vec<Class>, Vec<Node>), Error> {
        let content = fs::read(&self.file_path).context(super::IoSnafu {
            path: self.file_path.to_string_lossy().to_string(),
        })?;
        let content_str = String::from_utf8(content).context(super::EncodingSnafu {
            path: self.file_path.to_string_lossy().to_string(),
        })?;
        let (metadata, mut classes, mut nodes) = Self::parse_content(
            &content_str,
            &self.parameter_key_style,
            &self.default_environment,
        )?;
        let file_uri = format!("yaml_file://{}", self.file_path.to_string_lossy());
        for class in &mut classes {
            class.set_uri(format!("{}#class:{}", file_uri, class.name()));
        }
        for node in &mut nodes {
            node.set_uri(format!("{}#node:{}", file_uri, node.name()));
        }
        Ok((metadata, classes, nodes))
    }

    pub fn load_from_string(
        content: &str,
        parameter_key_style: &ParameterKeyStyle,
        base_uri: Option<&str>,
        default_environment: &Environment,
    ) -> Result<(InventoryMetadata, Vec<Class>, Vec<Node>), Error> {
        let (metadata, mut classes, mut nodes) =
            Self::parse_content(content, parameter_key_style, default_environment)?;
        let file_uri = format!("yaml_file://{}", base_uri.unwrap_or("<memory>"));
        for class in &mut classes {
            class.set_uri(format!("{}#class:{}", file_uri, class.name()));
        }
        for node in &mut nodes {
            node.set_uri(format!("{}#node:{}", file_uri, node.name()));
        }
        Ok((metadata, classes, nodes))
    }

    fn parse_content(
        content: &str,
        parameter_key_style: &ParameterKeyStyle,
        default_environment: &Environment,
    ) -> Result<(InventoryMetadata, Vec<Class>, Vec<Node>), Error> {
        let parser = YamlParser::new();
        let mut classes = Vec::new();
        let mut nodes = Vec::new();

        if content.trim().is_empty() {
            return Ok((InventoryMetadata::default(), vec![], vec![]));
        }

        let mut documents = content
            .split("\n---")
            .map(|doc| {
                if doc.starts_with("---") {
                    parser.parse(doc.trim_start_matches("---"))
                } else {
                    parser.parse(doc)
                }
            })
            .collect::<Result<Vec<Value>, _>>()
            .context(super::YamlSnafu {
                path: "<memory>".to_string(),
            })?;

        for doc in &mut documents {
            doc.detect_references();
        }

        if documents.is_empty() {
            return Ok((InventoryMetadata::default(), vec![], vec![]));
        }

        let metadata = Self::parse_metadata(&documents[0])?;

        for doc in documents.iter().skip(1) {
            if let Value::Hash(hash) = doc {
                let name_key = Key::String("name".to_string());
                if let Some(name_value) = hash.get(&name_key)
                    && let Value::String(name) = name_value
                {
                    let type_key = Key::String("type".to_string());
                    let doc_type = hash
                        .get(&type_key)
                        .map(|v| {
                            if let Value::String(s) = v {
                                s.as_str()
                            } else {
                                "class"
                            }
                        })
                        .unwrap_or("class");

                    match doc_type {
                        "node" => {
                            let mut value = doc.clone();
                            if let Value::Hash(ref mut hash) = value {
                                let hash_mut = Rc::make_mut(hash);
                                hash_mut.remove(&name_key);
                                hash_mut.remove(&Key::String("type".to_string()));
                            }
                            let node = node_parser::parse_node(
                                name.clone(),
                                value,
                                parameter_key_style,
                                default_environment,
                            )
                            .context(
                                super::InvalidNodeDefinitionSnafu {
                                    path: "<memory>".to_string(),
                                },
                            )?;
                            nodes.push(node);
                        }
                        _ => {
                            let mut value = doc.clone();
                            if let Value::Hash(ref mut hash) = value {
                                let hash_mut = Rc::make_mut(hash);
                                hash_mut.remove(&name_key);
                            }
                            let class = class_parser::parse_class(
                                name.clone(),
                                value,
                                parameter_key_style,
                                default_environment,
                            )
                            .context(
                                super::InvalidClassDefinitionSnafu {
                                    path: "<memory>".to_string(),
                                },
                            )?;
                            classes.push(class);
                        }
                    }
                }
            }
        }

        Ok((metadata, classes, nodes))
    }

    fn parse_metadata(doc: &Value) -> Result<InventoryMetadata, Error> {
        let mut metadata = InventoryMetadata::default();

        if let Value::Hash(hash) = doc {
            for (key, value) in hash.iter() {
                if let Key::String(key_str) = key {
                    match key_str.as_str() {
                        "classes_uri" => {
                            if let Value::String(v) = value {
                                metadata.classes_uri = Some(v.clone());
                            }
                        }
                        "nodes_uri" => {
                            if let Value::String(v) = value {
                                metadata.nodes_uri = Some(v.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(metadata)
    }
}
