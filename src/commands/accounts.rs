//! Account commands: list, show, balance.

use anyhow::Context;

use crate::cli::{AccountArgs, AccountsArgs};
use crate::client::AccountListOpts;
use crate::commands::{authed_client, resolve_account};
use crate::format::{self, OutputMode};

pub fn list(args: AccountsArgs, mode: OutputMode) -> anyhow::Result<()> {
    let client = authed_client()?;
    let all = args.all;
    let opts = AccountListOpts {
        include_credit_cards: all || args.credit_cards,
        include_bsu: all || args.bsu,
        include_ask: all || args.ask,
        include_pension: all || args.pension,
        include_currency: all || args.currency,
    };
    let accounts = client.accounts(&opts).context("listing accounts")?;
    match mode {
        OutputMode::Json => format::print_json(&serde_json::json!({
            "accounts": accounts.iter().map(account_json).collect::<Vec<_>>()
        })),
        OutputMode::Table => {
            if accounts.is_empty() {
                println!("No accounts found.");
            } else {
                format::accounts_table(&accounts);
            }
            Ok(())
        }
    }
}

pub fn show(args: AccountArgs, mode: OutputMode) -> anyhow::Result<()> {
    let client = authed_client()?;
    let account = resolve_account(&client, &args.account)?;

    if args.roles {
        let roles = client
            .account_roles(&account.key)
            .context("fetching roles")?;
        return format::print_json(&roles);
    }
    if args.details {
        let details = client
            .account_details(&account.key)
            .context("fetching account details")?;
        return format::print_json(&details);
    }

    // Fetch the dedicated single-account resource for the freshest data.
    let account = client.account(&account.key).unwrap_or(account);

    match mode {
        OutputMode::Json => format::print_json(&account_json(&account)),
        OutputMode::Table => {
            format::account_detail_table(&account);
            Ok(())
        }
    }
}

pub fn balance(account_number: String) -> anyhow::Result<()> {
    let client = authed_client()?;
    let digits: String = account_number
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    let balance = client.balance(&digits).context("fetching balance")?;
    format::print_json(&balance)
}

/// Compact JSON projection for an account (raw numbers, no locale formatting).
fn account_json(a: &crate::models::Account) -> serde_json::Value {
    serde_json::json!({
        "key": a.key,
        "name": a.name,
        "accountNumber": a.number(),
        "balance": a.balance,
        "availableBalance": a.available_balance,
        "currency": a.currency(),
        "type": a.account_type,
    })
}
