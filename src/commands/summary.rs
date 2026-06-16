//! `sb1 summary` — a financial overview computed generically from the API.
//!
//! Everything here is derived from data the API returns for any SpareBank 1
//! customer, with no hardcoded merchants or account assumptions:
//!
//! - **Net worth** sums account balances per currency; account `type` decides
//!   asset vs liability (credit cards / loans are negative balances already).
//! - **Internal transfers** between the user's own accounts are excluded from
//!   income/spending by matching a transaction's counterparty number against the
//!   set of the user's own account numbers (and IBANs). This works regardless of
//!   how many accounts the user has or what they are named.
//! - **Categories, recurring and subscription** flags come from the bank's own
//!   classification (`/transactions/classified`), not keyword guesses. If that
//!   endpoint is unavailable we fall back to plain transactions (no categories).

use std::collections::{BTreeMap, HashSet};

use anyhow::Context;

use crate::client::TxnQuery;
use crate::commands::authed_client;
use crate::format::{self, OutputMode};
use crate::models::{Account, Transaction};
use crate::util::{self, format_kr};

pub fn run(months: i64, mode: OutputMode, mask: bool) -> anyhow::Result<()> {
    let months = months.max(1);
    let client = authed_client()?;

    // All account types so net worth and transactions are complete.
    let opts = crate::client::AccountListOpts {
        include_credit_cards: true,
        include_bsu: true,
        include_ask: true,
        include_pension: true,
        include_currency: true,
    };
    let accounts = client.accounts(&opts).context("listing accounts")?;

    let own_numbers = own_account_numbers(&accounts);
    let keys: Vec<String> = accounts.iter().map(|a| a.key.clone()).collect();

    let from = util::days_ago(months * 30);
    let to = util::today();

    // Prefer classified (gives categories + subscription flags); fall back to plain.
    let mut query = TxnQuery {
        account_keys: keys.clone(),
        from_date: Some(from.clone()),
        to_date: Some(to.clone()),
        row_limit: None,
        source: Some("ALL".to_string()),
        classified: true,
    };
    let txns = match client.transactions(&query) {
        Ok(r) => r.transactions,
        Err(_) => {
            query.classified = false;
            client
                .transactions(&query)
                .context("listing transactions")?
                .transactions
        }
    };

    let report = build_report(&accounts, &txns, &own_numbers, months, &from, &to);

    match mode {
        OutputMode::Json => report.print_json(),
        OutputMode::Table => {
            report.print_table(mask);
            Ok(())
        }
    }
}

/// Digits of every account number and IBAN the user owns, for internal-transfer
/// detection.
fn own_account_numbers(accounts: &[Account]) -> HashSet<String> {
    let mut set = HashSet::new();
    for a in accounts {
        let n = a.number_raw();
        if !n.is_empty() {
            set.insert(n);
        }
        if let Some(iban) = &a.iban {
            let d: String = iban.chars().filter(|c| c.is_ascii_digit()).collect();
            if !d.is_empty() {
                set.insert(d);
            }
        }
    }
    set
}

fn is_internal(t: &Transaction, own: &HashSet<String>) -> bool {
    match &t.remote_account_number {
        Some(n) => {
            let d: String = n.chars().filter(|c| c.is_ascii_digit()).collect();
            !d.is_empty() && own.contains(&d)
        }
        None => false,
    }
}

struct Report {
    months: i64,
    from: String,
    to: String,
    net_worth: BTreeMap<String, f64>, // currency -> total
    assets: f64,
    liabilities: f64,
    income: f64,
    spending: f64,
    monthly: BTreeMap<String, (f64, f64)>, // month -> (in, out)
    top_out: Vec<(String, f64)>,
    categories: Vec<(String, f64)>,
    subscriptions: Vec<(String, f64)>,
}

fn build_report(
    accounts: &[Account],
    txns: &[Transaction],
    own: &HashSet<String>,
    months: i64,
    from: &str,
    to: &str,
) -> Report {
    // Net worth per currency, plus asset/liability split.
    let mut net_worth: BTreeMap<String, f64> = BTreeMap::new();
    let (mut assets, mut liabilities) = (0.0, 0.0);
    for a in accounts {
        let bal = a.display_balance();
        *net_worth.entry(a.currency().to_string()).or_default() += bal;
        if bal >= 0.0 {
            assets += bal;
        } else {
            liabilities += bal;
        }
    }

    let mut income = 0.0;
    let mut spending = 0.0;
    let mut monthly: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut out_by_party: BTreeMap<String, f64> = BTreeMap::new();
    let mut by_category: BTreeMap<String, f64> = BTreeMap::new();
    let mut subs: BTreeMap<String, f64> = BTreeMap::new();

    for t in txns {
        if is_internal(t, own) {
            continue;
        }
        let amt = t.amount_value();
        let m = t.date_str();
        let m = if m.len() >= 7 { m[..7].to_string() } else { m };
        let entry = monthly.entry(m).or_default();
        if amt >= 0.0 {
            income += amt;
            entry.0 += amt;
        } else {
            spending += amt;
            entry.1 += amt;
            let party = t
                .remote_account_name
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| t.best_description());
            *out_by_party.entry(clip(&party)).or_default() += amt;
            let cat = t.category.clone().unwrap_or_else(|| "Uncategorized".into());
            *by_category.entry(cat).or_default() += amt;
        }
        // Subscriptions: outgoing charges the bank flagged as recurring spend.
        if t.subscription == Some(true) && amt < 0.0 {
            let name = clip(&t.best_description());
            // keep the largest seen charge as the representative amount
            let e = subs.entry(name).or_insert(0.0);
            if amt.abs() > e.abs() {
                *e = amt;
            }
        }
    }

    let mut top_out: Vec<_> = out_by_party.into_iter().collect();
    top_out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    top_out.truncate(10);

    let mut categories: Vec<_> = by_category.into_iter().collect();
    categories.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    let mut subscriptions: Vec<_> = subs.into_iter().collect();
    subscriptions.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    Report {
        months,
        from: from.to_string(),
        to: to.to_string(),
        net_worth,
        assets,
        liabilities,
        income,
        spending,
        monthly,
        top_out,
        categories,
        subscriptions,
    }
}

fn clip(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 32 {
        format!("{}…", s.chars().take(31).collect::<String>())
    } else {
        s.to_string()
    }
}

impl Report {
    fn savings_rate(&self) -> Option<f64> {
        if self.income > 0.0 {
            Some((self.income + self.spending) / self.income * 100.0)
        } else {
            None
        }
    }

    fn print_table(&self, mask: bool) {
        use crate::util::{kr, MASKED};
        use comfy_table::{Cell, CellAlignment};
        // Counterparty/subscription names are personal, so they are masked too;
        // category labels, dates and month labels are not.
        let name = |s: &str| {
            if mask {
                MASKED.to_string()
            } else {
                s.to_string()
            }
        };
        println!(
            "Financial summary  ({} month(s): {} → {})\n",
            self.months, self.from, self.to
        );

        // Net worth
        println!("Net worth");
        for (ccy, total) in &self.net_worth {
            println!("  {ccy}: {}", money(*total, ccy, mask));
        }
        println!(
            "  assets {}   liabilities {}",
            kr(self.assets, mask),
            kr(self.liabilities, mask)
        );

        // Cash flow
        let net = self.income + self.spending;
        println!("\nCash flow (internal transfers excluded)");
        println!("  income    {:>14}", kr(self.income, mask));
        println!("  spending  {:>14}", kr(self.spending, mask));
        println!("  net       {:>14}", kr(net, mask));
        if let Some(rate) = self.savings_rate() {
            if mask {
                println!("  savings rate: ***%");
            } else {
                println!("  savings rate: {rate:.0}%");
            }
        }
        println!(
            "  monthly avg: in {}, out {}",
            kr(self.income / self.months as f64, mask),
            kr(self.spending / self.months as f64, mask)
        );

        // Monthly breakdown
        let mut t = base();
        t.set_header(vec!["Month", "In", "Out", "Net"]);
        for (m, (i, o)) in &self.monthly {
            t.add_row(vec![
                Cell::new(m),
                Cell::new(kr(*i, mask)).set_alignment(CellAlignment::Right),
                Cell::new(kr(*o, mask)).set_alignment(CellAlignment::Right),
                Cell::new(kr(i + o, mask)).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("\nBy month");
        println!("{t}");

        // Categories
        if self.categories.iter().any(|(c, _)| c != "Uncategorized") {
            let mut t = base();
            t.set_header(vec!["Category", "Spent"]);
            for (c, v) in &self.categories {
                t.add_row(vec![
                    Cell::new(c),
                    Cell::new(kr(*v, mask)).set_alignment(CellAlignment::Right),
                ]);
            }
            println!("\nSpending by category (bank-classified)");
            println!("{t}");
        }

        // Top counterparties
        let mut t = base();
        t.set_header(vec!["Counterparty / description", "Spent"]);
        for (p, v) in &self.top_out {
            t.add_row(vec![
                Cell::new(name(p)),
                Cell::new(kr(*v, mask)).set_alignment(CellAlignment::Right),
            ]);
        }
        println!("\nTop outgoing");
        println!("{t}");

        // Subscriptions
        if !self.subscriptions.is_empty() {
            let mut t = base();
            t.set_header(vec!["Subscription", "Charge"]);
            for (n, v) in &self.subscriptions {
                t.add_row(vec![
                    Cell::new(name(n)),
                    Cell::new(kr(*v, mask)).set_alignment(CellAlignment::Right),
                ]);
            }
            println!("\nLikely subscriptions (bank-flagged)");
            println!("{t}");
        }
    }

    fn print_json(&self) -> anyhow::Result<()> {
        let net: f64 = self.income + self.spending;
        format::print_json(&serde_json::json!({
            "period": { "months": self.months, "from": self.from, "to": self.to },
            "netWorth": self.net_worth,
            "assets": self.assets,
            "liabilities": self.liabilities,
            "income": self.income,
            "spending": self.spending,
            "net": net,
            "savingsRatePct": self.savings_rate(),
            "monthly": self.monthly.iter().map(|(m,(i,o))| serde_json::json!({
                "month": m, "in": i, "out": o, "net": i+o
            })).collect::<Vec<_>>(),
            "topOutgoing": self.top_out.iter().map(|(p,v)| serde_json::json!({"party": p, "amount": v})).collect::<Vec<_>>(),
            "categories": self.categories.iter().map(|(c,v)| serde_json::json!({"category": c, "amount": v})).collect::<Vec<_>>(),
            "subscriptions": self.subscriptions.iter().map(|(n,v)| serde_json::json!({"name": n, "amount": v})).collect::<Vec<_>>(),
        }))
    }
}

fn base() -> comfy_table::Table {
    let mut t = comfy_table::Table::new();
    t.load_preset(comfy_table::presets::UTF8_FULL)
        .set_content_arrangement(comfy_table::ContentArrangement::Dynamic);
    t
}

/// Format money in a currency: NOK uses the kr formatter, others a plain suffix.
fn money(v: f64, ccy: &str, mask: bool) -> String {
    if mask {
        return crate::util::mask_kr(v);
    }
    if ccy == "NOK" {
        format_kr(v)
    } else {
        format!("{v:.2} {ccy}")
    }
}
