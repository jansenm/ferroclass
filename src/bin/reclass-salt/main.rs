// SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
// SPDX-License-Identifier: MPL-2.0

use clap::Parser;
use ferroclass::output::format_output;
use ferroclass::output::salt::{self, PillarError, TopError};
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
    LoadConfig {
        source: ferroclass::configuration::Error,
    },
    #[snafu(display("error applying configuration options"))]
    ApplyConfig { source: configuration::ApplyError },
    #[snafu(transparent)]
    Top { source: TopError },
    #[snafu(transparent)]
    Pillar { source: PillarError },
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

    let mut config =
        match ferroclass::configuration::load(cwd.as_path()).context(LoadConfigSnafu {}) {
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

    let output_format = config.output_options.output;
    let pretty = config.output_options.pretty_print;
    let sorted = config.output_options.output_sorted;

    match cli.command_options {
        cli::CommandOptions {
            top: true,
            pillar: None,
        } => {
            let top_data = match salt::build_top(&config).map_err(Error::from) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("{}", format_error_chain(&e));
                    process::exit(1);
                }
            };
            match format_output(&top_data, output_format, pretty, sorted) {
                Ok(s) => println!("{}", s),
                Err(e) => {
                    eprintln!("Error serializing output: {}", e);
                    process::exit(1);
                }
            }
        }
        cli::CommandOptions {
            top: false,
            pillar: Some(ref minion_id),
        } => {
            let pillar_data = match salt::build_pillar(&config, minion_id).map_err(Error::from) {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("{}", format_error_chain(&e));
                    process::exit(1);
                }
            };
            match format_output(&pillar_data, output_format, pretty, sorted) {
                Ok(s) => println!("{}", s),
                Err(e) => {
                    eprintln!("Error serializing output: {}", e);
                    process::exit(1);
                }
            }
        }
        _ => unreachable!("clap ensures either --top or --pillar is provided"),
    }
}
