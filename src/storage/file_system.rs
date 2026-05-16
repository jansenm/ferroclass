// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

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

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(in super::file_system)))]
pub enum Error {
    #[snafu(display("i/o error: {path}"))]
    Io {
        source: std::io::Error,
        path: String,
    },
    #[snafu(display("file error: {path}"))]
    File {
        source: std::io::Error,
        path: String,
    },
    #[snafu(display("invalid file: {path}"))]
    InvalidFile {
        source: std::io::Error,
        path: String,
    },
    #[snafu(display("invalid yaml encountered"))]
    Yaml { source: yaml::Error },
    #[snafu(display("problematic encoding encountered"))]
    Encoding { source: FromUtf8Error },
    #[snafu(display("{message}: {path}"))]
    InvalidPath { message: String, path: String },
    #[snafu(display("recursive path detected {message}"))]
    RecursivePath { message: String },
    #[snafu(display("Failed to load the class"))]
    InvalidClassDefinition { source: class_parser::Error },
    #[snafu(display("Failed to load the node"))]
    InvalidNodeDefinition { source: node_parser::Error },
}
