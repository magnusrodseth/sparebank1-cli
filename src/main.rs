//! `sb1`: a feature-complete CLI for the SpareBank 1 personal banking API.
//!
//! Usage of the underlying API is governed by the bank's terms; see
//! [`crate::terms`] for the clauses that shape this client's behaviour
//! (personal use, confidential credentials, no rate-limit circumvention).

mod auth;
mod cli;
mod client;
mod commands;
mod error;
mod format;
mod models;
mod secrets;
mod terms;
mod util;

use clap::Parser;

use crate::cli::Cli;

fn main() {
    let cli = Cli::parse();
    if let Err(err) = commands::run(cli) {
        // Print the full error chain to stderr, secrets already redacted upstream.
        eprintln!("error: {err}");
        for cause in err.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        std::process::exit(1);
    }
}
