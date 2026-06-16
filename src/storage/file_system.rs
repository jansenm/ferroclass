// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! File-system storage backends.
//!
//! Provides [`YamlFsRepository`] for the directory-tree layout (`nodes/`,
//! `classes/`, `_init.yml` autoloading) and [`YamlFileRepository`] for the
//! single-file layout.

use crate::inventory::diagnostic::{Diagnostic, SourceLocation, ToDiagnostics, format_error_chain};
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

impl ToDiagnostics for Error {
    fn to_diagnostics(&self) -> Vec<Diagnostic> {
        match self {
            Error::Io { path, .. } => {
                vec![
                    Diagnostic::error(format_error_chain(self))
                        .with_code("PARSE-002")
                        .with_location(SourceLocation::file(path)),
                ]
            }
            Error::Yaml { source, path } => {
                let mut location = SourceLocation::file(path);
                if let Some((line, col)) = source.line_col() {
                    location.line = Some(line);
                    location.column = Some(col);
                }
                vec![
                    Diagnostic::error(format_error_chain(self))
                        .with_code("PARSE-002")
                        .with_location(location),
                ]
            }
            Error::Encoding { path, .. } => {
                vec![
                    Diagnostic::error(format_error_chain(self))
                        .with_code("PARSE-002")
                        .with_location(SourceLocation::file(path)),
                ]
            }
            Error::InvalidPath { path, .. } => {
                vec![
                    Diagnostic::error(format_error_chain(self))
                        .with_code("PARSE-002")
                        .with_location(SourceLocation::file(path)),
                ]
            }
            Error::InvalidClassDefinition { source, path } => {
                let subject = extract_class_name(source);
                vec![
                    Diagnostic::error(format_error_chain(self))
                        .with_code("PARSE-002")
                        .with_location(SourceLocation::file(path))
                        .with_subject(subject),
                ]
            }
            Error::InvalidNodeDefinition { source, path } => {
                let subject = extract_node_name(source);
                vec![
                    Diagnostic::error(format_error_chain(self))
                        .with_code("PARSE-003")
                        .with_location(SourceLocation::file(path))
                        .with_subject(subject),
                ]
            }
        }
    }
}

/// Extract the class name from a class_parser error, if available.
fn extract_class_name(error: &class_parser::Error) -> String {
    match error {
        class_parser::Error::InvalidDefinition { class_name, .. } => class_name.clone(),
        class_parser::Error::HashExpected => "<unknown>".to_string(),
    }
}

/// Extract the node name from a node_parser error, if available.
fn extract_node_name(error: &node_parser::Error) -> String {
    match error {
        node_parser::Error::InvalidDefinition { node_name, .. } => node_name.clone(),
        node_parser::Error::HashExpected => "<unknown>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::diagnostic::{DiagnosticSeverity, ToDiagnostics};
    use std::path::Path;

    #[test]
    fn test_io_error_to_diagnostics() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let fs_err = Error::Io {
            source: io_err,
            path: "/etc/reclass/nodes/web.yml".to_string(),
        };
        let diags = fs_err.to_diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
        assert_eq!(diags[0].code.as_deref(), Some("PARSE-002"));
        assert!(diags[0].location.is_some());
        let loc = diags[0].location.as_ref().unwrap();
        assert_eq!(loc.file, Path::new("/etc/reclass/nodes/web.yml"));
    }

    #[test]
    fn test_encoding_error_to_diagnostics() {
        let bytes = vec![0xff, 0xfe];
        let utf8_err = String::from_utf8(bytes).unwrap_err();
        let fs_err = Error::Encoding {
            source: utf8_err,
            path: "/etc/reclass/classes/broken.yml".to_string(),
        };
        let diags = fs_err.to_diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.as_deref(), Some("PARSE-002"));
        assert!(diags[0].location.is_some());
    }

    #[test]
    fn test_invalid_path_to_diagnostics() {
        let fs_err = Error::InvalidPath {
            message: "path traversal detected".to_string(),
            path: "/etc/reclass/../../etc/passwd".to_string(),
        };
        let diags = fs_err.to_diagnostics();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.as_deref(), Some("PARSE-002"));
        assert!(diags[0].message.contains("path traversal"));
    }
}
