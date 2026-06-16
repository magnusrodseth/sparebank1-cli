//! Credential and token storage.
//!
//! Per terms §4.1/§5.2 the client id, client secret and OAuth tokens are
//! confidential and the user is personally responsible for protecting them.
//!
//! Three backends are supported, selected by the `SB1_STORE` environment
//! variable. The default is the OS keychain, so the easy path is also a secure
//! one; plaintext files are an explicit, deliberate opt-in.
//!
//! - **`keychain`** (default): the platform-native secret store (macOS Keychain,
//!   Linux kernel keyutils, Windows Credential Manager). Secure at rest, gated by
//!   your OS. On macOS an unsigned binary re-prompts for the login password after
//!   each rebuild.
//! - **`op`** / **`1password`**: 1Password via the `op` CLI. Items live in a
//!   vault (default `Private`, override with `SB1_OP_VAULT`). Requires `op`
//!   installed and signed in; unlock is enforced by 1Password (biometrics).
//! - **`file`** (opt-in): JSON files under `~/.config/sparebank1-cli/` with
//!   `0600` permissions. No password prompts, so it is the most reliable backend
//!   for headless automation (servers, cron, CI, Docker). The trade-off is
//!   plaintext on disk: keep the directory out of version control and backups.
//!
//! Bootstrap: on first run `login` may read credentials from CLI flags, the
//! environment, or a local `.env` file (git-ignored), then persists them so the
//! `.env` can be deleted afterwards.

use std::collections::HashMap;
use std::path::PathBuf;

use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Keychain "service" namespace for all entries written by this tool.
const SERVICE: &str = "sparebank1-cli";
const ACCT_CREDENTIALS: &str = "client-credentials";
const ACCT_TOKEN: &str = "oauth-token";

/// Storage backend, chosen by `SB1_STORE` (default: keychain).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    File,
    Keychain,
    OnePassword,
}

fn backend() -> Backend {
    match std::env::var("SB1_STORE").as_deref() {
        Ok("file") => Backend::File,
        Ok("op") | Ok("1password") => Backend::OnePassword,
        // keychain is the secure default; anything unset/unknown falls back to it
        // rather than silently writing plaintext.
        _ => Backend::Keychain,
    }
}

/// One-line summary of every supported backend, for setup/status output so the
/// user (and any agent driving the CLI) sees all alternatives. The boolean marks
/// the currently active one.
pub fn backend_options() -> Vec<(&'static str, &'static str, bool)> {
    let active = backend();
    vec![
        (
            "keychain",
            "Platform-native store (macOS Keychain / Linux keyutils / Windows Credential Manager). Default, secure at rest.",
            active == Backend::Keychain,
        ),
        (
            "op",
            "1Password via the `op` CLI. Unlock enforced by 1Password (biometrics).",
            active == Backend::OnePassword,
        ),
        (
            "file",
            "Plaintext JSON in ~/.config/sparebank1-cli (0600). For headless/automation; keep out of git and backups.",
            active == Backend::File,
        ),
    ]
}

/// 1Password vault for the `op` backend.
fn op_vault() -> String {
    std::env::var("SB1_OP_VAULT").unwrap_or_else(|_| "Private".to_string())
}

/// 1Password item title for a logical account name.
fn op_title(account: &str) -> String {
    match account {
        ACCT_TOKEN => "SpareBank1 CLI token".to_string(),
        ACCT_CREDENTIALS => "SpareBank1 CLI credentials".to_string(),
        other => format!("SpareBank1 CLI {other}"),
    }
}

/// `~/.config/sparebank1-cli` (honours `XDG_CONFIG_HOME`).
fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("sparebank1-cli");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config").join("sparebank1-cli")
}

fn file_path(account: &str) -> PathBuf {
    config_dir().join(format!("{account}.json"))
}

/// Read a stored value by logical account name from the active backend.
fn kv_get(account: &str) -> Result<Option<String>> {
    match backend() {
        Backend::File => match std::fs::read_to_string(file_path(account)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(crate::error::Sb1Error::AuthFlow(format!(
                "reading {}: {e}",
                file_path(account).display()
            ))),
        },
        Backend::Keychain => match Entry::new(SERVICE, account)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        },
        Backend::OnePassword => op_read(account),
    }
}

/// Write a value by logical account name to the active backend.
fn kv_set(account: &str, value: &str) -> Result<()> {
    match backend() {
        Backend::File => {
            let dir = config_dir();
            std::fs::create_dir_all(&dir).map_err(|e| {
                crate::error::Sb1Error::AuthFlow(format!("creating {}: {e}", dir.display()))
            })?;
            let path = file_path(account);
            std::fs::write(&path, value).map_err(|e| {
                crate::error::Sb1Error::AuthFlow(format!("writing {}: {e}", path.display()))
            })?;
            set_owner_only(&path)?;
            Ok(())
        }
        Backend::Keychain => Ok(Entry::new(SERVICE, account)?.set_password(value)?),
        Backend::OnePassword => op_write(account, value),
    }
}

/// Delete a stored value; a missing entry is not an error.
fn kv_delete(account: &str) -> Result<()> {
    match backend() {
        Backend::File => match std::fs::remove_file(file_path(account)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(crate::error::Sb1Error::AuthFlow(format!(
                "removing file: {e}"
            ))),
        },
        Backend::Keychain => match Entry::new(SERVICE, account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        },
        Backend::OnePassword => op_delete(account),
    }
}

// ---- 1Password (`op` CLI) backend --------------------------------------

fn op_err(msg: impl std::fmt::Display) -> crate::error::Sb1Error {
    crate::error::Sb1Error::AuthFlow(format!("1Password (op) {msg}"))
}

/// Run `op` with args, returning stdout on success. If `SB1_OP_ACCOUNT` is set
/// (e.g. `my.1password.eu`), it is passed as `--account` to disambiguate when
/// multiple 1Password accounts are configured.
fn op_run(args: &[&str]) -> Result<std::process::Output> {
    let mut cmd = std::process::Command::new("op");
    if let Ok(account) = std::env::var("SB1_OP_ACCOUNT") {
        if !account.is_empty() {
            cmd.arg("--account").arg(account);
        }
    }
    cmd.args(args).output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            op_err("CLI not found, install it from https://developer.1password.com/docs/cli/")
        } else {
            op_err(e)
        }
    })
}

/// Read the `password` field of the item, or `None` if the item is absent.
fn op_read(account: &str) -> Result<Option<String>> {
    let vault = op_vault();
    let title = op_title(account);
    let reference = format!("op://{vault}/{title}/password");
    let out = op_run(&["read", "--no-newline", &reference])?;
    if out.status.success() {
        return Ok(Some(String::from_utf8_lossy(&out.stdout).to_string()));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    // A missing item is not an error, it just means "not stored yet".
    if stderr.contains("isn't an item")
        || stderr.contains("not found")
        || stderr.contains("doesn't exist")
        || stderr.contains("no item")
    {
        return Ok(None);
    }
    Err(op_err(format!("read failed: {}", stderr.trim())))
}

/// Create or update the item holding `value` in its `password` field.
fn op_write(account: &str, value: &str) -> Result<()> {
    let vault = op_vault();
    let title = op_title(account);

    // Does the item already exist?
    let exists = op_run(&["item", "get", &title, "--vault", &vault])?
        .status
        .success();

    let assignment = format!("password={value}");
    let out = if exists {
        op_run(&["item", "edit", &title, "--vault", &vault, &assignment])?
    } else {
        op_run(&[
            "item",
            "create",
            "--category",
            "Password",
            "--title",
            &title,
            "--vault",
            &vault,
            &assignment,
        ])?
    };
    if out.status.success() {
        Ok(())
    } else {
        Err(op_err(format!(
            "write failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

fn op_delete(account: &str) -> Result<()> {
    let vault = op_vault();
    let title = op_title(account);
    let out = op_run(&["item", "delete", &title, "--vault", &vault])?;
    // Treat "already gone" as success.
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("isn't an item") || stderr.contains("not found") {
        return Ok(());
    }
    Err(op_err(format!("delete failed: {}", stderr.trim())))
}

/// Restrict a secret file to `0600` (owner read/write only).
#[cfg(unix)]
fn set_owner_only(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
        crate::error::Sb1Error::AuthFlow(format!("setting permissions on {}: {e}", path.display()))
    })
}

#[cfg(not(unix))]
fn set_owner_only(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

/// OAuth client credentials issued by developer.sparebank1.no.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
    /// Registered redirect URI. Must match the value configured for the app.
    #[serde(default = "default_redirect")]
    pub redirect_uri: String,
}

pub fn default_redirect() -> String {
    "http://localhost:12345/callback".to_string()
}

/// A stored OAuth token plus the absolute expiry we computed at fetch time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default = "default_token_type")]
    pub token_type: String,
    /// Unix epoch seconds at which the access token should be considered expired
    /// (we subtract a small safety margin when storing).
    pub expires_at: i64,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

impl StoredToken {
    /// True if the access token is still within its validity window.
    pub fn is_valid(&self) -> bool {
        crate::util::now_unix() < self.expires_at
    }
}

/// Persist client credentials.
pub fn save_credentials(creds: &ClientCredentials) -> Result<()> {
    kv_set(ACCT_CREDENTIALS, &serde_json::to_string_pretty(creds)?)
}

/// Load client credentials, if present.
pub fn load_credentials() -> Result<Option<ClientCredentials>> {
    match kv_get(ACCT_CREDENTIALS)? {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

/// Persist an OAuth token.
pub fn save_token(token: &StoredToken) -> Result<()> {
    kv_set(ACCT_TOKEN, &serde_json::to_string_pretty(token)?)
}

/// Load the stored OAuth token, if present.
pub fn load_token() -> Result<Option<StoredToken>> {
    match kv_get(ACCT_TOKEN)? {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

/// Remove the stored token (used by `logout`). Missing entry is not an error.
pub fn delete_token() -> Result<()> {
    kv_delete(ACCT_TOKEN)
}

/// Remove stored client credentials (used by `logout --all`).
pub fn delete_credentials() -> Result<()> {
    kv_delete(ACCT_CREDENTIALS)
}

/// Returns the on-disk storage directory when the file backend is active, so
/// the CLI can warn the user to keep these secret files out of version control
/// and backups. Returns `None` when the keychain backend is in use.
pub fn file_store_dir() -> Option<PathBuf> {
    match backend() {
        Backend::File => Some(config_dir()),
        Backend::Keychain | Backend::OnePassword => None,
    }
}

/// Human-readable name of the active backend (for status output).
pub fn backend_name() -> &'static str {
    match backend() {
        Backend::File => "file (~/.config/sparebank1-cli)",
        Backend::Keychain => "keychain",
        Backend::OnePassword => "1password (op)",
    }
}

/// Read credentials from the environment, falling back to a `.env` file in the
/// current directory. Accepts both the bare names the user put in `.env`
/// (`CLIENT_ID`, `CLIENT_SECRET`, `REDIRECT_URL`) and `SB1_`-prefixed variants.
///
/// Returns `None` unless at least a client id and secret are found.
pub fn credentials_from_env() -> Option<ClientCredentials> {
    let mut env: HashMap<String, String> = std::env::vars().collect();
    // Merge in .env (process env wins over the file).
    if let Ok(contents) = std::fs::read_to_string(".env") {
        for (k, v) in parse_dotenv(&contents) {
            env.entry(k).or_insert(v);
        }
    }

    let pick = |keys: &[&str]| -> Option<String> { keys.iter().find_map(|k| env.get(*k).cloned()) };

    let client_id = pick(&["SB1_CLIENT_ID", "CLIENT_ID"])?;
    let client_secret = pick(&["SB1_CLIENT_SECRET", "CLIENT_SECRET"])?;
    let redirect_uri = pick(&["SB1_REDIRECT_URI", "REDIRECT_URL", "REDIRECT_URI"])
        .unwrap_or_else(default_redirect);

    Some(ClientCredentials {
        client_id,
        client_secret,
        redirect_uri,
    })
}

/// Minimal `.env` parser: `KEY=VALUE` per line, `#` comments, optional quotes.
fn parse_dotenv(contents: &str) -> Vec<(String, String)> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (k, v) = line.split_once('=')?;
            let v = v.trim();
            let v = v
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(v);
            Some((k.trim().to_string(), v.to_string()))
        })
        .collect()
}
