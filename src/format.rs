//! Output rendering: pretty tables (default), JSON, and CSV.
//!
//! Tables use Norwegian amount formatting (`kr 1 234,56`). JSON is emitted with
//! `--json` for scripting. Money is never rounded for JSON output — only the
//! table view applies locale formatting.

use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use serde::Serialize;

use crate::models::{Account, Transaction};
use crate::util::format_kr;

/// Global output mode, set from the top-level `--json` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Table,
    Json,
}

/// Print any serialisable value as pretty JSON.
pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn base_table() -> Table {
    let mut t = Table::new();
    t.load_preset(comfy_table::presets::UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    t
}

/// Render accounts as an aligned table.
pub fn accounts_table(accounts: &[Account]) {
    let mut table = base_table();
    table.set_header(vec!["Name", "Account no.", "Balance", "Ccy", "Key"]);
    for a in accounts {
        table.add_row(vec![
            Cell::new(&a.name),
            Cell::new(a.number()),
            Cell::new(format_kr(a.display_balance())).set_alignment(CellAlignment::Right),
            Cell::new(a.currency()),
            Cell::new(&a.key),
        ]);
    }
    println!("{table}");
    let total: f64 = accounts
        .iter()
        .filter(|a| a.currency() == "NOK")
        .map(|a| a.display_balance())
        .sum();
    println!("\nTotal (NOK accounts): {}", format_kr(total));
}

/// Render a single account as a key/value table.
pub fn account_detail_table(a: &Account) {
    let mut table = base_table();
    table.set_header(vec!["Field", "Value"]);
    let rows = [
        ("Name", a.name.clone()),
        ("Account number", a.number()),
        ("Balance", format_kr(a.balance.unwrap_or(0.0))),
        ("Available", format_kr(a.available_balance.unwrap_or(0.0))),
        ("Currency", a.currency().to_string()),
        ("Type", a.account_type.clone().unwrap_or_default()),
        (
            "Owner",
            a.owner
                .as_ref()
                .and_then(|o| o.name.clone())
                .unwrap_or_default(),
        ),
        ("Key", a.key.clone()),
    ];
    for (k, v) in rows {
        table.add_row(vec![Cell::new(k), Cell::new(v)]);
    }
    println!("{table}");
}

/// Render transactions as an aligned table.
pub fn transactions_table(txns: &[Transaction]) {
    let mut table = base_table();
    table.set_header(vec![
        "Date",
        "Description",
        "Amount",
        "Status",
        "Counterparty",
    ]);
    for t in txns {
        let counterparty = t
            .remote_account_name
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| t.remote_account_number.clone())
            .unwrap_or_default();
        table.add_row(vec![
            Cell::new(t.date_str()),
            Cell::new(t.best_description()),
            Cell::new(format_kr(t.amount_value())).set_alignment(CellAlignment::Right),
            Cell::new(t.booking_status.clone().unwrap_or_default()),
            Cell::new(counterparty),
        ]);
    }
    println!("{table}");

    let sum: f64 = txns.iter().map(|t| t.amount_value()).sum();
    println!("\n{} transaction(s). Net: {}", txns.len(), format_kr(sum));
}

/// Render transactions as CSV (id,date,description,amount,status,counterparty,
/// type_code,account_name). Used when exporting locally rather than via the
/// server-side `/transactions/export` endpoint.
pub fn transactions_csv(txns: &[Transaction]) -> String {
    let mut out =
        String::from("id,date,description,amount,currency,status,type_code,counterparty,account\n");
    for t in txns {
        let fields = [
            t.id.clone().unwrap_or_default(),
            t.date_str(),
            t.best_description(),
            format!("{:.2}", t.amount_value()),
            t.currency_code.clone().unwrap_or_default(),
            t.booking_status.clone().unwrap_or_default(),
            t.type_code.clone().unwrap_or_default(),
            t.remote_account_name
                .clone()
                .or_else(|| t.remote_account_number.clone())
                .unwrap_or_default(),
            t.account_name.clone().unwrap_or_default(),
        ];
        let line = fields
            .iter()
            .map(|f| csv_escape(f))
            .collect::<Vec<_>>()
            .join(",");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
