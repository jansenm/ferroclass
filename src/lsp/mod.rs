// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! LSP server for ferroclass inventory files.
//!
//! Provides diagnostics, go-to-definition, and completion for reclass
//! inventory YAML files. The server communicates over stdio using the
//! Language Server Protocol.

mod server;

pub use server::LspServer;
