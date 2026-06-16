//! HTTP client for the SpareBank 1 personal banking API.
//!
//! One request per call, no aggressive retries (terms §6). HTTP 429 is surfaced
//! as [`Sb1Error::RateLimited`] with any `Retry-After`; we never loop to bypass
//! a limit. An honest `User-Agent` is sent so the bank can attribute traffic
//! (terms §5.1).

use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{ACCEPT, RETRY_AFTER, USER_AGENT};
use serde::Serialize;

use crate::error::{Result, Sb1Error};
use crate::models::*;

const BASE: &str = "https://api.sparebank1.no/personal/banking";
const HELLOWORLD: &str = "https://api.sparebank1.no/common/helloworld";
const ACCEPT_V1: &str = "application/vnd.sparebank1.v1+json; charset=utf-8";
const UA: &str = concat!(
    "sparebank1-cli/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/magnusrodseth/sparebank1-cli)"
);

/// Build a bare blocking HTTP client with our identifying User-Agent.
pub fn http_agent() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(UA)
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

/// Authenticated API client.
pub struct ApiClient {
    http: Client,
    token: String,
}

impl ApiClient {
    pub fn new(access_token: String) -> Result<Self> {
        Ok(Self {
            http: http_agent()?,
            token: access_token,
        })
    }

    fn get(&self, url: &str) -> RequestBuilder {
        self.http
            .get(url)
            .header(ACCEPT, ACCEPT_V1)
            .bearer_auth(&self.token)
            .header(USER_AGENT, UA)
    }

    fn post_json<T: Serialize>(&self, url: &str, body: &T) -> RequestBuilder {
        self.http
            .post(url)
            .header(ACCEPT, ACCEPT_V1)
            .header(reqwest::header::CONTENT_TYPE, ACCEPT_V1)
            .bearer_auth(&self.token)
            .header(USER_AGENT, UA)
            .json(body)
    }

    // ---- common ---------------------------------------------------------

    /// `GET /common/helloworld`, verifies authentication end to end.
    pub fn hello(&self) -> Result<String> {
        let resp = self.get(HELLOWORLD).send()?;
        let resp = check(resp)?;
        let v: serde_json::Value = resp.json()?;
        Ok(v.get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("(no message)")
            .to_string())
    }

    // ---- accounts -------------------------------------------------------

    /// `GET /accounts`, with optional include-type toggles.
    pub fn accounts(&self, opts: &AccountListOpts) -> Result<Vec<Account>> {
        let mut req = self.get(&format!("{BASE}/accounts"));
        let mut q: Vec<(&str, &str)> = Vec::new();
        if opts.include_credit_cards {
            q.push(("includeCreditCardAccounts", "true"));
        }
        if opts.include_bsu {
            q.push(("includeBsuAccounts", "true"));
        }
        if opts.include_ask {
            q.push(("includeAskAccounts", "true"));
        }
        if opts.include_pension {
            q.push(("includePensionAccounts", "true"));
        }
        if opts.include_currency {
            q.push(("includeCurrencyAccounts", "true"));
        }
        if !q.is_empty() {
            req = req.query(&q);
        }
        let resp = check(req.send()?)?;
        let body: AccountsResponse = resp.json()?;
        Ok(body.accounts)
    }

    /// `GET /accounts/{accountKey}`.
    pub fn account(&self, key: &str) -> Result<Account> {
        let resp = check(self.get(&format!("{BASE}/accounts/{key}")).send()?)?;
        Ok(resp.json()?)
    }

    /// `GET /accounts/{accountKey}/details`, raw JSON (loose schema).
    pub fn account_details(&self, key: &str) -> Result<serde_json::Value> {
        let resp = check(self.get(&format!("{BASE}/accounts/{key}/details")).send()?)?;
        Ok(resp.json()?)
    }

    /// `GET /accounts/{accountKey}/roles`.
    pub fn account_roles(&self, key: &str) -> Result<Roles> {
        let resp = check(self.get(&format!("{BASE}/accounts/{key}/roles")).send()?)?;
        Ok(resp.json()?)
    }

    /// `POST /accounts/balance`, balance by account number.
    pub fn balance(&self, account_number: &str) -> Result<Balance> {
        let body = serde_json::json!({ "accountNumber": account_number });
        let resp = check(
            self.post_json(&format!("{BASE}/accounts/balance"), &body)
                .send()?,
        )?;
        Ok(resp.json()?)
    }

    // ---- transactions ---------------------------------------------------

    /// `GET /transactions`, or `/transactions/classified` when `opts.classified`
    /// is set. The classified endpoint returns a different envelope (each row is
    /// wrapped with the bank's category/recurring/subscription); we flatten it
    /// back into [`TransactionsResponse`] with those fields populated.
    pub fn transactions(&self, opts: &TxnQuery) -> Result<TransactionsResponse> {
        let path = if opts.classified {
            "transactions/classified"
        } else {
            "transactions"
        };
        let mut req = self.get(&format!("{BASE}/{path}"));
        let mut q: Vec<(&str, String)> = Vec::new();
        for key in &opts.account_keys {
            q.push(("accountKey", key.clone()));
        }
        if let Some(f) = &opts.from_date {
            q.push(("fromDate", f.clone()));
        }
        if let Some(t) = &opts.to_date {
            q.push(("toDate", t.clone()));
        }
        if let Some(l) = opts.row_limit {
            q.push(("rowLimit", l.to_string()));
        }
        if let Some(s) = &opts.source {
            q.push(("Transaction source", s.clone()));
        }
        req = req.query(&q);
        let resp = check(req.send()?)?;

        if opts.classified {
            let body: ClassifiedResponse = resp.json()?;
            Ok(TransactionsResponse {
                transactions: body
                    .transactions
                    .into_iter()
                    .map(ClassifiedItem::into_transaction)
                    .collect(),
                errors: Vec::new(),
            })
        } else {
            Ok(resp.json()?)
        }
    }

    /// `GET /transactions/{id}/details` (or `.../details/classified`).
    pub fn transaction_details(&self, id: &str, classified: bool) -> Result<serde_json::Value> {
        let path = if classified {
            format!("{BASE}/transactions/{id}/details/classified")
        } else {
            format!("{BASE}/transactions/{id}/details")
        };
        let resp = check(self.get(&path).send()?)?;
        Ok(resp.json()?)
    }

    /// `GET /transactions/export`, server-rendered CSV of booked transactions.
    pub fn transactions_export(
        &self,
        account_key: &str,
        from_date: &str,
        to_date: &str,
        fields: Option<&str>,
    ) -> Result<String> {
        let mut q: Vec<(&str, &str)> = vec![
            ("accountKey", account_key),
            ("fromDate", from_date),
            ("toDate", to_date),
        ];
        if let Some(f) = fields {
            q.push(("fields", f));
        }
        // The export endpoint emits CSV, not the versioned JSON media type, so we
        // override Accept here (the default JSON Accept yields HTTP 406).
        let req = self
            .http
            .get(format!("{BASE}/transactions/export"))
            .header(ACCEPT, "application/csv;charset=UTF-8")
            .bearer_auth(&self.token)
            .header(USER_AGENT, UA)
            .query(&q);
        let resp = check(req.send()?)?;
        Ok(resp.text()?)
    }

    // ---- transfers ------------------------------------------------------

    /// `POST /transfer/debit`, domestic/own-account payment.
    pub fn transfer_debit(&self, req: &DebitTransferRequest) -> Result<TransferResponse> {
        let resp = check(
            self.post_json(&format!("{BASE}/transfer/debit"), req)
                .send()?,
        )?;
        Ok(resp.json()?)
    }

    /// `POST /transfer/creditcard/transferTo`.
    pub fn transfer_creditcard(&self, req: &CreditCardTransferRequest) -> Result<TransferResponse> {
        let resp = check(
            self.post_json(&format!("{BASE}/transfer/creditcard/transferTo"), req)
                .send()?,
        )?;
        Ok(resp.json()?)
    }

    /// `POST /transfer/pension`.
    pub fn transfer_pension(&self, req: &PensionTransferRequest) -> Result<TransferResponse> {
        let resp = check(
            self.post_json(&format!("{BASE}/transfer/pension"), req)
                .send()?,
        )?;
        Ok(resp.json()?)
    }
}

/// Options for `GET /accounts`.
#[derive(Debug, Default, Clone)]
pub struct AccountListOpts {
    pub include_credit_cards: bool,
    pub include_bsu: bool,
    pub include_ask: bool,
    pub include_pension: bool,
    pub include_currency: bool,
}

/// Options for `GET /transactions`.
#[derive(Debug, Default, Clone)]
pub struct TxnQuery {
    pub account_keys: Vec<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub row_limit: Option<i64>,
    pub source: Option<String>,
    pub classified: bool,
}

/// Inspect a response: map 429/4xx/5xx to typed errors, pass success through.
fn check(resp: Response) -> Result<Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    if status.as_u16() == 401 {
        // Token rejected. The caller (auth layer) decides whether to refresh.
        return Err(Sb1Error::NotAuthenticated);
    }
    if status.as_u16() == 429 {
        let retry_after = resp
            .headers()
            .get(RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());
        return Err(Sb1Error::RateLimited { retry_after });
    }
    // Try to parse the structured error envelope; fall back to raw text.
    let text = resp.text().unwrap_or_default();
    let message = serde_json::from_str::<ApiErrorBody>(&text)
        .map(|b| b.human())
        .unwrap_or_else(|_| {
            if text.is_empty() {
                status.canonical_reason().unwrap_or("error").to_string()
            } else {
                text
            }
        });
    Err(Sb1Error::Api {
        status: status.as_u16(),
        message,
    })
}
