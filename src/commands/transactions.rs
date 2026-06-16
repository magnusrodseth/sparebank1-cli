//! Transaction commands: list, show, export.

use std::io::Write;

use anyhow::Context;

use crate::cli::{ExportArgs, TxnArgs};
use crate::client::TxnQuery;
use crate::commands::{authed_client, resolve_account};
use crate::format::{self, OutputMode};
use crate::util;

pub fn list(args: TxnArgs, mode: OutputMode) -> anyhow::Result<()> {
    let client = authed_client()?;

    // Resolve each --account reference to its key. If none given, use all.
    let account_keys: Vec<String> = if args.accounts.is_empty() {
        client
            .accounts(&Default::default())?
            .into_iter()
            .map(|a| a.key)
            .collect()
    } else {
        let mut keys = Vec::new();
        for a in &args.accounts {
            keys.push(resolve_account(&client, a)?.key);
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
                format::transactions_table(&txns);
            }
        }
    }
    Ok(())
}

pub fn show(id: String, classified: bool) -> anyhow::Result<()> {
    let client = authed_client()?;
    let details = client
        .transaction_details(&id, classified)
        .context("fetching transaction details")?;
    format::print_json(&details)
}

pub fn export(args: ExportArgs) -> anyhow::Result<()> {
    let client = authed_client()?;
    let account = resolve_account(&client, &args.account)?;
    let from = args.from.clone().unwrap_or_else(|| util::days_ago(90));
    let to = args.to.clone().unwrap_or_else(util::today);

    let csv = client
        .transactions_export(&account.key, &from, &to, args.fields.as_deref())
        .context("exporting transactions")?;

    match &args.output {
        Some(path) => {
            std::fs::write(path, &csv).with_context(|| format!("writing {path}"))?;
            eprintln!("✅ Exported {} ({from} → {to}) to {path}", account.name);
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
    })
}
