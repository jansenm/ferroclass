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

/// How far an entity's processing pipeline has progressed.
///
/// Both `Node` and `Class` carry a `state` field so callers can tell whether
/// the entity's data is usable and at what stage of processing it currently
/// resides. The states form a pipeline:
///
/// ```text
/// Source ──(merge)──→ Merged ──(interpolate)──→ Interpolated
///   │                   │                          │
///   └───(fail)──→ Failed ←────(fail)────────────┘
/// ```
///
/// - **Source** — parsed from YAML, no merging or interpolation applied.
///   The raw data as defined in the file: declared classes, local parameters,
///   local exports, environment, applications, and URI.
/// - **Merged** — class inheritance resolved and parameters merged, but
///   interpolation not yet applied. Data is trustworthy for structural
///   inspection but may contain unresolved `${...}` references.
/// - **Interpolated** — fully processed: merged, interpolated, and
///   `_reclass_` metadata added. All data is final and trustworthy.
///   This is the default state returned by `merge_node()`.
/// - **Failed** — processing failed. **Do not use the entity's data.**
///   Parameters, exports, classes, and applications are empty/zero. Only
///   the name, URI, pathname, state, and diagnostics are populated. The
///   entity exists as a placeholder so the caller can report what went
///   wrong and where.
/// # Ordering
///
/// Variants are ordered by pipeline progress: `Source < Merged < Interpolated`,
/// with `Failed` treated as the least advanced state so that
/// `Iterator::min()` correctly produces the worst-case state:
///
/// ```rust,ignore
/// assert!(Failed < Source);
/// assert!(Source < Merged);
/// assert!(Merged < Interpolated);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EntityState {
    /// Parsed from YAML, no merging or interpolation applied.
    Source,

    /// Class inheritance resolved and parameters merged.
    /// Interpolation not yet applied; data may contain unresolved `${...}`
    /// references.
    Merged,

    /// Fully processed: merged, interpolated, and `_reclass_` metadata added.
    /// All data is final and trustworthy.
    #[default]
    Interpolated,

    /// Processing failed. Data is NOT trustworthy — parameters, exports,
    /// classes, and applications are empty. Only name, URI, pathname, state,
    /// and diagnostics are populated.
    Failed,
}

impl EntityState {
    /// Returns `true` if the entity's data is trustworthy for use.
    ///
    /// `Source`, `Merged`, and `Interpolated` entities have usable data.
    /// `Failed` entities should not be relied on.
    pub fn is_usable(self) -> bool {
        self != EntityState::Failed
    }
}

impl fmt::Display for EntityState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntityState::Failed => write!(f, "failed"),
            EntityState::Source => write!(f, "source"),
            EntityState::Merged => write!(f, "merged"),
            EntityState::Interpolated => write!(f, "interpolated"),
        }
    }
}

impl Ord for EntityState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn progress(s: &EntityState) -> u8 {
            match s {
                EntityState::Failed => 0,
                EntityState::Source => 1,
                EntityState::Merged => 2,
                EntityState::Interpolated => 3,
            }
        }
        progress(self).cmp(&progress(other))
    }
}

impl PartialOrd for EntityState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

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
/// location (for LSP/IDE tooling), and an optional code for programmatic
/// identification.
///
/// The [`Display`](fmt::Display) format is `{severity}: {message} [{code}]`.
/// The message already contains the full error chain including file paths,
/// so `location` is NOT included in the display output — it exists as
/// structured metadata for LSP go-to-definition and related features.
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
        write!(f, "{}: {}", self.severity, self.message)?;
        if let Some(code) = &self.code {
            write!(f, " [{}]", code)?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

/// Trait for converting errors into structured diagnostics.
///
/// Each error type knows how to extract its file path, line/column, subject,
/// and diagnostic code. This moves the "what code, what message, what location"
/// knowledge next to the error definition instead of scattering it across
/// load functions and merge error handlers.
pub trait ToDiagnostics {
    /// Convert this error into one or more diagnostics.
    fn to_diagnostics(&self) -> Vec<Diagnostic>;
}

/// Format an error and its full source chain for diagnostic messages.
///
/// Walks `std::error::Error::source()` to produce a multi-line message:
/// the top-level error on the first line, each source cause indented on
/// subsequent lines (prefixed with `    caused by: `).
///
/// Example output:
/// ```text
/// invalid yaml in /path/to/file.yml
///     caused by: [91:8] while parsing a block mapping, did not find expected key
/// ```
pub fn format_error_chain(error: &dyn std::error::Error) -> String {
    let mut message = format!("{}", error);
    let mut source = error.source();
    while let Some(err) = source {
        message.push_str("\n    caused by: ");
        message.push_str(&format!("{}", err));
        source = err.source();
    }
    message
}

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
        // Location is structured metadata — not part of Display output.
        // Display format is: {severity}: {message} [{code}]
        let diag = Diagnostic::error("class not found")
            .with_location(SourceLocation::file_line_column("nodes/web.yml", 10, 5))
            .with_code("INV-001");
        let displayed = format!("{}", diag);
        assert!(displayed.contains("error"));
        assert!(displayed.contains("class not found"));
        assert!(displayed.contains("[INV-001]"));
        // Location is still accessible as structured data
        assert!(diag.location.is_some());
        let loc = diag.location.unwrap();
        assert_eq!(loc.file, Path::new("nodes/web.yml"));
        assert_eq!(loc.line, Some(10));
        assert_eq!(loc.column, Some(5));
    }

    #[test]
    fn test_diagnostic_display_without_location() {
        let diag = Diagnostic::warning("deprecated syntax");
        let displayed = format!("{}", diag);
        assert_eq!(displayed, "warning: deprecated syntax");
    }

    #[test]
    fn test_entity_state_ordering() {
        assert!(EntityState::Failed < EntityState::Source);
        assert!(EntityState::Source < EntityState::Merged);
        assert!(EntityState::Merged < EntityState::Interpolated);
        // min() returns the worst (least advanced) state
        assert_eq!(
            [
                EntityState::Interpolated,
                EntityState::Failed,
                EntityState::Merged
            ]
            .into_iter()
            .min(),
            Some(EntityState::Failed)
        );
        assert_eq!(
            [EntityState::Source, EntityState::Merged].into_iter().min(),
            Some(EntityState::Source)
        );
    }

    #[test]
    fn test_entity_state_default() {
        assert_eq!(EntityState::default(), EntityState::Interpolated);
    }

    #[test]
    fn test_entity_state_is_usable() {
        assert!(EntityState::Source.is_usable());
        assert!(EntityState::Merged.is_usable());
        assert!(EntityState::Interpolated.is_usable());
        assert!(!EntityState::Failed.is_usable());
    }

    #[test]
    fn test_entity_state_display() {
        assert_eq!(format!("{}", EntityState::Source), "source");
        assert_eq!(format!("{}", EntityState::Merged), "merged");
        assert_eq!(format!("{}", EntityState::Interpolated), "interpolated");
        assert_eq!(format!("{}", EntityState::Failed), "failed");
    }

    #[test]
    fn test_format_error_chain_simple() {
        use std::io;
        let err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let chain = format_error_chain(&err);
        assert_eq!(chain, "file not found");
    }

    #[test]
    fn test_format_error_chain_with_source() {
        // Create a snafu error with a source chain
        use crate::storage::file_system::Error as FsError;
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let fs_err = FsError::Io {
            source: io_err,
            path: "/etc/reclass/nodes/web.yml".to_string(),
        };
        let chain = format_error_chain(&fs_err);
        assert!(chain.contains("/etc/reclass/nodes/web.yml"));
        assert!(chain.contains("no such file"));
        // Source chain should use "caused by" format
        assert!(chain.contains("caused by:"));
    }
}
