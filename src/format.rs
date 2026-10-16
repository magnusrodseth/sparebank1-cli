//! Output rendering: pretty tables (default), JSON, and CSV.
//!
//! Tables use Norwegian amount formatting (`kr 1 234,56`). JSON is emitted with
//! `--json` for scripting. Money is never rounded for JSON output, only the
//! table view applies locale formatting.

use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use serde::Serialize;

use crate::commands::aggregate::AccountEntry;
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

/// Render deduplicated accounts returned by more than one profile.
pub fn aggregate_accounts_table(accounts: &[AccountEntry], mask: bool) {
    let mut table = base_table();
    table.set_header(vec![
        "Profiles",
        "Name",
        "Account no.",
        "Balance",
        "Ccy",
        "Key",
    ]);
    for entry in accounts {
        let account = &entry.account;
        let number = if mask {
            MASKED.to_string()
        } else {
            account.number()
        };
        table.add_row(vec![
            Cell::new(entry.profiles.join(", ")),
            Cell::new(&account.name),
            Cell::new(number),
            Cell::new(kr(account.display_balance(), mask)).set_alignment(CellAlignment::Right),
            Cell::new(account.currency()),
            Cell::new(&account.key),
        ]);
    }
    println!("{table}");
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

/// Render transactions from several profiles, always keeping their source visible.
pub fn aggregate_transactions_table(rows: &[(String, Transaction)], mask: bool) {
    let multi_account = rows
        .iter()
        .filter_map(|(_, txn)| txn.account_name.as_deref())
        .filter(|name| !name.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .len()
        > 1;
    let has_category = rows.iter().any(|(_, txn)| txn.category.is_some());
    let mut table = base_table();
    let mut header = vec![
        "Profile",
        "Date",
        "Description",
        "Amount",
        "Status",
        "Counterparty",
    ];
    if multi_account {
        header.insert(1, "Account");
    }
    if has_category {
        header.push("Category");
    }
    table.set_header(header);

    for (profile, txn) in rows {
        let counterparty = if mask {
            MASKED.to_string()
        } else {
            txn.remote_account_name
                .clone()
                .filter(|s| !s.is_empty())
                .or_else(|| txn.remote_account_number.clone())
                .unwrap_or_default()
        };
        let description = if mask {
            MASKED.to_string()
        } else {
            txn.best_description()
        };
        let mut row = vec![Cell::new(profile)];
        if multi_account {
            row.push(Cell::new(txn.account_name.clone().unwrap_or_default()));
        }
        row.extend([
            Cell::new(txn.date_str()),
            Cell::new(description),
            Cell::new(kr(txn.amount_value(), mask)).set_alignment(CellAlignment::Right),
            Cell::new(txn.booking_status.clone().unwrap_or_default()),
            Cell::new(counterparty),
        ]);
        if has_category {
            row.push(Cell::new(txn.category.clone().unwrap_or_default()));
        }
        table.add_row(row);
    }
    println!("{table}");
    let sum: f64 = rows.iter().map(|(_, txn)| txn.amount_value()).sum();
    println!("\n{} transaction(s). Net: {}", rows.len(), kr(sum, mask));
}

/// Render locally fetched aggregate transactions to CSV with a profile column.
pub fn aggregate_transactions_csv(rows: &[(String, Transaction)]) -> String {
    let mut out = String::from(
        "profile,id,date,description,amount,currency,status,type_code,counterparty,account\n",
    );
    for (profile, txn) in rows {
        let fields = [
            profile.clone(),
            txn.id.clone().unwrap_or_default(),
            txn.date_str(),
            txn.best_description(),
            format!("{:.2}", txn.amount_value()),
            txn.currency_code.clone().unwrap_or_default(),
            txn.booking_status.clone().unwrap_or_default(),
            txn.type_code.clone().unwrap_or_default(),
            txn.remote_account_name
                .clone()
                .or_else(|| txn.remote_account_number.clone())
                .unwrap_or_default(),
            txn.account_name.clone().unwrap_or_default(),
        ];
        out.push_str(
            &fields
                .iter()
                .map(|field| csv_escape(field))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

/// Combine bank-rendered CSV exports and prepend the profile that fetched each row.
pub fn aggregate_exports(exports: &[(String, String)]) -> anyhow::Result<String> {
    use anyhow::Context;
    use csv::{ReaderBuilder, WriterBuilder};

    let mut expected_headers: Option<Vec<String>> = None;
    let mut delimiter = None;
    let has_bom = exports
        .first()
        .is_some_and(|(_, csv)| csv.as_bytes().starts_with(&[0xef, 0xbb, 0xbf]));
    let mut rows = Vec::new();
    for (profile, csv) in exports {
        let current_delimiter = detect_csv_delimiter(csv.as_bytes());
        if delimiter.is_some_and(|expected| expected != current_delimiter) {
            anyhow::bail!("bank returned different CSV delimiters across profiles");
        }
        delimiter = Some(current_delimiter);
        let mut reader = ReaderBuilder::new()
            .delimiter(current_delimiter)
            .from_reader(csv.as_bytes());
        let headers = reader
            .headers()
            .context("reading export CSV header")?
            .iter()
            .map(|field| field.strip_prefix('\u{feff}').unwrap_or(field).to_owned())
            .collect::<Vec<_>>();
        if let Some(expected) = &expected_headers {
            if expected != &headers {
                anyhow::bail!("bank returned different CSV columns across profiles");
            }
        } else {
            expected_headers = Some(headers);
        }
        for record in reader.records() {
            let record = record.context("reading export CSV row")?;
            rows.push((profile.as_str(), record));
        }
    }
    let delimiter = delimiter.unwrap_or(b',');
    let mut writer = WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());
    if let Some(headers) = expected_headers {
        let mut output_headers = vec!["profile".to_owned()];
        output_headers.extend(headers);
        writer.write_record(output_headers)?;
    } else {
        writer.write_record(["profile"])?;
    }
    for (profile, record) in rows {
        let mut output = vec![profile];
        output.extend(record.iter());
        writer.write_record(output)?;
    }
    let bytes = writer
        .into_inner()
        .map_err(|error| error.into_error())
        .context("writing aggregate CSV")?;
    let mut output = String::from_utf8(bytes)?;
    if has_bom {
        output.insert(0, '\u{feff}');
    }
    Ok(output)
}

fn detect_csv_delimiter(data: &[u8]) -> u8 {
    let header = data.split(|byte| *byte == b'\n').next().unwrap_or_default();
    let (mut commas, mut semicolons, mut quoted) = (0, 0, false);
    let mut index = 0;
    while index < header.len() {
        match header[index] {
            b'"' if quoted && header.get(index + 1) == Some(&b'"') => index += 1,
            b'"' => quoted = !quoted,
            b',' if !quoted => commas += 1,
            b';' if !quoted => semicolons += 1,
            _ => {}
        }
        index += 1;
    }
    if semicolons > commas {
        b';'
    } else {
        b','
    }
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

    #[test]
    fn aggregate_export_prepends_profile_and_preserves_quoted_rows() {
        let output = aggregate_exports(&[
            (
                "alice".into(),
                "date,description\n2026-01-01,\"Coffee, Oslo\"\n".into(),
            ),
            (
                "bob".into(),
                "date,description\n2026-01-02,\"Two lines\ncontinued\"\n".into(),
            ),
        ])
        .unwrap();
        let mut reader = csv::Reader::from_reader(output.as_bytes());
        assert_eq!(
            reader.headers().unwrap().iter().collect::<Vec<_>>(),
            ["profile", "date", "description"]
        );
        let rows: Vec<_> = reader.records().map(|record| record.unwrap()).collect();
        assert_eq!(&rows[0][0], "alice");
        assert_eq!(&rows[0][2], "Coffee, Oslo");
        assert_eq!(&rows[1][0], "bob");
        assert_eq!(&rows[1][2], "Two lines\ncontinued");
    }

    #[test]
    fn aggregate_export_preserves_semicolon_delimiter() {
        let output = aggregate_exports(&[(
            "alice".into(),
            "date;description\n2026-01-01;Coffee\n".into(),
        )])
        .unwrap();

        assert_eq!(
            output,
            "profile;date;description\nalice;2026-01-01;Coffee\n"
        );
    }

    #[test]
    fn aggregate_export_preserves_utf8_bom() {
        let output = aggregate_exports(&[(
            "alice".into(),
            "\u{feff}Dato;Beskrivelse\n2026-01-01;Kaffe\n".into(),
        )])
        .unwrap();

        assert!(output.starts_with('\u{feff}'));
        assert!(output.contains("profile;Dato;Beskrivelse"));
    }
}
