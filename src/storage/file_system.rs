// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! File-system storage backends.
//!
//! Provides [`YamlFsRepository`] for the directory-tree layout (`nodes/`,
//! `classes/`, `_init.yml` autoloading) and [`YamlFileRepository`] for the
//! single-file layout.

use crate::parser::yaml;
use snafu::Snafu;
use std::string::FromUtf8Error;

mod classes_iterator;
mod iterator;
mod nodes_iterator;
mod repository;
mod yaml_file_repository;

use crate::inventory::elements::{class_parser, node_parser};
pub use repository::YamlFsRepository;
pub use yaml_file_repository::YamlFileRepository;

/// Errors that can occur while reading YAML files from a filesystem repository.
#[derive(Debug, Snafu)]
#[snafu(visibility(pub(in super::file_system)))]
pub enum Error {
    #[snafu(display("i/o error: {path}"))]
    Io {
        source: std::io::Error,
        path: String,
    },
    #[snafu(display("invalid yaml in {path}"))]
    Yaml { source: yaml::Error, path: String },
    #[snafu(display("invalid UTF-8 encoding in {path}"))]
    Encoding { source: FromUtf8Error, path: String },
    #[snafu(display("{message}: {path}"))]
    InvalidPath { message: String, path: String },
    #[snafu(display("file {path}"))]
    InvalidClassDefinition {
        source: class_parser::Error,
        path: String,
    },
    #[snafu(display("file {path}"))]
    InvalidNodeDefinition {
        source: node_parser::Error,
        path: String,
    },
}
