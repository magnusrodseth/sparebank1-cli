//! Output rendering: pretty tables (default), JSON, and CSV.
//!
//! Tables use Norwegian amount formatting (`kr 1 234,56`). JSON is emitted with
//! `--json` for scripting. Money is never rounded for JSON output, only the
//! table view applies locale formatting.

use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use serde::Serialize;

use crate::models::{Account, Transaction};
use crate::util::{kr, MASKED};

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

/// Render accounts as an aligned table. With `mask`, account numbers and
/// balances are hidden (for sharing screenshots); names/currency/key stay.
pub fn accounts_table(accounts: &[Account], mask: bool) {
    let mut table = base_table();
    table.set_header(vec!["Name", "Account no.", "Balance", "Ccy", "Key"]);
    for a in accounts {
        let number = if mask { MASKED.to_string() } else { a.number() };
        table.add_row(vec![
            Cell::new(&a.name),
            Cell::new(number),
            Cell::new(kr(a.display_balance(), mask)).set_alignment(CellAlignment::Right),
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
    println!("\nTotal (NOK accounts): {}", kr(total, mask));
}

/// Render a single account as a key/value table. With `mask`, the account
/// number, balances and owner name are hidden.
pub fn account_detail_table(a: &Account, mask: bool) {
    let mut table = base_table();
    table.set_header(vec!["Field", "Value"]);
    let masked = |real: String| if mask { MASKED.to_string() } else { real };
    let rows = [
        ("Name", a.name.clone()),
        ("Account number", masked(a.number())),
        ("Balance", kr(a.balance.unwrap_or(0.0), mask)),
        ("Available", kr(a.available_balance.unwrap_or(0.0), mask)),
        ("Currency", a.currency().to_string()),
        ("Type", a.account_type.clone().unwrap_or_default()),
        (
            "Owner",
            masked(
                a.owner
                    .as_ref()
                    .and_then(|o| o.name.clone())
                    .unwrap_or_default(),
            ),
        ),
        ("Key", a.key.clone()),
    ];
    for (k, v) in rows {
        table.add_row(vec![Cell::new(k), Cell::new(v)]);
    }
    println!("{table}");
}

/// Render transactions as an aligned table. When the rows span more than one
/// account, an "Account" column is added so each row can be attributed.
pub fn transactions_table(txns: &[Transaction], mask: bool) {
    let multi_account = txns
        .iter()
        .filter_map(|t| t.account_name.as_deref())
        .filter(|s| !s.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .len()
        > 1;
    // Only present when the classified endpoint was used.
    let has_category = txns.iter().any(|t| t.category.is_some());

    let mut table = base_table();
    let mut header = vec!["Date", "Description", "Amount", "Status", "Counterparty"];
    if multi_account {
        header.insert(1, "Account");
    }
    if has_category {
        header.push("Category");
    }
    table.set_header(header);

    for t in txns {
        // Description and counterparty are free text that can leak merchants and
        // names, so they are masked too; date, account, status and category stay.
        let counterparty = if mask {
            MASKED.to_string()
        } else {
            t.remote_account_name
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| t.remote_account_number.clone())
                .unwrap_or_default()
        };
        let description = if mask {
            MASKED.to_string()
        } else {
            t.best_description()
        };
        let mut row = vec![Cell::new(t.date_str())];
        if multi_account {
            row.push(Cell::new(t.account_name.clone().unwrap_or_default()));
        }
        row.push(Cell::new(description));
        row.push(Cell::new(kr(t.amount_value(), mask)).set_alignment(CellAlignment::Right));
        row.push(Cell::new(t.booking_status.clone().unwrap_or_default()));
        row.push(Cell::new(counterparty));
        if has_category {
            row.push(Cell::new(t.category.clone().unwrap_or_default()));
        }
        table.add_row(row);
    }
    println!("{table}");

    let sum: f64 = txns.iter().map(|t| t.amount_value()).sum();
    println!("\n{} transaction(s). Net: {}", txns.len(), kr(sum, mask));
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

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "id,date,description,amount,currency,status,type_code,counterparty,account\n";

    fn txn(v: serde_json::Value) -> Transaction {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn empty_input_yields_header_only() {
        assert_eq!(transactions_csv(&[]), HEADER);
    }

    #[test]
    fn formats_amount_and_falls_back_to_counterparty_number() {
        let csv = transactions_csv(&[txn(serde_json::json!({
            "id": "t1",
            "amount": -50.0,
            "description": "Coffee",
            "currencyCode": "NOK",
            "bookingStatus": "BOOKED",
            "remoteAccountNumber": "12345678903",
        }))]);
        let row = csv.strip_prefix(HEADER).unwrap();
        // amount rendered with two decimals; counterparty falls back to number.
        assert_eq!(row.trim_end(), "t1,,Coffee,-50.00,NOK,BOOKED,,12345678903,");
    }

    #[test]
    fn escapes_commas_by_quoting() {
        let csv = transactions_csv(&[txn(serde_json::json!({"description": "Rema 1000, Oslo"}))]);
        assert!(csv.contains("\"Rema 1000, Oslo\""));
    }

    #[test]
    fn escapes_embedded_quotes_by_doubling() {
        let csv = transactions_csv(&[txn(serde_json::json!({"description": "The \"Big\" Shop"}))]);
        assert!(csv.contains("\"The \"\"Big\"\" Shop\""));
    }
}
