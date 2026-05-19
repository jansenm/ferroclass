// SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use clap::Parser;
use ferroclass::output::ansible::{self, AnsibleInventoryError, HostVarsError};
use ferroclass::output::format_output;
use ferroclass::output::format_timestamp;
use snafu::{ResultExt, Snafu};
use std::env;
use std::process;

mod cli;
mod configuration;

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
    Load {
        source: ferroclass::configuration::Error,
    },
    #[snafu(display("error loading ansible inventory"))]
    Inventory { source: AnsibleInventoryError },
    #[snafu(display("error loading host vars"))]
    HostVars { source: HostVarsError },
}

#[cfg(not(tarpaulin_include))]
fn main() {
    tracing_subscriber::fmt::init();

    tracing::debug!("parsing command line options");
    let cli = cli::Cli::parse();
    tracing::debug!(options = ?cli, "command line options");

    let cwd = match env::current_dir().context(IoSnafu {}) {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("{}", format_error_chain(&e));
            process::exit(1);
        }
    };

    let mut config = match ferroclass::configuration::load(cwd.as_path()).context(LoadSnafu {}) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}", format_error_chain(&e));
            process::exit(1);
        }
    };

    let config = match configuration::apply_options(&mut config, &cli) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("{}", format_error_chain(&e));
            process::exit(1);
        }
    };
    tracing::debug!(?config, "configuration");

    let applications_postfix = cli.ansible_options.applications_postfix.clone();
    let output_format = config.output_options.output;
    let pretty = config.output_options.pretty_print;
    let sorted = config.output_options.output_sorted;

    match cli.command_options {
        cli::CommandOptions {
            list: true,
            host: None,
        } => {
            let timestamp = format_timestamp();
            let inventory =
                match ansible::build_inventory(&config, &applications_postfix, &timestamp)
                    .context(InventorySnafu {})
                {
                    Ok(inv) => inv,
                    Err(e) => {
                        eprintln!("{}", format_error_chain(&e));
                        process::exit(1);
                    }
                };
            match format_output(&inventory, output_format, pretty, sorted) {
                Ok(s) => println!("{}", s),
                Err(e) => {
                    eprintln!("Error serializing output: {}", e);
                    process::exit(1);
                }
            }
        }
        cli::CommandOptions {
            list: false,
            host: Some(ref hostname),
        } => {
            let timestamp = format_timestamp();
            let host_vars = match ansible::build_host_vars(&config, hostname, &timestamp)
                .context(HostVarsSnafu {})
            {
                Ok(vars) => vars,
                Err(e) => {
                    eprintln!("{}", format_error_chain(&e));
                    process::exit(1);
                }
            };
            match format_output(&host_vars, output_format, pretty, sorted) {
                Ok(s) => println!("{}", s),
                Err(e) => {
                    eprintln!("Error serializing output: {}", e);
                    process::exit(1);
                }
            }
        }
        _ => unreachable!("clap ensures either --list or --host is provided"),
    }
}
