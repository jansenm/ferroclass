// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Shared domain types used across the inventory subsystem.
//!
//! The main type is [`Environment`], which represents the reclass node
//! environment (defaults to `"base"`).

pub mod environment;

pub use environment::Environment;
