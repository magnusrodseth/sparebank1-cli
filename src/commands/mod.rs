//! Command handlers and dispatch.

pub mod accounts;
pub mod auth;
pub mod transactions;
pub mod transfer;

use anyhow::{anyhow, Context};

use crate::cli::{Cli, Command};
use crate::client::{AccountListOpts, ApiClient};
use crate::format::OutputMode;
use crate::models::Account;

/// Resolved output mode from the global `--json` flag.
pub fn output_mode(cli: &Cli) -> OutputMode {
    if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Table
    }
}

/// Build an authenticated API client, refreshing the token if needed.
pub fn authed_client() -> anyhow::Result<ApiClient> {
    let token = crate::auth::valid_access_token().context("authentication required")?;
    Ok(ApiClient::new(token)?)
}

/// Resolve a user-supplied account reference (key, name, or number) to a full
/// [`Account`]. Fetches the account list (including all types) once.
///
/// Matching order: exact key → exact account number → exact name (ci) → unique
/// partial name (ci). Ambiguous or missing references are hard errors.
pub fn resolve_account(client: &ApiClient, input: &str) -> anyhow::Result<Account> {
    let opts = AccountListOpts {
        include_credit_cards: true,
        include_bsu: true,
        include_ask: true,
        include_pension: true,
        include_currency: true,
    };
    let accounts = client.accounts(&opts)?;
    let digits = |s: &str| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
    let target_digits = digits(input);
    let lower = input.to_lowercase();

    // 1. exact key
    if let Some(a) = accounts.iter().find(|a| a.key == input) {
        return Ok(a.clone());
    }
    // 2. exact account number (digits only)
    if !target_digits.is_empty() {
        if let Some(a) = accounts
            .iter()
            .find(|a| digits(&a.number()) == target_digits)
        {
            return Ok(a.clone());
        }
    }
    // 3. exact name (case-insensitive)
    if let Some(a) = accounts.iter().find(|a| a.name.to_lowercase() == lower) {
        return Ok(a.clone());
    }
    // 4. unique partial name
    let partial: Vec<&Account> = accounts
        .iter()
        .filter(|a| a.name.to_lowercase().contains(&lower))
        .collect();
    match partial.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(anyhow!(
            "no account matches '{input}'. Run `sb1 accounts` to list them."
        )),
        many => {
            let names = many
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(anyhow!("'{input}' is ambiguous; matches: {names}"))
        }
    }
}

/// Top-level dispatch.
pub fn run(cli: Cli) -> anyhow::Result<()> {
    let mode = output_mode(&cli);
    match cli.command {
        Command::Login(args) => auth::login(args),
        Command::Logout { all } => auth::logout(all),
        Command::Status => auth::status(mode),
        Command::Hello => auth::hello(),
        Command::Refresh => auth::refresh(),
        Command::Accounts(args) => accounts::list(args, mode),
        Command::Account(args) => accounts::show(args, mode),
        Command::Balance { account_number } => accounts::balance(account_number),
        Command::Transactions(args) => transactions::list(args, mode),
        Command::Transaction { id, classified } => transactions::show(id, classified),
        Command::Export(args) => transactions::export(args),
        Command::Transfer { kind } => transfer::run(kind, mode),
    }
}
