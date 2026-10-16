//! Account commands: list, show, balance.

use anyhow::Context;

use crate::cli::{AccountArgs, AccountsArgs};
use crate::client::AccountListOpts;
use crate::commands::{aggregate, resolve_account};
use crate::format::{self, OutputMode};

pub fn list(
    args: AccountsArgs,
    mode: OutputMode,
    mask: bool,
    selected: Option<&str>,
    all_profiles: bool,
) -> anyhow::Result<()> {
    let all = all_profiles || args.all;
    let opts = AccountListOpts {
        include_credit_cards: all || args.credit_cards,
        include_bsu: all || args.bsu,
        include_ask: all || args.ask,
        include_pension: all || args.pension,
        include_currency: all || args.currency,
    };
    if !all_profiles {
        let clients = aggregate::clients_for(selected, false)?;
        let accounts = clients.clients[0]
            .client
            .accounts(&opts)
            .context("listing accounts")?;
        return match mode {
            OutputMode::Json => format::print_json(&serde_json::json!({
                "accounts": accounts.iter().map(account_json).collect::<Vec<_>>()
            })),
            OutputMode::Table => {
                if accounts.is_empty() {
                    println!("No accounts found.");
                } else {
                    format::accounts_table(&accounts, mask);
                }
                Ok(())
            }
        };
    }

    let mut clients = aggregate::clients_for(None, true)?;
    let accounts = clients.fetch_accounts(&opts);
    clients.report_failures();
    match mode {
        OutputMode::Json => format::print_json(&serde_json::json!({
            "complete": clients.complete(),
            "profiles": clients.profiles,
            "errors": clients.failures_json(),
            "accounts": accounts.iter().map(|entry| {
                let mut value = account_json(&entry.account);
                value["profiles"] = serde_json::json!(entry.profiles);
                value["shared"] = serde_json::json!(entry.profiles.len() > 1);
                value
            }).collect::<Vec<_>>()
        })),
        OutputMode::Table => {
            if accounts.is_empty() {
                println!("No accounts found.");
            } else {
                format::aggregate_accounts_table(&accounts, mask);
            }
            Ok(())
        }
    }
}

pub fn show(
    args: AccountArgs,
    mode: OutputMode,
    mask: bool,
    selected: Option<&str>,
    all_profiles: bool,
) -> anyhow::Result<()> {
    if !all_profiles {
        let clients = aggregate::clients_for(selected, false)?;
        let client = &clients.clients[0].client;
        let account = resolve_account(client, &args.account)?;

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

        return match mode {
            OutputMode::Json => format::print_json(&account_json(&account)),
            OutputMode::Table => {
                format::account_detail_table(&account, mask);
                Ok(())
            }
        };
    }

    let mut clients = aggregate::clients_for(None, true)?;
    let opts = all_accounts_opts();
    let entries = clients.fetch_accounts(&opts);
    let entry = match aggregate::resolve_account(&entries, &args.account) {
        Ok(entry) => entry,
        Err(error) => {
            clients.report_failures();
            return Err(error);
        }
    };
    let account_key = aggregate::account_key(&entry)?;
    if args.roles {
        let result = clients.first_success(&entry.profiles, |client| {
            client
                .client
                .account_roles(account_key)
                .map_err(anyhow::Error::from)
        });
        let Some((queried_by, roles)) = result else {
            clients.report_failures();
            if clients.is_rate_limited() {
                return format::print_json(&serde_json::json!({
                    "complete": false,
                    "errors": clients.failures_json(),
                    "profiles": entry.profiles,
                    "queriedBy": null,
                    "account": account_json(&entry.account),
                    "roles": null,
                }));
            }
            anyhow::bail!("could not fetch account roles from any profile");
        };
        clients.report_failures();
        return format::print_json(&serde_json::json!({
            "complete": clients.complete(),
            "errors": clients.failures_json(),
            "profiles": entry.profiles,
            "queriedBy": queried_by,
            "account": account_json(&entry.account),
            "roles": roles,
        }));
    }
    if args.details {
        let result = clients.first_success(&entry.profiles, |client| {
            client
                .client
                .account_details(account_key)
                .map_err(anyhow::Error::from)
        });
        let Some((queried_by, details)) = result else {
            clients.report_failures();
            if clients.is_rate_limited() {
                return format::print_json(&serde_json::json!({
                    "complete": false,
                    "errors": clients.failures_json(),
                    "profiles": entry.profiles,
                    "queriedBy": null,
                    "account": account_json(&entry.account),
                    "details": null,
                }));
            }
            anyhow::bail!("could not fetch account details from any profile");
        };
        clients.report_failures();
        return format::print_json(&serde_json::json!({
            "complete": clients.complete(),
            "errors": clients.failures_json(),
            "profiles": entry.profiles,
            "queriedBy": queried_by,
            "account": account_json(&entry.account),
            "details": details,
        }));
    }

    let result = clients.first_success(&entry.profiles, |client| {
        client
            .client
            .account(account_key)
            .map_err(anyhow::Error::from)
    });
    let (queried_by, account) = result
        .map(|(profile, account)| (Some(profile), account))
        .unwrap_or((None, entry.account.clone()));
    clients.report_failures();
    match mode {
        OutputMode::Json => {
            let mut result = account_json(&account);
            result["profiles"] = serde_json::json!(entry.profiles);
            result["shared"] = serde_json::json!(entry.profiles.len() > 1);
            format::print_json(&serde_json::json!({
                "complete": clients.complete(),
                "errors": clients.failures_json(),
                "queriedBy": queried_by,
                "account": result,
            }))
        }
        OutputMode::Table => {
            format::aggregate_accounts_table(&[entry], mask);
            Ok(())
        }
    }
}

pub fn balance(
    account_number: String,
    _mode: OutputMode,
    _mask: bool,
    selected: Option<&str>,
    all_profiles: bool,
) -> anyhow::Result<()> {
    let digits: String = account_number
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if !all_profiles {
        let clients = aggregate::clients_for(selected, false)?;
        let balance = clients.clients[0]
            .client
            .balance(&digits)
            .context("fetching balance")?;
        return format::print_json(&balance);
    }

    let mut clients = aggregate::clients_for(None, true)?;
    let entries = clients.fetch_accounts(&all_accounts_opts());
    let entry = match aggregate::resolve_account(&entries, &digits) {
        Ok(entry) => entry,
        Err(error) => {
            clients.report_failures();
            return Err(error);
        }
    };
    let result = clients.first_success(&entry.profiles, |client| {
        client.client.balance(&digits).map_err(anyhow::Error::from)
    });
    let (queried_by, balance) = result
        .map(|(profile, balance)| (Some(profile), balance))
        .unwrap_or((None, serde_json::Value::Null));
    clients.report_failures();
    format::print_json(&serde_json::json!({
        "complete": clients.complete(),
        "profiles": entry.profiles,
        "queriedBy": queried_by,
        "account": account_json(&entry.account),
        "balance": balance,
        "errors": clients.failures_json(),
    }))
}

pub(crate) fn all_accounts_opts() -> AccountListOpts {
    AccountListOpts {
        include_credit_cards: true,
        include_bsu: true,
        include_ask: true,
        include_pension: true,
        include_currency: true,
    }
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
