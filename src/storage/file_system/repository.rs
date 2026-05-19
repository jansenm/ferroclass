// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use super::classes_iterator::ClassesIterator;
use super::nodes_iterator::NodesIterator;
use super::{Error, InvalidClassDefinitionSnafu, InvalidNodeDefinitionSnafu, YamlSnafu};
use crate::inventory::elements::{Class, Node, class_parser, node_parser};
use crate::inventory::options::{ParameterKeyStyle, YamlFsStorageOptions};
use crate::inventory::types::Environment;
use crate::inventory::value::Value;
use crate::parser::yaml::{Parser, YamlParser};
use crate::storage::file_system::Error::InvalidPath;
use snafu::prelude::*;
use std::path::{Path, PathBuf};
use std::{fs, path};

#[derive(Debug)]
pub enum FileFormat {
    Yaml,
}

#[derive(Debug)]
pub struct YamlFsRepository {
    base_directory: PathBuf,
    nodes_directory: PathBuf,
    classes_directory: PathBuf,
    parameter_key_style: ParameterKeyStyle,
    compose_node_name: bool,
    default_environment: Environment,
}

impl YamlFsRepository {
    pub fn new(
        options: &YamlFsStorageOptions,
        parameter_key_style: ParameterKeyStyle,
    ) -> Result<Self, Error> {
        let base_directory =
            path::absolute(&options.inventory_base_uri).context(super::IoSnafu {
                path: options.inventory_base_uri.clone(),
            })?;
        let mut nodes_directory = PathBuf::from(&base_directory.clone());
        nodes_directory.push(options.nodes_uri.clone());
        let mut classes_directory = PathBuf::from(&base_directory.clone());
        classes_directory.push(options.classes_uri.clone());
        Ok(Self {
            base_directory,
            nodes_directory,
            classes_directory,
            parameter_key_style,
            compose_node_name: options.compose_node_name,
            default_environment: options.default_environment.clone(),
        })
    }

    pub fn classes_uri(&self) -> &PathBuf {
        &self.classes_directory
    }
    pub fn nodes_uri(&self) -> &PathBuf {
        &self.nodes_directory
    }

    pub fn nodes_iter(&self) -> NodesIterator<'_> {
        NodesIterator::new(self)
    }

    pub fn classes_iter(&self) -> ClassesIterator<'_> {
        ClassesIterator::new(self)
    }

    pub fn load(&self, path: &Path) -> Result<String, Error> {
        let content = fs::read(path).context(super::IoSnafu {
            path: path.to_string_lossy().to_string(),
        })?;
        String::from_utf8(content).context(super::EncodingSnafu {
            path: path.to_string_lossy().to_string(),
        })
    }

    pub(crate) fn load_class(&self, path: &Path, format: FileFormat) -> Result<Class, Error> {
        let class_name = self.class_name(path)?;
        tracing::debug!(?path, "loading class");
        let span = tracing::span!(tracing::Level::DEBUG, "loading class", ?class_name);
        let _s = span.enter();

        let parser = YamlParser {};
        let content = self.load(path)?;
        let path_str = path.to_string_lossy().to_string();
        let mut content_parsed: Value = match format {
            FileFormat::Yaml => parser.parse(&content),
        }
        .context(YamlSnafu {
            path: path_str.clone(),
        })?;
        content_parsed.detect_references();

        let mut class = class_parser::parse_class(
            class_name,
            content_parsed,
            &self.parameter_key_style,
            &self.default_environment,
        )
        .context(InvalidClassDefinitionSnafu { path: path_str })?;

        let uri = format!("yaml_fs://{}", path.to_string_lossy());
        class.set_uri(uri);
        Ok(class)
    }

    pub fn load_node(&self, path: &Path, format: FileFormat) -> Result<Node, Error> {
        let short_name = self.short_node_name(path)?;
        let node_name = if self.compose_node_name {
            self.composed_node_name(path)?
        } else {
            short_name.clone()
        };
        tracing::debug!(?path, "loading node");
        let span = tracing::span!(tracing::Level::DEBUG, "loading node", ?node_name);
        let _s = span.enter();

        let parser = YamlParser {};
        let content = self.load(path)?;
        let path_str = path.to_string_lossy().to_string();
        let mut content_parsed: Value = match format {
            FileFormat::Yaml => parser.parse(&content),
        }
        .context(YamlSnafu {
            path: path_str.clone(),
        })?;
        content_parsed.detect_references();

        let mut node = node_parser::parse_node(
            node_name,
            content_parsed,
            &self.parameter_key_style,
            &self.default_environment,
        )
        .context(InvalidNodeDefinitionSnafu { path: path_str })?;
        node.set_short_name(short_name);
        if let Ok(pathname) = self.node_pathname(path) {
            node.set_pathname(pathname);
        }
        let uri = format!("yaml_fs://{}", path.to_string_lossy());
        node.set_uri(uri);
        Ok(node)
    }

    fn short_node_name(&self, path: &Path) -> Result<String, Error> {
        if !path.starts_with(&self.base_directory) {
            return Err(InvalidPath {
                message: "Path must be inside root".to_string(),
                path: path.to_string_lossy().to_string(),
            });
        }
        let file_name = path
            .to_path_buf()
            .with_extension("")
            .file_name()
            .ok_or(InvalidPath {
                message: "invalid path encountered looking for a file name".to_string(),
                path: path.to_string_lossy().to_string(),
            })?
            .to_string_lossy()
            .to_string();

        Ok(file_name)
    }

    fn composed_node_name(&self, path: &Path) -> Result<String, Error> {
        let rel_path = path
            .strip_prefix(&self.nodes_directory)
            .map_err(|_err| InvalidPath {
                message: "Path must be inside nodes directory".to_string(),
                path: path.to_string_lossy().to_string(),
            })?;
        let rel_path_no_ext = rel_path.with_extension("");
        let rel_str = rel_path_no_ext.to_string_lossy();

        if rel_str == "." || rel_str.is_empty() {
            return self.short_node_name(path);
        }

        let parts: Vec<&str> = rel_str.split('/').collect();
        if parts[0].starts_with('_') {
            return self.short_node_name(path);
        }

        Ok(rel_str.replace('/', "."))
    }

    fn node_pathname(&self, path: &Path) -> Result<String, Error> {
        let rel_path = path
            .strip_prefix(&self.nodes_directory)
            .map_err(|_err| InvalidPath {
                message: "Path must be inside nodes directory".to_string(),
                path: path.to_string_lossy().to_string(),
            })?;
        let pathname = rel_path.with_extension("").to_string_lossy().to_string();
        Ok(pathname)
    }

    fn class_name(&self, path: &Path) -> Result<String, Error> {
        let rel_path = path
            .strip_prefix(&self.classes_directory)
            .map_err(|_err| InvalidPath {
                message: "Path must be inside root".to_string(),
                path: path.to_string_lossy().to_string(),
            })?
            .with_extension("");

        let file_name = rel_path
            .file_name()
            .ok_or_else(|| InvalidPath {
                message: "invalid path encountered looking for a file name".to_string(),
                path: path.to_string_lossy().to_string(),
            })?
            .to_string_lossy()
            .to_string();

        if file_name == "init" {
            let parent = rel_path.parent();
            match parent {
                Some(p) if !p.as_os_str().is_empty() => {
                    return Ok(p.to_string_lossy().to_string().replace('/', "."));
                }
                _ => return Ok(file_name),
            }
        }

        Ok(rel_path.to_string_lossy().to_string().replace('/', "."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::options::YamlFsStorageOptions;
    use std::path::PathBuf;

    fn make_repository(nodes_dir: &str, compose: bool) -> YamlFsRepository {
        YamlFsRepository::new(
            &YamlFsStorageOptions {
                inventory_base_uri: "/tmp".to_string(),
                nodes_uri: nodes_dir.to_string(),
                classes_uri: "classes".to_string(),
                parameter_key_style: crate::inventory::options::ParameterKeyStyle::None,
                compose_node_name: compose,
                ..YamlFsStorageOptions::default()
            },
            crate::inventory::options::ParameterKeyStyle::None,
        )
        .unwrap()
    }

    #[test]
    fn test_short_node_name_basename() {
        let repo = make_repository("nodes", false);
        let path = PathBuf::from("/tmp/nodes/web-server.yml");
        assert_eq!(repo.short_node_name(&path).unwrap(), "web-server");
    }

    #[test]
    fn test_short_node_name_nested() {
        let repo = make_repository("nodes", false);
        let path = PathBuf::from("/tmp/nodes/dev-infra/web-server.yml");
        assert_eq!(repo.short_node_name(&path).unwrap(), "web-server");
    }

    #[test]
    fn test_composed_node_name_nested() {
        let repo = make_repository("nodes", true);
        let path = PathBuf::from("/tmp/nodes/dev-infra/web-server.yml");
        assert_eq!(
            repo.composed_node_name(&path).unwrap(),
            "dev-infra.web-server"
        );
    }

    #[test]
    fn test_composed_node_name_root_level() {
        let repo = make_repository("nodes", true);
        let path = PathBuf::from("/tmp/nodes/web01.yml");
        assert_eq!(repo.composed_node_name(&path).unwrap(), "web01");
    }

    #[test]
    fn test_composed_node_name_underscore_dir() {
        let repo = make_repository("nodes", true);
        let path = PathBuf::from("/tmp/nodes/_cluster/web01.yml");
        assert_eq!(repo.composed_node_name(&path).unwrap(), "web01");
    }

    #[test]
    fn test_composed_node_name_deeply_nested() {
        let repo = make_repository("nodes", true);
        let path = PathBuf::from("/tmp/nodes/staging/db/primary.yml");
        assert_eq!(
            repo.composed_node_name(&path).unwrap(),
            "staging.db.primary"
        );
    }

    #[test]
    fn test_class_name_regular() {
        let repo = make_repository("nodes", false);
        let path = PathBuf::from("/tmp/classes/linux/distro/tumbleweed.yml");
        assert_eq!(repo.class_name(&path).unwrap(), "linux.distro.tumbleweed");
    }

    #[test]
    fn test_class_name_root_level() {
        let repo = make_repository("nodes", false);
        let path = PathBuf::from("/tmp/classes/defaults.yml");
        assert_eq!(repo.class_name(&path).unwrap(), "defaults");
    }

    #[test]
    fn test_class_name_init_in_subdirectory() {
        let repo = make_repository("nodes", false);
        let path = PathBuf::from("/tmp/classes/foo/init.yml");
        assert_eq!(repo.class_name(&path).unwrap(), "foo");
    }

    #[test]
    fn test_class_name_init_deeply_nested() {
        let repo = make_repository("nodes", false);
        let path = PathBuf::from("/tmp/classes/linux/distro/init.yml");
        assert_eq!(repo.class_name(&path).unwrap(), "linux.distro");
    }

    #[test]
    fn test_class_name_init_at_root() {
        let repo = make_repository("nodes", false);
        let path = PathBuf::from("/tmp/classes/init.yml");
        assert_eq!(repo.class_name(&path).unwrap(), "init");
    }

    #[test]
    fn test_class_name_non_init_in_subdirectory() {
        let repo = make_repository("nodes", false);
        let path = PathBuf::from("/tmp/classes/foo/bar.yml");
        assert_eq!(repo.class_name(&path).unwrap(), "foo.bar");
    }
}
