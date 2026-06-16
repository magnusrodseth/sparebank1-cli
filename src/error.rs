//! Error types for the SpareBank 1 CLI.

use thiserror::Error;

/// Errors raised by the client and auth layers.
///
/// Application glue (`main`, command handlers) uses [`anyhow::Result`]; this enum
/// is for the cases where callers need to branch on the *kind* of failure (e.g.
/// "token expired, try a refresh" vs "client secret rejected, tell the user to
/// rotate it").
#[derive(Debug, Error)]
pub enum Sb1Error {
    /// No usable token on disk/keychain, the user must run `sb1 login`.
    #[error("not logged in, run `sb1 login` first")]
    NotAuthenticated,

    /// The client secret was rejected (`invalid_client`). Per terms §3 the
    /// secret has limited validity, so this most often means it has expired and
    /// must be rotated.
    #[error(
        "client credentials were rejected by SpareBank 1 (invalid_client).\n\
         The client secret has limited validity (terms §3); it has most likely \
         expired.\n\
         Rotate it at https://developer.sparebank1.no and run `sb1 login` again."
    )]
    InvalidClientCredentials,

    /// The stored login expired (`invalid_grant`): the refresh token is no
    /// longer valid (used, revoked, or aged out). Distinct from a bad client
    /// secret — the fix is to log in again, not to rotate the secret.
    #[error("your saved login has expired (invalid_grant). Run `sb1 login` again.")]
    SessionExpired,

    /// The API enforced a rate limit (HTTP 429). We do not circumvent limits
    /// (terms §6); the caller should back off and try again later.
    #[error("rate limited by SpareBank 1 (HTTP 429){}; please wait and try again. This tool does not bypass rate limits (terms §6)", retry_after.map(|s| format!(", retry after {s}s")).unwrap_or_default())]
    RateLimited { retry_after: Option<u64> },

    /// A structured error response from the API (the `errors` array).
    #[error("SpareBank 1 API error (HTTP {status}): {message}")]
    Api { status: u16, message: String },

    /// The local OAuth callback flow failed (timeout, state mismatch, etc.).
    #[error("BankID login flow failed: {0}")]
    AuthFlow(String),

    /// Keychain access failed.
    #[error("keychain error: {0}")]
    Keyring(#[from] keyring::Error),

    /// Network/transport error.
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON (de)serialisation error.
    #[error("could not parse response: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Sb1Error>;
