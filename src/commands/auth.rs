//! Auth-related commands: login, logout, status, hello, refresh.

use anyhow::{anyhow, Context};
use chrono::{Local, TimeZone};

use crate::cli::LoginArgs;
use crate::format::OutputMode;
use crate::profiles::{Profile, Registry};
use crate::secrets::{self, ClientCredentials};
use crate::{auth, terms};

/// Resolve explicit credentials, this profile's stored credentials, then env/.env bootstrap values.
fn resolve_credentials(
    args: &LoginArgs,
    profile: Option<&Profile>,
) -> anyhow::Result<ClientCredentials> {
    match (&args.client_id, &args.client_secret) {
        (Some(_), None) | (None, Some(_)) => {
            anyhow::bail!("provide both --client-id and --client-secret")
        }
        _ => {}
    }
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
    // Existing profile credentials take precedence over global bootstrap env vars.
    if let Some(mut creds) = profile
        .map(secrets::load_credentials_for)
        .transpose()?
        .flatten()
    {
        if let Some(r) = &args.redirect_uri {
            creds.redirect_uri = r.clone();
        }
        return Ok(creds);
    }
    // Finally: environment / .env bootstrap, used to configure a new profile.
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

/// Print the active storage backend and every alternative, so the user (and any
/// agent driving the CLI) sees what is supported and how to switch via SB1_STORE.
fn print_storage_overview() {
    eprintln!("Secret storage (choose with the SB1_STORE environment variable):");
    for (name, desc, active) in secrets::backend_options() {
        let marker = if active { "→ active" } else { "        " };
        eprintln!("  {marker}  {name:<9} {desc}");
    }
    eprintln!();
}

pub fn login(args: LoginArgs, selected: Option<&str>) -> anyhow::Result<()> {
    let registry = Registry::load()?;
    let name = selected
        .map(str::to_owned)
        .or_else(|| registry.default_profile.clone())
        .unwrap_or_else(|| "default".to_owned());
    crate::profiles::validate_name(&name)?;
    let existing = registry.get_optional(&name);
    let profile = existing.clone().unwrap_or_else(|| Profile {
        name: name.clone(),
        legacy: false,
    });
    if existing.is_none() && !confirm_new_profile(&name)? {
        println!("Aborted.");
        return Ok(());
    }
    let creds = resolve_credentials(&args, existing.as_ref())?;

    // Show all storage options up front so the choice is informed.
    print_storage_overview();

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

    let token = auth::login(&creds).context("BankID login failed")?;
    if existing.is_none() {
        Registry::update(|latest| {
            if latest.get_optional(&name).is_some() {
                anyhow::bail!("profile '{name}' already exists");
            }
            if !args.no_save_credentials {
                secrets::save_credentials_for(&profile, &creds).context("saving credentials")?;
            }
            secrets::save_token_for(&profile, &token).context("saving token")?;
            latest.add(profile.clone())
        })?;
    } else {
        if !args.no_save_credentials {
            secrets::save_credentials_for(&profile, &creds).context("saving credentials")?;
        }
        secrets::save_token_for(&profile, &token).context("saving token")?;
    }
    let expiry = Local
        .timestamp_opt(token.expires_at, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    println!(
        "✅ Logged in to profile '{}'. Access token valid until {expiry}.",
        name
    );
    if !args.no_save_credentials {
        println!(
            "   Credentials stored via {}, you can delete .env now.",
            secrets::backend_name()
        );
    }
    Ok(())
}

fn confirm_new_profile(name: &str) -> anyhow::Result<bool> {
    use std::io::{self, Write};
    print!("Create profile '{name}'? [y/N] ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}

pub fn logout(all: bool, selected: Option<&str>) -> anyhow::Result<()> {
    let registry = Registry::load()?;
    let profile = registry.select(selected, true)?;
    secrets::delete_token_for(&profile)?;
    if all {
        secrets::delete_credentials_for(&profile)?;
        println!(
            "✅ Logged out profile '{}' and removed stored client credentials.",
            profile.name
        );
    } else {
        println!(
            "✅ Logged out profile '{}' (token removed). Client credentials kept; use --all to remove them.", profile.name
        );
    }
    Ok(())
}

pub fn status(mode: OutputMode, selected: Option<&str>) -> anyhow::Result<()> {
    let registry = Registry::load()?;
    let profiles = if let Some(name) = selected {
        vec![registry.get(name)?]
    } else {
        registry.profiles.clone()
    };
    let statuses = profiles
        .iter()
        .map(|p| profile_status(p, &registry))
        .collect::<anyhow::Result<Vec<_>>>()?;
    if mode == OutputMode::Json {
        // Preserve the single-profile JSON contract for existing consumers.
        let active_status = if selected.is_some() {
            statuses.first()
        } else {
            statuses.iter().find(|status| status["isDefault"] == true)
        };
        return crate::format::print_json(&serde_json::json!({
            "loggedIn": active_status.is_some_and(|status| status["loggedIn"] == true),
            "tokenValid": active_status.is_some_and(|status| status["tokenValid"] == true),
            "expiresAt": active_status.map(|status| status["expiresAt"].clone()),
            "hasStoredCredentials": active_status.is_some_and(|status| status["hasStoredCredentials"] == true),
            "defaultProfile": registry.default_profile,
            "profiles": statuses,
            "storageBackend": secrets::backend_name(),
            "supportedBackends": secrets::backend_options().iter().map(|(name, _, _)| *name).collect::<Vec<_>>(),
        }));
    }

    if statuses.is_empty() {
        println!("No profiles configured. Run `sb1 login` first.");
    }
    for status in &statuses {
        println!(
            "Profile:             {}{}",
            status["name"].as_str().unwrap_or_default(),
            if status["isDefault"] == true {
                " (default)"
            } else {
                ""
            }
        );
        println!("Logged in:           {}", yesno(status["loggedIn"] == true));
        println!(
            "Access token valid:  {}",
            yesno(status["tokenValid"] == true)
        );
        if let Some(e) = status["expiresAt"].as_str() {
            println!("Token expires:       {e}");
        }
        println!(
            "Credentials stored:  {}\n",
            yesno(status["hasStoredCredentials"] == true)
        );
    }
    println!("Storage backend:     {}", secrets::backend_name());
    println!("\nSecret storage options (set SB1_STORE):");
    for (name, desc, active) in secrets::backend_options() {
        let marker = if active { "→ active" } else { "        " };
        println!("  {marker}  {name:<9} {desc}");
    }
    println!("\n{}", terms::SUMMARY);
    Ok(())
}

fn profile_status(profile: &Profile, registry: &Registry) -> anyhow::Result<serde_json::Value> {
    let token = secrets::load_token_for(profile)?;
    let has_creds = secrets::load_credentials_for(profile)?.is_some();
    let expiry_str = token
        .as_ref()
        .and_then(|t| Local.timestamp_opt(t.expires_at, 0).single())
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string());
    Ok(serde_json::json!({
        "name": profile.name,
        "isDefault": registry.default_profile.as_deref() == Some(&profile.name),
        "loggedIn": token.is_some(),
        "tokenValid": token.as_ref().is_some_and(|t| t.is_valid()),
        "expiresAt": expiry_str,
        "hasStoredCredentials": has_creds,
        "legacyContext": profile.legacy,
    }))
}

pub fn hello(selected: Option<&str>, all_profiles: bool, mode: OutputMode) -> anyhow::Result<()> {
    let mut clients = crate::commands::aggregate::clients_for(selected, all_profiles)?;
    if !clients.aggregated {
        let msg = clients.clients[0]
            .client
            .hello()
            .context("hello world request failed")?;
        println!("{msg}");
        return Ok(());
    }

    let mut results = Vec::new();
    let mut failures = Vec::new();
    for entry in &clients.clients {
        if clients.is_rate_limited() {
            break;
        }
        match entry.client.hello() {
            Ok(message) => results.push(serde_json::json!({
                "profile": entry.profile.name,
                "message": message,
            })),
            Err(error) => {
                let rate_limited = matches!(error, crate::error::Sb1Error::RateLimited { .. });
                failures.push((entry.profile.name.clone(), anyhow::Error::from(error)));
                if rate_limited {
                    break;
                }
            }
        }
    }
    for (profile, error) in failures {
        clients.add_api_error(&profile, error);
    }
    clients.report_failures();
    if mode == OutputMode::Json {
        return crate::format::print_json(&serde_json::json!({
            "complete": clients.complete(),
            "profiles": clients.profiles,
            "results": results,
            "errors": clients.failures_json(),
        }));
    }

    for result in results {
        println!("{}: {}", result["profile"], result["message"]);
    }
    Ok(())
}

pub fn refresh(selected: Option<&str>) -> anyhow::Result<()> {
    let registry = Registry::load()?;
    let profile = registry.select(selected, true)?;
    let token = auth::force_refresh(&profile)
        .with_context(|| format!("token refresh failed for profile '{}'", profile.name))?;
    let expiry = Local
        .timestamp_opt(token.expires_at, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    println!(
        "✅ Token refreshed for profile '{}'. Valid until {expiry}.",
        profile.name
    );
    Ok(())
}

fn yesno(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}
