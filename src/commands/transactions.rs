//! Transaction commands: list, show, export.

use std::collections::HashSet;
use std::io::Write;

use anyhow::Context;

use crate::cli::{ExportArgs, TxnArgs};
use crate::client::TxnQuery;
use crate::commands::resolve_account;
use crate::format::{self, OutputMode};
use crate::util;

pub fn list(
    args: TxnArgs,
    mode: OutputMode,
    mask: bool,
    selected: Option<&str>,
    all_profiles: bool,
) -> anyhow::Result<()> {
    if all_profiles {
        return list_aggregate(args, mode, mask);
    }
    let clients = crate::commands::aggregate::clients_for(selected, false)?;
    let client = &clients.clients[0].client;

    // Resolve each account reference (positional or -a/--account) to its key.
    // If none given, use all accounts.
    let account_refs = args.account_refs();
    let account_keys: Vec<String> = if account_refs.is_empty() {
        client
            .accounts(&Default::default())?
            .into_iter()
            .map(|a| a.key)
            .collect()
    } else {
        let mut keys = Vec::new();
        for a in &account_refs {
            keys.push(resolve_account(client, a)?.key);
        }
        keys
    };

    if account_keys.is_empty() {
        anyhow::bail!("no accounts to query");
    }

    // Date range: --days wins, else --from/--to, else last 30 days.
    let (from, to) = match args.days {
        Some(n) => (util::days_ago(n), util::today()),
        None => (
            args.from.clone().unwrap_or_else(|| util::days_ago(30)),
            args.to.clone().unwrap_or_else(util::today),
        ),
    };

    let query = TxnQuery {
        account_keys,
        from_date: Some(from),
        to_date: Some(to),
        row_limit: args.limit,
        source: args.source.clone(),
        classified: args.classified,
    };

    let resp = client
        .transactions(&query)
        .context("listing transactions")?;
    if !resp.errors.is_empty() {
        eprintln!(
            "⚠ partial failure for some accounts: {}",
            resp.errors.join(", ")
        );
    }
    let txns = resp.transactions;

    // Decide rendering: --csv or --json or table; optional file output.
    let rendered: Option<String> = if args.csv {
        Some(format::transactions_csv(&txns))
    } else if mode == OutputMode::Json {
        Some(serde_json::to_string_pretty(&serde_json::json!({
            "transactions": txns.iter().map(txn_json).collect::<Vec<_>>()
        }))?)
    } else {
        None
    };

    match (rendered, &args.output) {
        (Some(text), Some(path)) => {
            std::fs::write(path, &text).with_context(|| format!("writing {path}"))?;
            eprintln!("✅ {} transaction(s) written to {path}", txns.len());
        }
        (Some(text), None) => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(text.as_bytes())?;
        }
        (None, _) => {
            if txns.is_empty() {
                println!("No transactions in range.");
            } else {
                format::transactions_table(&txns, mask);
            }
        }
    }
    Ok(())
}

fn list_aggregate(args: TxnArgs, mode: OutputMode, mask: bool) -> anyhow::Result<()> {
    let mut clients = crate::commands::aggregate::clients_for(None, true)?;
    let entries = clients.fetch_accounts(&crate::commands::accounts::all_accounts_opts());
    let refs = args.account_refs();
    let selected_entries = if refs.is_empty() {
        entries
    } else {
        let mut selected = Vec::new();
        let mut seen = HashSet::new();
        for reference in refs {
            let entry = match crate::commands::aggregate::resolve_account(&entries, &reference) {
                Ok(entry) => entry,
                Err(error) => {
                    clients.report_failures();
                    return Err(error);
                }
            };
            let identity = if entry.account.key.is_empty() {
                format!("{}:{}", entry.profiles[0], entry.account.name)
            } else {
                entry.account.key.clone()
            };
            if seen.insert(identity) {
                selected.push(entry);
            }
        }
        selected
    };

    let (from, to) = txn_date_range(&args);
    let mut rows = Vec::new();
    let mut request_failures = Vec::new();
    for entry in selected_entries {
        if clients.is_rate_limited() {
            break;
        }
        let account_key = match crate::commands::aggregate::account_key(&entry) {
            Ok(key) => key.to_owned(),
            Err(error) => {
                let source = entry.profiles.first().cloned().unwrap_or_default();
                request_failures.push((source, error.to_string()));
                continue;
            }
        };
        let query = TxnQuery {
            account_keys: vec![account_key],
            from_date: Some(from.clone()),
            to_date: Some(to.clone()),
            row_limit: args.limit,
            source: args.source.clone(),
            classified: args.classified,
        };
        let result = clients.first_success(&entry.profiles, |profile_client| {
            profile_client
                .client
                .transactions(&query)
                .map_err(anyhow::Error::from)
        });
        if let Some((source, response)) = result {
            for error in response.errors {
                request_failures.push((source.clone(), error));
            }
            rows.extend(
                response
                    .transactions
                    .into_iter()
                    .map(|txn| (source.clone(), txn)),
            );
        }
    }
    for (profile, error) in request_failures {
        clients.add_failure(&profile, error);
    }
    clients.report_failures();

    let rendered: Option<String> = if args.csv {
        Some(format::aggregate_transactions_csv(&rows))
    } else if mode == OutputMode::Json {
        Some(serde_json::to_string_pretty(&serde_json::json!({
            "complete": clients.complete(),
            "profiles": clients.profiles,
            "errors": clients.failures_json(),
            "transactions": rows.iter().map(|(profile, txn)| {
                let mut value = txn_json(txn);
                value["profile"] = serde_json::json!(profile);
                value
            }).collect::<Vec<_>>(),
        }))?)
    } else {
        None
    };

    match (rendered, &args.output) {
        (Some(text), Some(path)) => {
            std::fs::write(path, &text).with_context(|| format!("writing {path}"))?;
            eprintln!("✅ {} transaction(s) written to {path}", rows.len());
        }
        (Some(text), None) => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(text.as_bytes())?;
        }
        (None, _) => {
            if rows.is_empty() {
                println!("No transactions in range.");
            } else {
                format::aggregate_transactions_table(&rows, mask);
            }
        }
    }
    Ok(())
}

fn txn_date_range(args: &TxnArgs) -> (String, String) {
    match args.days {
        Some(n) => (util::days_ago(n), util::today()),
        None => (
            args.from.clone().unwrap_or_else(|| util::days_ago(30)),
            args.to.clone().unwrap_or_else(util::today),
        ),
    }
}

pub fn show(
    id: String,
    classified: bool,
    selected: Option<&str>,
    all_profiles: bool,
) -> anyhow::Result<()> {
    let mut clients = crate::commands::aggregate::clients_for(selected, all_profiles)?;
    if clients.aggregated {
        let mut matches = Vec::new();
        let mut failures = Vec::new();
        let mut not_found = Vec::new();
        let mut rate_limited = false;
        for entry in &clients.clients {
            if clients.is_rate_limited() || rate_limited {
                break;
            }
            match entry.client.transaction_details(&id, classified) {
                Ok(details) => {
                    if let Some(existing) = matches
                        .iter_mut()
                        .find(|result: &&mut serde_json::Value| result["details"] == details)
                    {
                        existing["profiles"]
                            .as_array_mut()
                            .expect("match profile list")
                            .push(serde_json::json!(entry.profile.name));
                    } else {
                        matches.push(serde_json::json!({
                            "profiles": [entry.profile.name],
                            "details": details,
                        }));
                    }
                }
                Err(error) if is_not_found(&error) => not_found.push(entry.profile.name.clone()),
                Err(error) => {
                    rate_limited = matches!(error, crate::error::Sb1Error::RateLimited { .. });
                    failures.push((entry.profile.name.clone(), anyhow::Error::from(error)));
                }
            }
        }
        for (profile, error) in failures {
            clients.add_api_error(&profile, error);
        }
        if matches.is_empty() && clients.failures.is_empty() {
            anyhow::bail!("transaction '{id}' was not found in any configured profile");
        }
        clients.report_failures();
        return format::print_json(&serde_json::json!({
            "complete": clients.complete(),
            "profiles": clients.profiles,
            "matches": matches,
            "notFoundIn": not_found,
            "errors": clients.failures_json(),
        }));
    }
    let client = &clients.clients[0].client;
    let details = client
        .transaction_details(&id, classified)
        .context("fetching transaction details")?;
    format::print_json(&details)
}

fn is_not_found(error: &crate::error::Sb1Error) -> bool {
    matches!(error, crate::error::Sb1Error::Api { status: 404, .. })
}

pub fn export(args: ExportArgs, selected: Option<&str>, all_profiles: bool) -> anyhow::Result<()> {
    let from = args.from.clone().unwrap_or_else(|| util::days_ago(90));
    let to = args.to.clone().unwrap_or_else(util::today);

    if all_profiles {
        let mut clients = crate::commands::aggregate::clients_for(None, true)?;
        let entries = clients.fetch_accounts(&crate::commands::accounts::all_accounts_opts());
        let entry = match crate::commands::aggregate::resolve_account(&entries, &args.account) {
            Ok(entry) => entry,
            Err(error) => {
                clients.report_failures();
                return Err(error);
            }
        };
        let account_key = crate::commands::aggregate::account_key(&entry)?.to_owned();
        let result = clients.first_success(&entry.profiles, |client| {
            client
                .client
                .transactions_export(&account_key, &from, &to, args.fields.as_deref())
                .map_err(anyhow::Error::from)
        });
        let Some((source, bank_csv)) = result else {
            clients.report_failures();
            anyhow::bail!("aggregate export failed; see the profile errors above");
        };
        let csv = format::aggregate_exports(&[(source, bank_csv)])?;
        clients.report_failures();
        return write_export(
            &csv,
            args.output.as_deref(),
            &entry.account.name,
            &from,
            &to,
        );
    }

    let clients = crate::commands::aggregate::clients_for(selected, false)?;
    let client = &clients.clients[0].client;
    let account = resolve_account(client, &args.account)?;

    let csv = client
        .transactions_export(&account.key, &from, &to, args.fields.as_deref())
        .context("exporting transactions")?;

    write_export(&csv, args.output.as_deref(), &account.name, &from, &to)
}

fn write_export(
    csv: &str,
    output: Option<&str>,
    account_name: &str,
    from: &str,
    to: &str,
) -> anyhow::Result<()> {
    match output {
        Some(path) => {
            std::fs::write(path, csv).with_context(|| format!("writing {path}"))?;
            eprintln!("✅ Exported {} ({from} → {to}) to {path}", account_name);
        }
        None => print!("{csv}"),
    }
    Ok(())
}

/// Compact JSON projection for a transaction (raw numbers, ISO date).
fn txn_json(t: &crate::models::Transaction) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "date": t.date_str(),
        "amount": t.amount,
        "currency": t.currency_code,
        "description": t.best_description(),
        "status": t.booking_status,
        "typeCode": t.type_code,
        "counterpartyName": t.remote_account_name,
        "counterpartyNumber": t.remote_account_number,
        "account": t.account_name,
        "category": t.category,
        "recurring": t.recurring,
        "subscription": t.subscription,
    })
}
