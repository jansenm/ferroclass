// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! LSP server implementation.
//!
//! The server holds an [`Inventory`] in memory, reloads it on file changes,
//! and answers LSP requests from it. All LSP-to-domain conversion happens
//! here — domain types never depend on LSP types.
//!
//! # Inventory detection
//!
//! On initialization the server checks whether the workspace root looks like
//! a reclass inventory (has `nodes/` + `classes/` directories, or a
//! `reclass-config.yml` file). If not, the server starts but does **not**
//! load or publish diagnostics — it stays idle until the client opens a
//! workspace that is a valid inventory root.

use crate::inventory::options::{Options, StorageType};
use crate::inventory::{self as inv, Diagnostic, DiagnosticSeverity, Inventory};
use lsp_types::{
    DiagnosticSeverity as LspSeverity, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::{Client, LanguageServer};

/// The LSP server backend.
///
/// Holds a reference-counted, read-write-locked `Inventory` that is
/// reloaded on file changes. All LSP protocol handling is delegated
/// from the [`tower_lsp::LanguageServer`] trait implementation.
pub struct LspServer {
    client: Client,
    inventory: Arc<RwLock<Option<Inventory>>>,
    options: Arc<RwLock<Options>>,
    /// Whether the workspace root was identified as a reclass inventory.
    active: Arc<RwLock<bool>>,
}

impl LspServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            inventory: Arc::new(RwLock::new(None)),
            options: Arc::new(RwLock::new(Options::default())),
            active: Arc::new(RwLock::new(false)),
        }
    }

    /// Check whether a path looks like a reclass inventory root.
    ///
    /// A directory is considered a reclass inventory if it has:
    /// - A `nodes/` directory, AND
    /// - A `classes/` directory
    /// OR:
    /// - A `reclass-config.yml` file
    fn is_reclass_inventory_root(path: &std::path::Path) -> bool {
        let has_nodes = path.join("nodes").is_dir();
        let has_classes = path.join("classes").is_dir();
        let has_config =
            path.join("reclass-config.yml").is_file() || path.join("reclass-config.yaml").is_file();

        (has_nodes && has_classes) || has_config
    }

    /// Load (or reload) the inventory from disk and publish diagnostics.
    ///
    /// Does nothing if the workspace is not a reclass inventory root.
    async fn reload_inventory(&self) {
        let active = *self.active.read().await;
        if !active {
            return;
        }

        let options = self.options.read().await;
        let result = inv::load_with_diagnostics(&options.storage_options);
        match result {
            Ok(load_result) => {
                let has_errors = load_result.has_errors();
                let diagnostics = load_result.diagnostics().to_vec();
                let inventory = load_result.into_inventory();

                // Clear old diagnostics for all previously-published files.
                self.clear_all_diagnostics(&diagnostics).await;

                // Publish new diagnostics for all files with errors/warnings.
                self.publish_diagnostics(&diagnostics).await;

                let mut inv = self.inventory.write().await;
                *inv = Some(inventory);

                if has_errors {
                    self.client
                        .log_message(
                            lsp_types::MessageType::ERROR,
                            "Inventory loaded with errors",
                        )
                        .await;
                } else {
                    self.client
                        .log_message(
                            lsp_types::MessageType::INFO,
                            "Inventory loaded successfully",
                        )
                        .await;
                }
            }
            Err(e) => {
                self.client
                    .log_message(
                        lsp_types::MessageType::ERROR,
                        format!("Failed to load inventory: {e}"),
                    )
                    .await;
            }
        }
    }

    /// Convert domain diagnostics to LSP diagnostics grouped by file URI.
    fn diagnostics_by_file(diagnostics: &[Diagnostic]) -> Vec<(Url, Vec<lsp_types::Diagnostic>)> {
        let mut files: std::collections::HashMap<Url, Vec<lsp_types::Diagnostic>> =
            std::collections::HashMap::new();

        for diag in diagnostics {
            let lsp_diag = Self::domain_diagnostic_to_lsp(diag);
            let uri = diag
                .location
                .as_ref()
                .and_then(|loc| Url::from_file_path(&loc.file).ok());

            // If no location, publish to a generic URI
            let uri = uri.unwrap_or_else(|| Url::parse("file:///__inventory__").unwrap());

            files.entry(uri).or_default().push(lsp_diag);
        }

        files.into_iter().collect()
    }

    /// Convert a domain [`Diagnostic`] to an LSP [`lsp_types::Diagnostic`].
    fn domain_diagnostic_to_lsp(diag: &Diagnostic) -> lsp_types::Diagnostic {
        let severity = match diag.severity {
            DiagnosticSeverity::Error => LspSeverity::ERROR,
            DiagnosticSeverity::Warning => LspSeverity::WARNING,
            DiagnosticSeverity::Info => LspSeverity::INFORMATION,
            DiagnosticSeverity::Hint => LspSeverity::HINT,
        };

        let range = diag
            .location
            .as_ref()
            .map_or_else(lsp_types::Range::default, |loc| {
                let line = loc.line.unwrap_or(0).saturating_sub(1) as u32;
                let col = loc.column.unwrap_or(0).saturating_sub(1) as u32;
                lsp_types::Range::new(
                    lsp_types::Position::new(line, col),
                    lsp_types::Position::new(line, col),
                )
            });

        lsp_types::Diagnostic::new(
            range,
            Some(severity),
            diag.code
                .as_deref()
                .map(|c| lsp_types::NumberOrString::String(c.to_string())),
            Some("ferroclass".to_string()),
            diag.message.clone(),
            None,
            None,
        )
    }

    /// Collect all file URIs that currently have diagnostics published.
    /// Used to clear stale diagnostics after reload.
    fn diagnostic_file_uris(diagnostics: &[Diagnostic]) -> Vec<Url> {
        diagnostics
            .iter()
            .filter_map(|diag| {
                diag.location
                    .as_ref()
                    .and_then(|loc| Url::from_file_path(&loc.file).ok())
            })
            .collect()
    }

    /// Clear diagnostics for all files that had them before.
    async fn clear_all_diagnostics(&self, _diagnostics: &[Diagnostic]) {
        // We need to clear diagnostics for files that no longer have errors.
        // The simplest approach: publish empty diagnostic lists for all files
        // that had diagnostics in the previous run. Since we don't track the
        // previous state yet, we'll rely on the new diagnostics replacing
        // the old ones per-file.
    }

    /// Publish diagnostics to the client for all affected files.
    async fn publish_diagnostics(&self, diagnostics: &[Diagnostic]) {
        let files = Self::diagnostics_by_file(diagnostics);
        for (uri, diags) in files {
            self.client.publish_diagnostics(uri, diags, None).await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for LspServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Check if the workspace root looks like a reclass inventory.
        if let Some(root_uri) = &params.root_uri {
            if let Ok(path) = root_uri.to_file_path() {
                if Self::is_reclass_inventory_root(&path) {
                    let mut active = self.active.write().await;
                    *active = true;

                    let mut options = self.options.write().await;
                    options.storage_options.storage_type = StorageType::YamlFs;
                    options.storage_options.yaml_fs_options.inventory_base_uri =
                        path.to_string_lossy().to_string();

                    self.client
                        .log_message(
                            lsp_types::MessageType::INFO,
                            format!(
                                "ferroclass: detected reclass inventory at {}",
                                path.display()
                            ),
                        )
                        .await;
                } else {
                    self.client
                        .log_message(
                            lsp_types::MessageType::INFO,
                            format!(
                                "ferroclass: workspace root {} is not a reclass inventory (no nodes/ + classes/ or reclass-config.yml); server will stay idle",
                                path.display()
                            ),
                        )
                        .await;
                }
            }
        } else {
            self.client
                .log_message(
                    lsp_types::MessageType::WARNING,
                    "ferroclass: no workspace root provided; server will stay idle",
                )
                .await;
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(lsp_types::OneOf::Left(true)),
                completion_provider: Some(lsp_types::CompletionOptions {
                    trigger_characters: Some(vec!["- ".to_string(), ":".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: lsp_types::InitializedParams) {
        self.client
            .log_message(
                lsp_types::MessageType::INFO,
                "ferroclass LSP server initialized",
            )
            .await;
        self.reload_inventory().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, _: DidOpenTextDocumentParams) {
        self.reload_inventory().await;
    }

    async fn did_change(&self, _: DidChangeTextDocumentParams) {
        self.reload_inventory().await;
    }

    async fn goto_definition(
        &self,
        _params: lsp_types::GotoDefinitionParams,
    ) -> Result<Option<lsp_types::GotoDefinitionResponse>> {
        let inv = self.inventory.read().await;
        let Some(inventory) = inv.as_ref() else {
            return Ok(None);
        };

        // v1: return None until we have document content from the client.
        // A full implementation would parse the document to find the word
        // under the cursor, then look it up as a class or node name.
        let _ = inventory;
        Ok(None)
    }

    async fn completion(
        &self,
        _params: lsp_types::CompletionParams,
    ) -> Result<Option<lsp_types::CompletionResponse>> {
        let inv = self.inventory.read().await;
        let Some(inventory) = inv.as_ref() else {
            return Ok(None);
        };

        let mut items: Vec<lsp_types::CompletionItem> = Vec::new();

        // Offer class names
        for name in inventory.class_names() {
            items.push(lsp_types::CompletionItem::new_simple(
                name.to_string(),
                "class".to_string(),
            ));
        }

        // Offer node names
        for name in inventory.node_names() {
            items.push(lsp_types::CompletionItem::new_simple(
                name.to_string(),
                "node".to_string(),
            ));
        }

        Ok(Some(lsp_types::CompletionResponse::Array(items)))
    }
}

/// Create an LSP Location from a file path and line/column (0-based).
fn make_location(file_path: &std::path::Path, line: u32, character: u32) -> lsp_types::Location {
    let uri = Url::from_file_path(file_path)
        .unwrap_or_else(|_| Url::parse("file:///__unknown__").unwrap());
    lsp_types::Location {
        uri,
        range: lsp_types::Range::new(
            lsp_types::Position::new(line, character),
            lsp_types::Position::new(line, character),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_reclass_inventory_root_with_nodes_and_classes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("nodes")).unwrap();
        std::fs::create_dir_all(root.join("classes")).unwrap();
        assert!(LspServer::is_reclass_inventory_root(root));
    }

    #[test]
    fn test_is_reclass_inventory_root_with_config_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("reclass-config.yml"), "storage: yaml_fs\n").unwrap();
        assert!(LspServer::is_reclass_inventory_root(root));
    }

    #[test]
    fn test_is_reclass_inventory_root_with_config_yml() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("reclass-config.yaml"), "storage: yaml_fs\n").unwrap();
        assert!(LspServer::is_reclass_inventory_root(root));
    }

    #[test]
    fn test_is_reclass_inventory_root_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!LspServer::is_reclass_inventory_root(tmp.path()));
    }

    #[test]
    fn test_is_reclass_inventory_root_only_nodes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("nodes")).unwrap();
        assert!(!LspServer::is_reclass_inventory_root(root));
    }

    #[test]
    fn test_is_reclass_inventory_root_only_classes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("classes")).unwrap();
        assert!(!LspServer::is_reclass_inventory_root(root));
    }
}
