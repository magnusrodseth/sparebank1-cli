//! Serde models for the SpareBank 1 personal banking API.
//!
//! Field names follow the API's camelCase via `rename_all`. Everything is
//! `Option`/defaulted defensively: the documented schemas mark few fields
//! required, and the bank versions media types (v1/v4/v5), so we tolerate
//! missing fields rather than fail a whole command on one absent attribute.
//!
//! Some fields are deserialized to mirror the API schema even when no command
//! renders them yet; `dead_code` is allowed module-wide for that reason.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Response envelope for `GET /accounts`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsResponse {
    #[serde(default)]
    pub accounts: Vec<Account>,
}

/// A single account. The accounts spec doesn't pin a response schema, so these
/// fields come from the live v1 payload and the reference client.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub name: String,
    /// Bare account number, e.g. "12345678903".
    #[serde(default)]
    pub account_number: Option<String>,
    #[serde(default)]
    pub iban: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub balance: Option<f64>,
    #[serde(default)]
    pub available_balance: Option<f64>,
    #[serde(default)]
    pub currency_code: Option<String>,
    #[serde(default, rename = "type")]
    pub account_type: Option<String>,
    #[serde(default)]
    pub product_type: Option<String>,
    #[serde(default)]
    pub owner: Option<Owner>,
}

impl Account {
    /// Best available balance figure for display.
    pub fn display_balance(&self) -> f64 {
        self.available_balance.or(self.balance).unwrap_or(0.0)
    }

    pub fn currency(&self) -> &str {
        self.currency_code.as_deref().unwrap_or("NOK")
    }

    /// Account number formatted Norwegian-style (XXXX.XX.XXXXX) for display.
    pub fn number(&self) -> String {
        let raw = self.number_raw();
        if raw.len() == 11 {
            format!("{}.{}.{}", &raw[0..4], &raw[4..6], &raw[6..11])
        } else {
            raw
        }
    }

    /// Bare digits of the account number, for transfer request bodies.
    pub fn number_raw(&self) -> String {
        self.account_number
            .as_deref()
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub ssn_or_org_number: Option<String>,
}

/// Roles on an account (`GET /accounts/{key}/roles`). Schema is loose, so we
/// keep the raw JSON value for display.
pub type Roles = serde_json::Value;

/// Response envelope for `GET /transactions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionsResponse {
    #[serde(default)]
    pub transactions: Vec<Transaction>,
    /// Partial-failure markers (e.g. `CREDIT_ACCOUNTS_FAILED`).
    #[serde(default)]
    pub errors: Vec<String>,
}

/// A transaction (`TransactionDTO`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    #[serde(default)]
    pub id: Option<String>,
    /// Date as milliseconds since the Unix epoch.
    #[serde(default)]
    pub date: Option<i64>,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cleaned_description: Option<String>,
    #[serde(default)]
    pub type_code: Option<String>,
    #[serde(default)]
    pub type_text: Option<String>,
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default)]
    pub account_key: Option<String>,
    #[serde(default)]
    pub currency_code: Option<String>,
    #[serde(default)]
    pub booking_status: Option<String>,
    #[serde(default)]
    pub remote_account_number: Option<String>,
    #[serde(default)]
    pub remote_account_name: Option<String>,
    #[serde(default)]
    pub kid_or_message: Option<String>,
    #[serde(default)]
    pub merchant: Option<Merchant>,
    #[serde(default)]
    pub source: Option<String>,
}

impl Transaction {
    pub fn date_str(&self) -> String {
        self.date
            .map(crate::util::ms_epoch_to_date)
            .unwrap_or_default()
    }

    pub fn best_description(&self) -> String {
        self.cleaned_description
            .clone()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| self.description.clone())
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    pub fn amount_value(&self) -> f64 {
        self.amount.unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Merchant {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub postcode: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
}

/// Account balance response (`POST /accounts/balance`). Loose schema.
pub type Balance = serde_json::Value;

/// Request body for `POST /transfer/debit` (own-account / domestic payment).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebitTransferRequest {
    /// Amount as a decimal string, e.g. "12.32".
    pub amount: String,
    pub from_account: String,
    pub to_account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency_code: Option<String>,
}

/// Request body for `POST /transfer/creditcard/transferTo`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditCardTransferRequest {
    pub amount: String,
    pub from_account: String,
    pub credit_card_account_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
}

/// Request body for `POST /transfer/pension`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PensionTransferRequest {
    pub amount: String,
    pub from_account: String,
    pub policy_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
}

/// Successful transfer response.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResponse {
    #[serde(default)]
    pub payment_id: Option<String>,
    #[serde(default)]
    pub warnings: Vec<serde_json::Value>,
}

/// Error envelope used by the transfer API (and others) on 4xx/5xx.
#[derive(Debug, Deserialize)]
pub struct ApiErrorBody {
    #[serde(default)]
    pub errors: Vec<ApiErrorItem>,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrorItem {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

impl ApiErrorBody {
    /// Flatten the error list into a single human-readable string.
    pub fn human(&self) -> String {
        if self.errors.is_empty() {
            return "unknown error".to_string();
        }
        self.errors
            .iter()
            .map(|e| {
                let msg = e
                    .message
                    .clone()
                    .or_else(|| e.detail.clone())
                    .unwrap_or_default();
                match &e.code {
                    Some(c) if !msg.is_empty() => format!("[{c}] {msg}"),
                    Some(c) => format!("[{c}]"),
                    None => msg,
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}
