// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use clap::Parser;
use snafu::{ResultExt, Snafu};
use std::env;
use std::process;

mod cmd;
mod configuration;
mod parser_options;

use self::cmd::{inventory_main, nodeinfo_main};

#[cfg(not(tarpaulin_include))]
fn format_error_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![format!("{}", err)];
    let mut source = err.source();
    while let Some(e) = source {
        parts.push(format!("  caused by: {}", e));
        source = e.source();
    }
    parts.join("\n")
}

#[cfg(not(tarpaulin_include))]
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("error getting working directory"))]
    Io { source: std::io::Error },
    #[snafu(display("error loading configuration"))]
    LoadConfig {
        source: ::ferroclass::configuration::Error,
    },
    #[snafu(display("error applying configuration options"))]
    ApplyConfig { source: configuration::ApplyError },
}

#[cfg(not(tarpaulin_include))]
fn main() {
    tracing_subscriber::fmt::init();

    tracing::debug!("parsing command line options");
    let cli = parser_options::Arguments::parse();
    tracing::debug!(options = ?cli, "command line options");

    let cwd = match env::current_dir().context(IoSnafu {}) {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("{}", format_error_chain(&e));
            process::exit(1);
        }
    };

    let mut config =
        match ::ferroclass::configuration::load(cwd.as_path()).context(LoadConfigSnafu {}) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("{}", format_error_chain(&e));
                process::exit(1);
            }
        };

    let config = match configuration::apply_options(&mut config, &cli).context(ApplyConfigSnafu {})
    {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}", format_error_chain(&e));
            process::exit(1);
        }
    };
    tracing::debug!(?config, "configuration");

    match cli.command_options {
        parser_options::CommandOptions {
            inventory: true,
            nodeinfo: None,
        } => {
            if let Err(e) = inventory_main(config) {
                eprintln!("{}", format_error_chain(&e));
                process::exit(1);
            }
        }
        parser_options::CommandOptions {
            inventory: false,
            nodeinfo: Some(ref node_name),
        } => {
            if let Err(e) = nodeinfo_main(config, node_name) {
                eprintln!("{}", format_error_chain(&e));
                process::exit(1);
            }
        }
        _ => panic!("should not happen"),
    }
}
