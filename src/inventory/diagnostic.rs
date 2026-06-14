// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Diagnostic types for collect-and-continue error reporting.
//!
//! The diagnostic model replaces abort-on-first-error with a structured
//! approach: load and merge operations collect problems as they go, then
//! return both the partial result and a list of diagnostics. This lets
//! the LSP, MCP, and Explorer interfaces show *all* problems at once
//! instead of just the first one.
//!
//! # Usage
//!
//! ```rust,ignore
//! use ferroclass::inventory::diagnostic::{Diagnostic, DiagnosticSeverity};
//!
//! let report = inventory.merge_node_with_diagnostics("web01.example.com");
//! for diag in &report.diagnostics {
//!     match diag.severity {
//!         DiagnosticSeverity::Error => eprintln!("ERROR: {}", diag.message),
//!         DiagnosticSeverity::Warning => eprintln!("WARN: {}", diag.message),
//!         DiagnosticSeverity::Info => eprintln!("INFO: {}", diag.message),
//!         DiagnosticSeverity::Hint => eprintln!("HINT: {}", diag.message),
//!     }
//! }
//! ```

use std::fmt;
use std::path::PathBuf;

/// Severity level for diagnostics.
///
/// Follows the LSP specification severity levels, which map directly
/// to editor diagnostic displays. Variants are ordered from least severe
/// to most severe so that `Error > Warning > Info > Hint` (which is the
/// natural severity ordering where errors are "greater" than hints).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DiagnosticSeverity {
    /// A suggestion for improvement.
    ///
    /// Examples: unused class, simplifiable expression.
    Hint,

    /// Informational message about the inventory.
    ///
    /// Examples: class mapping applied, automatic parameters added.
    Info,

    /// A potential problem that doesn't prevent operation.
    ///
    /// Examples: class not found (with `ignore_class_notfound`), overridden
    /// parameter, deprecated syntax.
    Warning,

    /// An error that prevents correct operation.
    ///
    /// Examples: missing class, circular inheritance, unresolvable reference.
    Error,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticSeverity::Error => write!(f, "error"),
            DiagnosticSeverity::Warning => write!(f, "warning"),
            DiagnosticSeverity::Info => write!(f, "info"),
            DiagnosticSeverity::Hint => write!(f, "hint"),
        }
    }
}

/// Source location in a YAML file.
///
/// Tracks where a diagnostic originated, enabling "go to definition" and
/// "show all problems in file" in the LSP.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    /// Path to the source file (YAML class or node definition).
    pub file: PathBuf,

    /// 1-based line number within the file, if known.
    pub line: Option<usize>,

    /// 1-based column number within the line, if known.
    pub column: Option<usize>,
}

impl SourceLocation {
    /// Create a source location with only a file path.
    pub fn file(file: impl Into<PathBuf>) -> Self {
        Self {
            file: file.into(),
            line: None,
            column: None,
        }
    }

    /// Create a source location with file and line.
    pub fn file_line(file: impl Into<PathBuf>, line: usize) -> Self {
        Self {
            file: file.into(),
            line: Some(line),
            column: None,
        }
    }

    /// Create a source location with file, line, and column.
    pub fn file_line_column(file: impl Into<PathBuf>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line: Some(line),
            column: Some(column),
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file.display())?;
        if let Some(line) = self.line {
            write!(f, ":{}", line)?;
            if let Some(col) = self.column {
                write!(f, ":{}", col)?;
            }
        }
        Ok(())
    }
}

/// A diagnostic message produced during loading or merging.
///
/// Diagnostics carry a severity, human-readable message, optional source
/// location, and an optional code for programmatic identification.
///
/// Diagnostic codes follow a simple convention:
/// - `INV-xxx`: inventory-level errors (missing class, duplicate node)
/// - `MERGE-xxx`: merge errors (circular inheritance, override conflict)
/// - `REF-xxx`: reference errors (unresolvable `$[...]` reference)
/// - `PARSE-xxx`: parse errors (invalid YAML, unexpected type)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    /// How severe this diagnostic is.
    pub severity: DiagnosticSeverity,

    /// Human-readable message describing the problem.
    pub message: String,

    /// Where the diagnostic originated, if known.
    pub location: Option<SourceLocation>,

    /// Machine-readable diagnostic code (e.g. `"INV-001"`).
    pub code: Option<String>,

    /// The entity this diagnostic is about (node name, class name, etc.).
    pub subject: Option<String>,
}

impl Diagnostic {
    /// Create an error-level diagnostic.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            location: None,
            code: None,
            subject: None,
        }
    }

    /// Create a warning-level diagnostic.
    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            location: None,
            code: None,
            subject: None,
        }
    }

    /// Create an info-level diagnostic.
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Info,
            message: message.into(),
            location: None,
            code: None,
            subject: None,
        }
    }

    /// Create a hint-level diagnostic.
    pub fn hint(message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Hint,
            message: message.into(),
            location: None,
            code: None,
            subject: None,
        }
    }

    /// Attach a source location to this diagnostic.
    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Attach a diagnostic code to this diagnostic.
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Attach a subject (node name, class name, etc.) to this diagnostic.
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.location {
            Some(loc) => write!(f, "{}: {}: {}", loc, self.severity, self.message)?,
            None => write!(f, "{}: {}", self.severity, self.message)?,
        }
        if let Some(code) = &self.code {
            write!(f, " [{}]", code)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_diagnostic_severity_ordering() {
        assert!(DiagnosticSeverity::Error > DiagnosticSeverity::Warning);
        assert!(DiagnosticSeverity::Warning > DiagnosticSeverity::Info);
        assert!(DiagnosticSeverity::Info > DiagnosticSeverity::Hint);
    }

    #[test]
    fn test_diagnostic_severity_display() {
        assert_eq!(format!("{}", DiagnosticSeverity::Error), "error");
        assert_eq!(format!("{}", DiagnosticSeverity::Warning), "warning");
        assert_eq!(format!("{}", DiagnosticSeverity::Info), "info");
        assert_eq!(format!("{}", DiagnosticSeverity::Hint), "hint");
    }

    #[test]
    fn test_source_location_display() {
        let loc = SourceLocation::file("/etc/reclass/nodes/web.yml");
        assert_eq!(format!("{}", loc), "/etc/reclass/nodes/web.yml");

        let loc = SourceLocation::file_line("/etc/reclass/nodes/web.yml", 42);
        assert_eq!(format!("{}", loc), "/etc/reclass/nodes/web.yml:42");

        let loc = SourceLocation::file_line_column("/etc/reclass/nodes/web.yml", 42, 10);
        assert_eq!(format!("{}", loc), "/etc/reclass/nodes/web.yml:42:10");
    }

    #[test]
    fn test_diagnostic_error() {
        let diag = Diagnostic::error("class 'foo' not found")
            .with_code("INV-001")
            .with_subject("web01.example.com");
        assert_eq!(diag.severity, DiagnosticSeverity::Error);
        assert_eq!(diag.message, "class 'foo' not found");
        assert_eq!(diag.code.as_deref(), Some("INV-001"));
        assert_eq!(diag.subject.as_deref(), Some("web01.example.com"));
        assert!(diag.location.is_none());
    }

    #[test]
    fn test_diagnostic_warning_with_location() {
        let diag = Diagnostic::warning("class 'optional' not found, skipping")
            .with_location(SourceLocation::file_line("nodes/web.yml", 5));
        assert_eq!(diag.severity, DiagnosticSeverity::Warning);
        assert!(diag.location.is_some());
        let loc = diag.location.unwrap();
        assert_eq!(loc.file, Path::new("nodes/web.yml"));
        assert_eq!(loc.line, Some(5));
        assert_eq!(loc.column, None);
    }

    #[test]
    fn test_diagnostic_display_with_location() {
        let diag = Diagnostic::error("class not found")
            .with_location(SourceLocation::file_line_column("nodes/web.yml", 10, 5))
            .with_code("INV-001");
        let displayed = format!("{}", diag);
        assert!(displayed.contains("nodes/web.yml:10:5"));
        assert!(displayed.contains("error"));
        assert!(displayed.contains("class not found"));
        assert!(displayed.contains("[INV-001]"));
    }

    #[test]
    fn test_diagnostic_display_without_location() {
        let diag = Diagnostic::warning("deprecated syntax");
        let displayed = format!("{}", diag);
        assert_eq!(displayed, "warning: deprecated syntax");
    }
}
