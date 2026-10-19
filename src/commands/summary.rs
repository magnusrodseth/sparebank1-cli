//! `sb1 summary` — a financial overview computed generically from the API.
//!
//! Reports use bank classifications and exclude transfers between included
//! accounts. Combined summaries group monetary metrics by account currency.
//! Single-profile JSON retains the released field names and calculations.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use anyhow::Context;

use crate::client::TxnQuery;
use crate::format::{self, money, OutputMode};
use crate::models::{Account, Transaction};
use crate::util;

pub fn run(
    months: i64,
    mode: OutputMode,
    mask: bool,
    selection: &crate::profiles::ProfileSelection,
) -> anyhow::Result<()> {
    if selection.is_combined() {
        return run_aggregate(months, mode, mask, selection);
    }
    let clients = crate::commands::aggregate::clients_for(selection)?;
    run_single(months, mode, mask, &clients.clients[0].client)
}

fn run_single(
    months: i64,
    mode: OutputMode,
    mask: bool,
    client: &crate::client::ApiClient,
) -> anyhow::Result<()> {
    let months = months.max(1);

    // All account types so net worth and transactions are complete.
    let opts = crate::client::AccountListOpts {
        include_credit_cards: true,
        include_bsu: true,
        include_ask: true,
        include_pension: true,
        include_currency: true,
    };
    let accounts = client.accounts(&opts).context("listing accounts")?;

    let included_numbers = included_account_numbers(&accounts);
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

    let report = build_report(&accounts, &txns, &included_numbers, months, &from, &to);

    match mode {
        OutputMode::Json => report.print_json(),
        OutputMode::Table => {
            report.print_table(mask, None);
            Ok(())
        }
    }
}

fn run_aggregate(
    months: i64,
    mode: OutputMode,
    mask: bool,
    selection: &crate::profiles::ProfileSelection,
) -> anyhow::Result<()> {
    let months = months.max(1);
    let mut clients = crate::commands::aggregate::clients_for(selection)?;
    let entries = clients.fetch_accounts(&crate::commands::accounts::all_accounts_opts());
    let accounts: Vec<Account> = entries.iter().map(|entry| entry.account.clone()).collect();
    let included_numbers = included_account_numbers(&accounts);
    let from = util::days_ago(months * 30);
    let to = util::today();
    let mut txns = Vec::new();

    for entry in &entries {
        if clients.is_rate_limited() {
            break;
        }
        let account_key = match crate::commands::aggregate::account_key(entry) {
            Ok(key) => key.to_owned(),
            Err(error) => {
                let source = entry.profiles.first().cloned().unwrap_or_default();
                clients.add_failure(&source, error);
                continue;
            }
        };
        let query = TxnQuery {
            account_keys: vec![account_key],
            from_date: Some(from.clone()),
            to_date: Some(to.clone()),
            row_limit: None,
            source: Some("ALL".to_string()),
            classified: true,
        };
        let result = clients.first_success(&entry.profiles, |profile_client| match profile_client
            .client
            .transactions(&query)
        {
            Ok(response) => Ok(response),
            Err(classified_error)
                if matches!(
                    classified_error,
                    crate::error::Sb1Error::Api { status: 404, .. }
                ) =>
            {
                let fallback_query = TxnQuery {
                    classified: false,
                    ..query.clone()
                };
                match profile_client.client.transactions(&fallback_query) {
                    Ok(response) => Ok(response),
                    Err(fallback_error) => Err(anyhow::Error::from(fallback_error).context(
                        format!("classified endpoint unavailable ({classified_error})"),
                    )),
                }
            }
            Err(error) => Err(anyhow::Error::from(error)),
        });
        if let Some((source, response)) = result {
            for error in response.errors {
                clients.add_failure(&source, error);
            }
            // A single-account request gives the currency of the posted amount.
            // Do not use the original purchase currency as an exchange rate.
            let currency = entry.account.currency().to_owned();
            txns.extend(
                response
                    .transactions
                    .into_iter()
                    .map(|txn| (currency.clone(), txn)),
            );
        }
    }

    clients.report_failures();
    let reports = combined_reports(&accounts, &txns, &included_numbers, months, &from, &to);
    match mode {
        OutputMode::Json => {
            let mut balances = BTreeMap::new();
            let mut positive = BTreeMap::new();
            let mut negative = BTreeMap::new();
            let mut flows = BTreeMap::new();
            for (currency, report) in &reports {
                balances.insert(
                    currency,
                    report.net_worth.get(currency).copied().unwrap_or(0.0),
                );
                positive.insert(currency, report.assets);
                negative.insert(currency, report.liabilities);
                let mut flow = report.json_value();
                let fields = flow.as_object_mut().expect("report object");
                for field in ["period", "netWorth", "assets", "liabilities"] {
                    fields.remove(field);
                }
                flows.insert(currency, flow);
            }
            format::print_json(&serde_json::json!({
                "period": {"months": months, "from": from, "to": to},
                "balancesByCurrency": balances,
                "positiveBalancesByCurrency": positive,
                "negativeBalancesByCurrency": negative,
                "cashFlowByCurrency": flows,
                "accounts": entries.iter().map(|entry| serde_json::json!({
                    "key": entry.account.key,
                    "name": entry.account.name,
                    "currency": entry.account.currency(),
                    "profiles": entry.profiles,
                    "shared": entry.profiles.len() > 1,
                })).collect::<Vec<_>>(),
                "complete": clients.complete(),
                "profiles": clients.profiles,
                "errors": clients.failures_json(),
            }))
        }
        OutputMode::Table => {
            println!(
                "Combined account summary across profiles: {}",
                clients.profiles.join(", ")
            );
            if !clients.complete() {
                println!("⚠ Combined account summary is incomplete; see profile errors on stderr.");
            }
            println!("Accessible accounts");
            format::aggregate_accounts_table(&entries, mask);
            for (currency, report) in &reports {
                report.print_table(mask, Some(currency));
            }
            Ok(())
        }
    }
}

fn combined_reports(
    accounts: &[Account],
    transactions: &[(String, Transaction)],
    included_numbers: &HashSet<String>,
    months: i64,
    from: &str,
    to: &str,
) -> BTreeMap<String, Report> {
    let mut currencies: BTreeSet<_> = accounts.iter().map(|a| a.currency().to_owned()).collect();
    currencies.extend(transactions.iter().map(|(currency, _)| currency.clone()));
    currencies
        .into_iter()
        .map(|currency| {
            let accounts: Vec<_> = accounts
                .iter()
                .filter(|a| a.currency() == currency)
                .cloned()
                .collect();
            let txns: Vec<_> = transactions
                .iter()
                .filter(|(ccy, _)| *ccy == currency)
                .map(|(_, txn)| txn.clone())
                .collect();
            let report = build_report(&accounts, &txns, included_numbers, months, from, to);
            (currency, report)
        })
        .collect()
}

/// Digits of every included account number and IBAN, for transfer detection.
fn included_account_numbers(accounts: &[Account]) -> HashSet<String> {
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

fn is_between_included_accounts(t: &Transaction, included: &HashSet<String>) -> bool {
    match &t.remote_account_number {
        Some(n) => {
            let d: String = n.chars().filter(|c| c.is_ascii_digit()).collect();
            !d.is_empty() && included.contains(&d)
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
    included: &HashSet<String>,
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
        if is_between_included_accounts(t, included) {
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

    fn print_table(&self, mask: bool, currency: Option<&str>) {
        use crate::util::{kr, MASKED};
        use comfy_table::{Cell, CellAlignment};
        let amount = |value| match currency {
            Some(ccy) => money(value, ccy, mask),
            None => kr(value, mask),
        };
        // Counterparty/subscription names are personal, so they are masked too;
        // category labels, dates and month labels are not.
        let name = |s: &str| {
            if mask {
                MASKED.to_string()
            } else {
                s.to_string()
            }
        };
        let title = if currency.is_some() {
            "Combined account summary"
        } else {
            "Financial summary"
        };
        println!(
            "{title}  ({} month(s): {} → {})\n",
            self.months, self.from, self.to
        );

        // Net worth
        println!(
            "{}",
            if currency.is_some() {
                "Account balances"
            } else {
                "Net worth"
            }
        );
        for (ccy, total) in &self.net_worth {
            println!("  {ccy}: {}", money(*total, ccy, mask));
        }
        println!(
            "  {} {}   {} {}",
            if currency.is_some() {
                "positive balances"
            } else {
                "assets"
            },
            amount(self.assets),
            if currency.is_some() {
                "negative balances"
            } else {
                "liabilities"
            },
            amount(self.liabilities)
        );

        // Cash flow
        let net = self.income + self.spending;
        println!(
            "\nCash flow ({})",
            if currency.is_some() {
                "transfers between included accounts excluded"
            } else {
                "internal transfers excluded"
            }
        );
        println!("  income    {:>14}", amount(self.income));
        println!("  spending  {:>14}", amount(self.spending));
        println!("  net       {:>14}", amount(net));
        if let Some(rate) = self.savings_rate() {
            if mask {
                println!("  savings rate: ***%");
            } else {
                println!("  savings rate: {rate:.0}%");
            }
        }
        println!(
            "  monthly avg: in {}, out {}",
            amount(self.income / self.months as f64),
            amount(self.spending / self.months as f64)
        );

        // Monthly breakdown
        let mut t = base();
        t.set_header(vec!["Month", "In", "Out", "Net"]);
        for (m, (i, o)) in &self.monthly {
            t.add_row(vec![
                Cell::new(m),
                Cell::new(amount(*i)).set_alignment(CellAlignment::Right),
                Cell::new(amount(*o)).set_alignment(CellAlignment::Right),
                Cell::new(amount(i + o)).set_alignment(CellAlignment::Right),
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
                    Cell::new(amount(*v)).set_alignment(CellAlignment::Right),
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
                Cell::new(amount(*v)).set_alignment(CellAlignment::Right),
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
                    Cell::new(amount(*v)).set_alignment(CellAlignment::Right),
                ]);
            }
            println!("\nLikely subscriptions (bank-flagged)");
            println!("{t}");
        }
    }

    fn print_json(&self) -> anyhow::Result<()> {
        format::print_json(&self.json_value())
    }

    fn json_value(&self) -> serde_json::Value {
        let net: f64 = self.income + self.spending;
        serde_json::json!({
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
        })
    }
}

fn base() -> comfy_table::Table {
    let mut t = comfy_table::Table::new();
    t.load_preset(comfy_table::presets::UTF8_FULL)
        .set_content_arrangement(comfy_table::ContentArrangement::Dynamic);
    t
}
