// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

//! Generate man pages from CLI definitions.
//!
//! This binary generates roff-formatted man pages for the ferroclass,
//! ferroclass-ansible, and ferroclass-salt binaries using clap_mangen.
//!
//! Usage: cargo run --bin generate-manpages --features manpages

use std::fs;
use std::path::Path;

use clap::CommandFactory;
use clap_mangen::Man;

use ferroclass::cli::reclass::Arguments as ReclassCli;
use ferroclass::cli::reclass_ansible::Cli as AnsibleCli;
use ferroclass::cli::reclass_salt::Cli as SaltCli;

fn main() {
    let out_dir = Path::new("man");
    fs::create_dir_all(out_dir).expect("failed to create man/ directory");

    let commands: Vec<(&str, clap::Command)> = vec![
        ("ferroclass", ReclassCli::command()),
        ("ferroclass-ansible", AnsibleCli::command()),
        ("ferroclass-salt", SaltCli::command()),
    ];

    for (name, cmd) in commands {
        let man = Man::new(cmd);
        let mut buf: Vec<u8> = vec![];
        man.render(&mut buf).unwrap_or_else(|e| {
            panic!("failed to render man page for {name}: {e}");
        });

        let path = out_dir.join(format!("{name}.1"));
        fs::write(&path, &buf).unwrap_or_else(|e| {
            panic!("failed to write {}: {e}", path.display());
        });
        println!("Generated {}", path.display());
    }
}
