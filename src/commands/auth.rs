//! Auth-related commands: login, logout, status, hello, refresh.

use anyhow::{anyhow, Context};
use chrono::{Local, TimeZone};

use crate::cli::LoginArgs;
use crate::format::OutputMode;
use crate::secrets::{self, ClientCredentials};
use crate::{auth, terms};

/// Resolve client credentials from flags → keychain → env/.env.
fn resolve_credentials(args: &LoginArgs) -> anyhow::Result<ClientCredentials> {
    // Highest priority: explicit flags (also fed by CLIENT_ID/SECRET env).
    if let (Some(id), Some(secret)) = (&args.client_id, &args.client_secret) {
        return Ok(ClientCredentials {
            client_id: id.clone(),
            client_secret: secret.clone(),
            redirect_uri: args
                .redirect_uri
                .clone()
                .unwrap_or_else(secrets::default_redirect),
        });
    }
    // Next: previously stored credentials.
    if let Some(mut creds) = secrets::load_credentials()? {
        if let Some(r) = &args.redirect_uri {
            creds.redirect_uri = r.clone();
        }
        return Ok(creds);
    }
    // Finally: environment / .env bootstrap.
    if let Some(mut creds) = secrets::credentials_from_env() {
        if let Some(r) = &args.redirect_uri {
            creds.redirect_uri = r.clone();
        }
        return Ok(creds);
    }
    Err(anyhow!(
        "no client credentials found.\nProvide --client-id/--client-secret, set \
         CLIENT_ID/CLIENT_SECRET (or add them to a .env file), or run a previous \
         `sb1 login` that saved them."
    ))
}

pub fn login(args: LoginArgs) -> anyhow::Result<()> {
    let creds = resolve_credentials(&args)?;

    // Warn up front if secrets will land on disk (file backend), so the user
    // can ensure the directory is git-ignored and excluded from backups.
    if let Some(dir) = secrets::file_store_dir() {
        eprintln!(
            "⚠ Storing credentials and tokens as files in {}\n\
             \x20 These are SECRETS. Make sure that directory is NOT in a git repo and is\n\
             \x20 excluded from cloud backups. Use `SB1_STORE=keychain` or `SB1_STORE=op`\n\
             \x20 (1Password) to keep them out of plaintext files.\n",
            dir.display()
        );
    }

    // Persist credentials unless told not to. Per terms §4.1 they are
    // confidential — kept in the chosen secret store, never committed.
    if !args.no_save_credentials {
        secrets::save_credentials(&creds).context("saving credentials")?;
    }

    let token = auth::login(&creds).context("BankID login failed")?;
    let expiry = Local
        .timestamp_opt(token.expires_at, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    println!("✅ Logged in. Access token valid until {expiry}.");
    if !args.no_save_credentials {
        println!(
            "   Credentials stored via {} — you can delete .env now.",
            secrets::backend_name()
        );
    }
    Ok(())
}

pub fn logout(all: bool) -> anyhow::Result<()> {
    secrets::delete_token()?;
    if all {
        secrets::delete_credentials()?;
        println!("✅ Logged out and removed stored client credentials.");
    } else {
        println!(
            "✅ Logged out (token removed). Client credentials kept; use --all to remove them."
        );
    }
    Ok(())
}

pub fn status(mode: OutputMode) -> anyhow::Result<()> {
    let token = secrets::load_token()?;
    let has_creds = secrets::load_credentials()?.is_some();

    let (logged_in, valid, expires_at) = match &token {
        Some(t) => (true, t.is_valid(), Some(t.expires_at)),
        None => (false, false, None),
    };
    let expiry_str = expires_at
        .and_then(|e| Local.timestamp_opt(e, 0).single())
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string());

    if mode == OutputMode::Json {
        return crate::format::print_json(&serde_json::json!({
            "loggedIn": logged_in,
            "tokenValid": valid,
            "expiresAt": expiry_str,
            "hasStoredCredentials": has_creds,
            "storageBackend": secrets::backend_name(),
        }));
    }

    println!("Logged in:           {}", yesno(logged_in));
    println!("Access token valid:  {}", yesno(valid));
    if let Some(e) = expiry_str {
        println!("Token expires:       {e}");
    }
    println!("Credentials stored:  {}", yesno(has_creds));
    println!("Storage backend:     {}", secrets::backend_name());
    println!("\n{}", terms::SUMMARY);
    Ok(())
}

pub fn hello() -> anyhow::Result<()> {
    let client = crate::commands::authed_client()?;
    let msg = client.hello().context("hello world request failed")?;
    println!("{msg}");
    Ok(())
}

pub fn refresh() -> anyhow::Result<()> {
    let token = auth::force_refresh().context("token refresh failed")?;
    let expiry = Local
        .timestamp_opt(token.expires_at, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    println!("✅ Token refreshed. Valid until {expiry}.");
    Ok(())
}

fn yesno(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}
