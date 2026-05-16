// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! CLI argument definitions for each binary.
//!
//! These structs are defined in the library crate so they can be shared
//! between the binaries and the man page generator, avoiding duplication
//! (Architecture Rules #1 and #2).

pub mod reclass;
pub mod reclass_ansible;
pub mod reclass_salt;
