// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Ferroclass LSP server binary.
//!
//! Communicates over stdio using the Language Server Protocol.
//! Run with: `ferroclass-lsp`

use ferroclass::lsp::LspServer;
use tower_lsp::LspService;
use tower_lsp::Server;

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::new(LspServer::new);
    Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
        .serve(service)
        .await;
}
