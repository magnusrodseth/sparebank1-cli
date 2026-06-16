//! SpareBank 1 API terms of use, constraints that shape this client.
//!
//! This tool talks to the personal SpareBank 1 banking API. Usage is governed
//! by the bank's "Vilkår for bruk av SpareBank 1 sine APIer". The clauses below
//! are not legal boilerplate dumped here for show, each maps to a concrete
//! behaviour this CLI is built to honour. Read them before extending the client.
//!
//! - §3: the client secret has **limited validity** ("passord med begrenset
//!   gyldighet"). When it expires the API returns 401/invalid_client; we surface
//!   a clear "rotate your client secret in the developer portal" message instead
//!   of a cryptic auth failure. See [`crate::error::Sb1Error`].
//!
//! - §4.1: access is **strictly personal**. The client id and secret must be
//!   kept confidential and never shared with a third party. We store them in the
//!   chosen secret store (keychain / 1Password / a `0600` file), never logged,
//!   and the `.env` bootstrap file is git-ignored. See [`crate::secrets`].
//!
//! - §4.3: access only **as documented**. We call only the documented endpoints
//!   and parameters (see `docs/api/*.json`, fetched from the dev portal).
//!
//! - §5.1: the bank **monitors** API usage. We send an honest, identifiable
//!   `User-Agent` so traffic from this tool is attributable. See
//!   [`crate::client`].
//!
//! - §5.2 / §7: the user is **personally responsible for protecting** the data
//!   and credentials. Secret files are written `0600` and secrets are redacted
//!   from all output.
//!
//! - §6: the bank **enforces rate limits**, and circumventing them results in
//!   immediate termination of access. This client is a good citizen: it does not
//!   auto-retry aggressively, does not poll in tight loops, and respects
//!   `Retry-After` on HTTP 429. Do not add circumvention logic.

/// Human-readable one-line summary shown by `sb1 status`.
pub const SUMMARY: &str = "Personal use only. Credentials are confidential and \
kept in your chosen secret store. The bank enforces rate limits and monitors \
usage; this tool does not retry aggressively or circumvent limits.";
