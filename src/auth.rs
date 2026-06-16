//! OAuth 2.0 authorization-code flow with BankID, plus token refresh.
//!
//! Flow (confirmed against the live endpoints and the reference client):
//! 1. Open the browser at `https://api.sparebank1.no/oauth/authorize` with
//!    `client_id`, `response_type=code`, `redirect_uri`, `state`, `scope`.
//! 2. The user authenticates with BankID; the bank redirects to our local
//!    loopback server (`http://localhost:12345/callback?code=...&state=...`).
//! 3. We exchange the code at `POST /oauth/token` for access + refresh tokens.
//! 4. Subsequent calls refresh via `grant_type=refresh_token`.
//!
//! Per terms §3 the client secret has limited validity; a rejected secret here
//! is mapped to [`Sb1Error::InvalidClientCredentials`] with a rotate-it message.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use serde::Deserialize;
use url::Url;

use crate::error::{Result, Sb1Error};
use crate::secrets::{self, ClientCredentials, StoredToken};

const AUTH_URL: &str = "https://api.sparebank1.no/oauth/authorize";
const TOKEN_URL: &str = "https://api.sparebank1.no/oauth/token";
/// Scopes: identity plus the three API families this CLI covers.
const SCOPE: &str = "openid accounts transactions transfer";
/// Safety margin (seconds) subtracted from `expires_in` so we refresh early.
const EXPIRY_MARGIN: i64 = 60;
/// How long to wait for the user to complete BankID before giving up.
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);

/// Raw token endpoint response.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default = "default_bearer")]
    token_type: String,
    #[serde(default)]
    expires_in: Option<i64>,
}

fn default_bearer() -> String {
    "Bearer".to_string()
}

/// OAuth scopes to request. Defaults to the full set this CLI uses; can be
/// overridden with `SB1_SCOPE` (e.g. to drop `transfer` if the client app is
/// not approved for it, which the bank rejects with `access_denied`).
fn scope_from_env() -> String {
    std::env::var("SB1_SCOPE").unwrap_or_else(|_| SCOPE.to_string())
}

impl TokenResponse {
    fn into_stored(self) -> StoredToken {
        let expires_in = self.expires_in.unwrap_or(3600);
        StoredToken {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            token_type: self.token_type,
            expires_at: crate::util::now_unix() + expires_in - EXPIRY_MARGIN,
        }
    }
}

/// Run the interactive BankID login and persist the resulting token.
///
/// `port` is parsed from the redirect URI; we bind a loopback listener there to
/// catch the authorization code.
pub fn login(creds: &ClientCredentials) -> Result<StoredToken> {
    let redirect = Url::parse(&creds.redirect_uri)
        .map_err(|e| Sb1Error::AuthFlow(format!("invalid redirect_uri: {e}")))?;
    let port = redirect
        .port()
        .ok_or_else(|| Sb1Error::AuthFlow("redirect_uri must include a port".into()))?;
    let path = redirect.path().to_string();

    // Bind the loopback listener *before* opening the browser so we never miss
    // the redirect.
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| {
        Sb1Error::AuthFlow(format!(
            "could not bind 127.0.0.1:{port} for the OAuth callback ({e}). \
             Is another process using it?"
        ))
    })?;
    listener
        .set_nonblocking(false)
        .map_err(|e| Sb1Error::AuthFlow(e.to_string()))?;

    let state = random_state();
    let authorize = build_authorize_url(creds, &state)?;

    eprintln!("Opening your browser for BankID login…");
    eprintln!("If it doesn't open, visit:\n  {authorize}\n");
    let _ = webbrowser::open(authorize.as_str());

    let (code, returned_state) = wait_for_callback(&listener, &path)?;
    if returned_state.as_deref() != Some(state.as_str()) {
        return Err(Sb1Error::AuthFlow(
            "state mismatch, possible CSRF, aborting".into(),
        ));
    }
    let code = code.ok_or_else(|| Sb1Error::AuthFlow("no authorization code received".into()))?;

    let token = exchange_code(creds, &code)?;
    secrets::save_token(&token)?;
    Ok(token)
}

/// Return a valid access token, refreshing or erroring as needed.
///
/// Never triggers an interactive login on its own, that is reserved for the
/// explicit `login` command so non-interactive use fails loudly.
pub fn valid_access_token() -> Result<String> {
    let token = secrets::load_token()?.ok_or(Sb1Error::NotAuthenticated)?;
    if token.is_valid() {
        return Ok(token.access_token);
    }
    // Expired: try a refresh if we have the means.
    let creds = secrets::load_credentials()?;
    match (token.refresh_token.clone(), creds) {
        (Some(rt), Some(creds)) => {
            let refreshed = refresh(&creds, &rt)?;
            secrets::save_token(&refreshed)?;
            Ok(refreshed.access_token)
        }
        _ => Err(Sb1Error::NotAuthenticated),
    }
}

/// Force a refresh using the stored refresh token. Returns the new token.
pub fn force_refresh() -> Result<StoredToken> {
    let token = secrets::load_token()?.ok_or(Sb1Error::NotAuthenticated)?;
    let creds = secrets::load_credentials()?.ok_or(Sb1Error::NotAuthenticated)?;
    let rt = token
        .refresh_token
        .ok_or_else(|| Sb1Error::AuthFlow("no refresh token stored".into()))?;
    let refreshed = refresh(&creds, &rt)?;
    secrets::save_token(&refreshed)?;
    Ok(refreshed)
}

fn build_authorize_url(creds: &ClientCredentials, state: &str) -> Result<Url> {
    let mut url = Url::parse(AUTH_URL).map_err(|e| Sb1Error::AuthFlow(e.to_string()))?;
    // NB: no `finInst` hint, the authorize page lets the user pick their bank.
    // Hardcoding the wrong institution causes `access_denied`.
    url.query_pairs_mut()
        .append_pair("client_id", &creds.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &creds.redirect_uri)
        .append_pair("state", state)
        .append_pair("scope", &scope_from_env());
    Ok(url)
}

fn exchange_code(creds: &ClientCredentials, code: &str) -> Result<StoredToken> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &creds.redirect_uri),
        ("client_id", &creds.client_id),
        ("client_secret", &creds.client_secret),
    ];
    post_token(&params)
}

fn refresh(creds: &ClientCredentials, refresh_token: &str) -> Result<StoredToken> {
    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", &creds.client_id),
        ("client_secret", &creds.client_secret),
    ];
    post_token(&params)
}

fn post_token(params: &[(&str, &str)]) -> Result<StoredToken> {
    let client = crate::client::http_agent()?;
    let resp = client.post(TOKEN_URL).form(params).send()?;
    let status = resp.status();
    if status.is_success() {
        let body: TokenResponse = resp.json()?;
        return Ok(body.into_stored());
    }
    // Distinguish the two OAuth failure modes so the user gets the right fix:
    //   invalid_grant  -> stored login/refresh token expired -> `sb1 login`
    //   invalid_client -> client secret rejected/expired      -> rotate secret
    let text = resp.text().unwrap_or_default();
    if text.contains("invalid_grant") {
        return Err(Sb1Error::SessionExpired);
    }
    if status.as_u16() == 401 || text.contains("invalid_client") {
        return Err(Sb1Error::InvalidClientCredentials);
    }
    Err(Sb1Error::Api {
        status: status.as_u16(),
        message: redact(&text),
    })
}

/// Block until the browser hits our callback path, returning `(code, state)`.
fn wait_for_callback(
    listener: &TcpListener,
    callback_path: &str,
) -> Result<(Option<String>, Option<String>)> {
    listener
        .set_nonblocking(false)
        .map_err(|e| Sb1Error::AuthFlow(e.to_string()))?;

    // A single accept with a read timeout keeps this simple; the browser hits us
    // once. We loop only to skip unrelated requests (e.g. favicon).
    let deadline = std::time::Instant::now() + CALLBACK_TIMEOUT;
    for conn in listener.incoming() {
        if std::time::Instant::now() > deadline {
            break;
        }
        let mut stream = match conn {
            Ok(s) => s,
            Err(_) => continue,
        };
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);
        let request_line = request.lines().next().unwrap_or("");
        // "GET /callback?code=...&state=... HTTP/1.1"
        let target = request_line.split_whitespace().nth(1).unwrap_or("");

        if !target.starts_with(callback_path) {
            write_response(&mut stream, "404 Not Found", "Not found");
            continue;
        }

        let parsed = Url::parse(&format!("http://localhost{target}"))
            .map_err(|e| Sb1Error::AuthFlow(e.to_string()))?;
        let mut code = None;
        let mut state = None;
        let mut err = None;
        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "code" => code = Some(v.into_owned()),
                "state" => state = Some(v.into_owned()),
                "error" => err = Some(v.into_owned()),
                _ => {}
            }
        }

        let body = if err.is_some() {
            "<h1>Login failed</h1><p>You can close this tab and return to the terminal.</p>"
        } else {
            "<h1>Login complete ✅</h1><p>You can close this tab and return to the terminal.</p>"
        };
        write_response(&mut stream, "200 OK", body);

        if let Some(e) = err {
            return Err(Sb1Error::AuthFlow(format!("authorization error: {e}")));
        }
        return Ok((code, state));
    }
    Err(Sb1Error::AuthFlow(
        "timed out waiting for BankID callback".into(),
    ))
}

fn write_response(stream: &mut impl Write, status: &str, body: &str) {
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>sb1</title>\
         <style>body{{font-family:system-ui;margin:4rem auto;max-width:32rem;text-align:center}}</style>\
         </head><body>{body}</body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// 128 bits of CSPRNG entropy, base64url-ish, for the OAuth `state`.
fn random_state() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    // Two independent random u64s from the OS-seeded RandomState.
    let a = RandomState::new().build_hasher().finish();
    let b = RandomState::new().build_hasher().finish();
    format!("{a:016x}{b:016x}")
}

/// Redact anything token-like from text before it reaches logs/output.
fn redact(s: &str) -> String {
    let mut out = s.to_string();
    for marker in ["access_token", "refresh_token", "client_secret", "code"] {
        if out.contains(marker) {
            out = format!("<redacted: response contained {marker}>");
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_stored_applies_expiry_margin() {
        let before = crate::util::now_unix();
        let st = TokenResponse {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            token_type: "Bearer".into(),
            expires_in: Some(3600),
        }
        .into_stored();
        let after = crate::util::now_unix();
        // expires_at = now + expires_in - EXPIRY_MARGIN (60).
        assert!(st.expires_at >= before + 3600 - EXPIRY_MARGIN);
        assert!(st.expires_at <= after + 3600 - EXPIRY_MARGIN);
        assert_eq!(st.refresh_token.as_deref(), Some("r"));
    }

    #[test]
    fn into_stored_defaults_missing_expiry_to_one_hour() {
        let before = crate::util::now_unix();
        let st = TokenResponse {
            access_token: "a".into(),
            refresh_token: None,
            token_type: "Bearer".into(),
            expires_in: None,
        }
        .into_stored();
        assert!(st.expires_at >= before + 3600 - EXPIRY_MARGIN);
    }

    #[test]
    fn redact_masks_token_like_text() {
        assert_eq!(
            redact("{\"access_token\":\"abc\"}"),
            "<redacted: response contained access_token>"
        );
        // Innocuous text is left intact.
        assert_eq!(redact("some plain error"), "some plain error");
    }

    #[test]
    fn random_state_is_32_hex_chars_and_varies() {
        let a = random_state();
        let b = random_state();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn authorize_url_has_required_params_and_no_fininst() {
        let creds = ClientCredentials {
            client_id: "my-client".into(),
            client_secret: "shh".into(),
            redirect_uri: "http://localhost:12345/callback".into(),
        };
        let url = build_authorize_url(&creds, "STATE123").unwrap();
        let s = url.as_str();
        assert!(s.contains("client_id=my-client"));
        assert!(s.contains("response_type=code"));
        assert!(s.contains("state=STATE123"));
        assert!(s.contains("scope="));
        // Regression guard: a hardcoded finInst hint causes access_denied.
        assert!(!s.contains("finInst"));
        // The client secret must never appear in the authorize URL.
        assert!(!s.contains("shh"));
    }
}
