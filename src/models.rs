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

    // Populated only from the classified endpoint (not in the plain DTO).
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub recurring: Option<bool>,
    #[serde(default)]
    pub subscription: Option<bool>,
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

/// Response envelope for `GET /transactions/classified`. Each item wraps a
/// transaction with the bank's own classification (category, recurring,
/// subscription) instead of the flat shape used by `/transactions`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedResponse {
    #[serde(default)]
    pub transactions: Vec<ClassifiedItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedItem {
    pub transaction: Transaction,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub recurring: bool,
    #[serde(default)]
    pub subscription: bool,
}

/// A bank-assigned category (localised). `main_i18n` is the display label.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub main_i18n: Option<String>,
    #[serde(default)]
    pub sub_i18n: Option<String>,
}

impl ClassifiedItem {
    /// Flatten into a [`Transaction`] enriched with category/recurring/subscription.
    pub fn into_transaction(self) -> Transaction {
        let category = self
            .categories
            .into_iter()
            .find_map(|c| c.main_i18n.or(c.main))
            .filter(|c| !c.is_empty() && c.to_lowercase() != "ukategorisert");
        let mut t = self.transaction;
        t.category = category;
        t.recurring = Some(self.recurring);
        t.subscription = Some(self.subscription);
        t
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn from_json<T: serde::de::DeserializeOwned>(v: serde_json::Value) -> T {
        serde_json::from_value(v).unwrap()
    }

    // ---- Account --------------------------------------------------------

    #[test]
    fn display_balance_prefers_available_then_balance() {
        let a: Account = from_json(serde_json::json!({"balance": 100.0, "availableBalance": 80.0}));
        assert_eq!(a.display_balance(), 80.0);
        let b: Account = from_json(serde_json::json!({"balance": 100.0}));
        assert_eq!(b.display_balance(), 100.0);
        let c: Account = from_json(serde_json::json!({}));
        assert_eq!(c.display_balance(), 0.0);
    }

    #[test]
    fn currency_defaults_to_nok() {
        let a: Account = from_json(serde_json::json!({}));
        assert_eq!(a.currency(), "NOK");
        let b: Account = from_json(serde_json::json!({"currencyCode": "USD"}));
        assert_eq!(b.currency(), "USD");
    }

    #[test]
    fn number_is_formatted_norwegian_style_for_11_digits() {
        let a: Account = from_json(serde_json::json!({"accountNumber": "20853454096"}));
        assert_eq!(a.number(), "2085.34.54096");
    }

    #[test]
    fn number_returns_raw_when_not_eleven_digits() {
        let a: Account = from_json(serde_json::json!({"accountNumber": "1234"}));
        assert_eq!(a.number(), "1234");
    }

    #[test]
    fn number_raw_strips_non_digits() {
        let a: Account = from_json(serde_json::json!({"accountNumber": "2085.34.54096"}));
        assert_eq!(a.number_raw(), "20853454096");
    }

    // ---- Transaction ----------------------------------------------------

    #[test]
    fn date_str_is_empty_without_a_date() {
        let t: Transaction = from_json(serde_json::json!({}));
        assert_eq!(t.date_str(), "");
    }

    #[test]
    fn best_description_prefers_cleaned_then_falls_back_and_trims() {
        let cleaned: Transaction =
            from_json(serde_json::json!({"description": "RAW", "cleanedDescription": "  Clean  "}));
        assert_eq!(cleaned.best_description(), "Clean");
        // Blank cleaned description falls back to the raw description.
        let blank: Transaction =
            from_json(serde_json::json!({"description": "Raw", "cleanedDescription": "   "}));
        assert_eq!(blank.best_description(), "Raw");
    }

    #[test]
    fn amount_value_defaults_to_zero() {
        let t: Transaction = from_json(serde_json::json!({}));
        assert_eq!(t.amount_value(), 0.0);
    }

    // ---- ClassifiedItem -------------------------------------------------

    #[test]
    fn classified_item_picks_localised_category_and_sets_flags() {
        let item: ClassifiedItem = from_json(serde_json::json!({
            "transaction": {"amount": -50.0},
            "categories": [{"main": "GROCERIES", "mainI18n": "Dagligvarer"}],
            "recurring": true,
            "subscription": false,
        }));
        let t = item.into_transaction();
        assert_eq!(t.category.as_deref(), Some("Dagligvarer"));
        assert_eq!(t.recurring, Some(true));
        assert_eq!(t.subscription, Some(false));
    }

    #[test]
    fn classified_item_drops_uncategorised_label() {
        let item: ClassifiedItem = from_json(serde_json::json!({
            "transaction": {},
            "categories": [{"mainI18n": "Ukategorisert"}],
        }));
        assert_eq!(item.into_transaction().category, None);
    }

    // ---- ApiErrorBody ---------------------------------------------------

    #[test]
    fn human_reports_unknown_for_empty_errors() {
        let b = ApiErrorBody { errors: vec![] };
        assert_eq!(b.human(), "unknown error");
    }

    #[test]
    fn human_formats_code_and_message() {
        let b = ApiErrorBody {
            errors: vec![ApiErrorItem {
                code: Some("INSUFFICIENT_FUNDS".into()),
                message: Some("Not enough money".into()),
                detail: None,
            }],
        };
        assert_eq!(b.human(), "[INSUFFICIENT_FUNDS] Not enough money");
    }

    #[test]
    fn human_falls_back_to_detail_and_joins_multiple() {
        let b = ApiErrorBody {
            errors: vec![
                ApiErrorItem {
                    code: None,
                    message: None,
                    detail: Some("just a detail".into()),
                },
                ApiErrorItem {
                    code: Some("E2".into()),
                    message: None,
                    detail: None,
                },
            ],
        };
        assert_eq!(b.human(), "just a detail; [E2]");
    }
}
