//! Transfer commands. Money movement is irreversible, so every path shows a
//! confirmation summary before executing (skippable with `-y/--yes`).
//!
//! Per the agreed scope, `debit` transfers are restricted to the user's **own**
//! accounts: both `--from` and `--to` must resolve to accounts in the user's own
//! account list. An unknown destination is rejected rather than sent.

use std::io::{self, Write};

use anyhow::{anyhow, bail, Context};

use crate::cli::{CreditcardArgs, DebitArgs, PensionArgs, TransferKind};
use crate::commands::{authed_client, resolve_account};
use crate::format::OutputMode;
use crate::models::{
    CreditCardTransferRequest, DebitTransferRequest, PensionTransferRequest, TransferResponse,
};
use crate::util::format_kr;

pub fn run(kind: TransferKind, mode: OutputMode) -> anyhow::Result<()> {
    match kind {
        TransferKind::Debit(args) => debit(args, mode),
        TransferKind::Creditcard(args) => creditcard(args, mode),
        TransferKind::Pension(args) => pension(args, mode),
    }
}

fn debit(args: DebitArgs, mode: OutputMode) -> anyhow::Result<()> {
    let client = authed_client()?;
    let amount = normalize_amount(&args.amount)?;

    let from = resolve_account(&client, &args.from).context("resolving --from account")?;
    // Own-account only: --to must also be one of the user's accounts.
    let to = resolve_account(&client, &args.to).map_err(|_| {
        anyhow!(
            "destination '{}' is not one of your own accounts. This CLI only \
             performs transfers between your own accounts.",
            args.to
        )
    })?;

    if from.key == to.key {
        bail!("source and destination are the same account");
    }

    let summary = format!(
        "Transfer {amount_disp}\n  from: {from_name} ({from_no})\n    to: {to_name} ({to_no}){msg}{due}",
        amount_disp = format_kr(amount.parse::<f64>().unwrap_or(0.0)),
        from_name = from.name,
        from_no = from.number(),
        to_name = to.name,
        to_no = to.number(),
        msg = args
            .message
            .as_ref()
            .map(|m| format!("\n   msg: {m}"))
            .unwrap_or_default(),
        due = args
            .due_date
            .as_ref()
            .map(|d| format!("\n   due: {d}"))
            .unwrap_or_default(),
    );
    if !confirm(&summary, args.yes)? {
        println!("Aborted.");
        return Ok(());
    }

    let req = DebitTransferRequest {
        amount,
        from_account: from.number_raw(),
        to_account: to.number_raw(),
        message: args.message,
        due_date: args.due_date,
        currency_code: None,
    };
    let resp = client.transfer_debit(&req).context("executing transfer")?;
    report(&resp, mode)
}

fn creditcard(args: CreditcardArgs, mode: OutputMode) -> anyhow::Result<()> {
    let client = authed_client()?;
    let amount = normalize_amount(&args.amount)?;
    let from = resolve_account(&client, &args.from).context("resolving --from account")?;

    let summary = format!(
        "Pay credit card {cc}\n  from: {from_name} ({from_no})\n amount: {amt}",
        cc = args.credit_card_id,
        from_name = from.name,
        from_no = from.number(),
        amt = format_kr(amount.parse::<f64>().unwrap_or(0.0)),
    );
    if !confirm(&summary, args.yes)? {
        println!("Aborted.");
        return Ok(());
    }

    let req = CreditCardTransferRequest {
        amount,
        from_account: from.number_raw(),
        credit_card_account_id: args.credit_card_id,
        due_date: args.due_date,
    };
    let resp = client
        .transfer_creditcard(&req)
        .context("executing credit card transfer")?;
    report(&resp, mode)
}

fn pension(args: PensionArgs, mode: OutputMode) -> anyhow::Result<()> {
    let client = authed_client()?;
    let amount = normalize_amount(&args.amount)?;
    let from = resolve_account(&client, &args.from).context("resolving --from account")?;

    let summary = format!(
        "Transfer to pension policy {pol}\n  from: {from_name} ({from_no})\n amount: {amt}",
        pol = args.policy_number,
        from_name = from.name,
        from_no = from.number(),
        amt = format_kr(amount.parse::<f64>().unwrap_or(0.0)),
    );
    if !confirm(&summary, args.yes)? {
        println!("Aborted.");
        return Ok(());
    }

    let req = PensionTransferRequest {
        amount,
        from_account: from.number_raw(),
        policy_number: args.policy_number,
        due_date: args.due_date,
    };
    let resp = client
        .transfer_pension(&req)
        .context("executing pension transfer")?;
    report(&resp, mode)
}

/// Normalise an amount string to the API's decimal form ("250.50").
/// Accepts comma or dot decimals and spaces; rejects non-positive values.
fn normalize_amount(input: &str) -> anyhow::Result<String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replace(',', ".");
    let value: f64 = cleaned
        .parse()
        .map_err(|_| anyhow!("invalid amount '{input}'"))?;
    if value <= 0.0 {
        bail!("amount must be positive");
    }
    Ok(format!("{value:.2}"))
}

/// Show a summary and ask for confirmation unless `assume_yes`.
fn confirm(summary: &str, assume_yes: bool) -> anyhow::Result<bool> {
    println!("{summary}\n");
    if assume_yes {
        return Ok(true);
    }
    print!("Proceed? [y/N] ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn report(resp: &TransferResponse, mode: OutputMode) -> anyhow::Result<()> {
    if mode == OutputMode::Json {
        return crate::format::print_json(resp);
    }
    match &resp.payment_id {
        Some(id) => println!("✅ Transfer submitted. Payment id: {id}"),
        None => println!("✅ Transfer submitted."),
    }
    if !resp.warnings.is_empty() {
        println!("⚠ Warnings: {}", serde_json::to_string(&resp.warnings)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_amount;

    #[test]
    fn accepts_plain_integer() {
        assert_eq!(normalize_amount("250").unwrap(), "250.00");
    }

    #[test]
    fn accepts_dot_decimal() {
        assert_eq!(normalize_amount("250.50").unwrap(), "250.50");
    }

    #[test]
    fn accepts_norwegian_comma_decimal() {
        assert_eq!(normalize_amount("250,50").unwrap(), "250.50");
    }

    #[test]
    fn strips_whitespace_including_thousands_spaces() {
        assert_eq!(normalize_amount(" 1 234,5 ").unwrap(), "1234.50");
    }

    #[test]
    fn rejects_zero() {
        let err = normalize_amount("0").unwrap_err();
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn rejects_negative() {
        let err = normalize_amount("-5").unwrap_err();
        assert!(err.to_string().contains("positive"));
    }

    #[test]
    fn rejects_non_numeric() {
        let err = normalize_amount("abc").unwrap_err();
        assert!(err.to_string().contains("invalid amount"));
    }
}
