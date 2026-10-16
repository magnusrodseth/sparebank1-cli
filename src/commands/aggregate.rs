//! Shared helpers for opt-in, read-only views across profiles.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use crate::client::AccountListOpts;
use crate::client::ApiClient;
use crate::models::Account;
use crate::profiles::{Profile, Registry};

pub struct ProfileClient {
    pub profile: Profile,
    pub client: ApiClient,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFailure {
    pub profile: String,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
}

pub struct ClientCollection {
    /// Profiles represented by this request, including any that failed.
    pub profiles: Vec<String>,
    pub clients: Vec<ProfileClient>,
    pub failures: Vec<ProfileFailure>,
    pub aggregated: bool,
    rate_limited: bool,
}

pub type ProfileAttempt<T> = Option<(String, T)>;

impl ClientCollection {
    pub fn complete(&self) -> bool {
        self.failures.is_empty()
    }

    pub fn report_failures(&self) {
        for failure in &self.failures {
            eprintln!(
                "⚠ aggregate incomplete: profile '{}': {}",
                failure.profile, failure.error
            );
        }
    }

    pub fn failures_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.failures).expect("profile failures serialize")
    }

    pub fn add_failure(&mut self, profile: &str, error: impl std::fmt::Display) {
        self.failures.push(ProfileFailure {
            profile: profile.to_owned(),
            error: error.to_string(),
            retry_after_seconds: None,
        });
    }

    pub fn is_rate_limited(&self) -> bool {
        self.rate_limited
    }

    pub fn add_api_error(&mut self, profile: &str, error: anyhow::Error) {
        if is_rate_limited(&error) {
            self.rate_limited = true;
        }
        self.failures.push(profile_failure(profile, &error));
    }

    pub fn fetch_accounts(&mut self, opts: &AccountListOpts) -> Vec<AccountEntry> {
        if self.rate_limited {
            return Vec::new();
        }
        let mut lists = Vec::new();
        let mut failures = Vec::new();
        for entry in &self.clients {
            match entry.client.accounts(opts) {
                Ok(accounts) => lists.push((entry.profile.name.clone(), accounts)),
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
            self.add_api_error(&profile, error);
        }
        merge_accounts(lists)
    }

    /// Try profiles known to have access to one account until a request succeeds.
    /// Failed attempts are returned so callers can mark the aggregate incomplete.
    pub fn first_success<T>(
        &mut self,
        profiles: &[String],
        mut operation: impl FnMut(&ProfileClient) -> Result<T>,
    ) -> ProfileAttempt<T> {
        for profile in profiles {
            if self.rate_limited {
                break;
            }
            let Some(client) = self
                .clients
                .iter()
                .find(|client| client.profile.name == *profile)
            else {
                continue;
            };
            match operation(client) {
                Ok(value) => return Some((profile.clone(), value)),
                Err(error) => {
                    let should_try_another_profile = may_try_another_profile(&error);
                    let rate_limited = is_rate_limited(&error);
                    self.failures.push(profile_failure(profile, &error));
                    if rate_limited {
                        self.rate_limited = true;
                        break;
                    }
                    if !should_try_another_profile {
                        break;
                    }
                }
            }
        }
        None
    }
}

/// Only switch profiles when the API says this profile cannot access the
/// resource. Rate limits, server errors, and transport failures must not fan out.
fn may_try_another_profile(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<crate::error::Sb1Error>(),
        Some(crate::error::Sb1Error::Api {
            status: 403 | 404,
            ..
        })
    )
}

fn is_rate_limited(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<crate::error::Sb1Error>(),
        Some(crate::error::Sb1Error::RateLimited { .. })
    )
}

fn profile_failure(profile: &str, error: &anyhow::Error) -> ProfileFailure {
    let retry_after_seconds = match error.downcast_ref::<crate::error::Sb1Error>() {
        Some(crate::error::Sb1Error::RateLimited { retry_after }) => *retry_after,
        _ => None,
    };
    ProfileFailure {
        profile: profile.to_owned(),
        error: format!("{error:#}"),
        retry_after_seconds,
    }
}

/// Resolve the requested profile set and authenticate independently per profile.
/// In aggregate mode, one failed profile does not discard other profiles' data.
pub fn clients_for(selected: Option<&str>, all_profiles: bool) -> Result<ClientCollection> {
    let registry = Registry::load()?;
    if all_profiles {
        if registry.profiles.is_empty() {
            bail!("no profile configured; run sb1 login first");
        }
        let profiles: Vec<String> = registry.profiles.iter().map(|p| p.name.clone()).collect();
        let mut clients = Vec::new();
        let mut failures = Vec::new();
        let mut rate_limited = false;
        for profile in registry.profiles {
            match crate::commands::authed_client_for(&profile) {
                Ok(client) => clients.push(ProfileClient { profile, client }),
                Err(error) => {
                    rate_limited = is_rate_limited(&error);
                    failures.push(profile_failure(&profile.name, &error));
                    if rate_limited {
                        break;
                    }
                }
            }
        }
        return Ok(ClientCollection {
            profiles,
            clients,
            failures,
            aggregated: true,
            rate_limited,
        });
    }

    let profile = registry.select(selected, false)?;
    let client = crate::commands::authed_client_for(&profile)?;
    Ok(ClientCollection {
        profiles: vec![profile.name.clone()],
        clients: vec![ProfileClient { profile, client }],
        failures: Vec::new(),
        aggregated: false,
        rate_limited: false,
    })
}

#[derive(Debug, Clone)]
pub struct AccountEntry {
    pub account: Account,
    /// Every profile that returned this bank account key.
    pub profiles: Vec<String>,
}

pub fn account_key(entry: &AccountEntry) -> Result<&str> {
    if entry.account.key.is_empty() {
        bail!(
            "account '{}' has no bank account key; it cannot be queried in aggregate mode",
            entry.account.name
        );
    }
    Ok(&entry.account.key)
}

/// Combine account lists in registry order, merging only non-empty bank keys.
pub fn merge_accounts(
    lists: impl IntoIterator<Item = (String, Vec<Account>)>,
) -> Vec<AccountEntry> {
    let mut merged: Vec<AccountEntry> = Vec::new();
    let mut keyed = HashMap::new();
    for (profile, accounts) in lists {
        for account in accounts {
            if !account.key.is_empty() {
                if let Some(index) = keyed.get(&account.key).copied() {
                    let entry: &mut AccountEntry = &mut merged[index];
                    if !entry.profiles.contains(&profile) {
                        entry.profiles.push(profile.clone());
                    }
                    continue;
                }
            }
            let index = merged.len();
            if !account.key.is_empty() {
                keyed.insert(account.key.clone(), index);
            }
            merged.push(AccountEntry {
                account,
                profiles: vec![profile.clone()],
            });
        }
    }
    merged
}

/// Resolve an aggregate account reference. Exact keys/numbers/names take
/// precedence; ambiguous names explain how to select a single profile.
pub fn resolve_account(entries: &[AccountEntry], input: &str) -> Result<AccountEntry> {
    if let Some(account) = entries
        .iter()
        .find(|entry| !entry.account.key.is_empty() && entry.account.key == input)
    {
        return Ok(account.clone());
    }

    let digits: String = input.chars().filter(char::is_ascii_digit).collect();
    if !digits.is_empty() {
        let matches: Vec<_> = entries
            .iter()
            .filter(|entry| entry.account.number_raw() == digits)
            .collect();
        if !matches.is_empty() {
            return one_or_ambiguous(input, matches);
        }
    }

    let lower = input.to_lowercase();
    let exact: Vec<_> = entries
        .iter()
        .filter(|entry| entry.account.name.to_lowercase() == lower)
        .collect();
    if !exact.is_empty() {
        return one_or_ambiguous(input, exact);
    }

    let partial: Vec<_> = entries
        .iter()
        .filter(|entry| entry.account.name.to_lowercase().contains(&lower))
        .collect();
    if !partial.is_empty() {
        return one_or_ambiguous(input, partial);
    }

    Err(anyhow!(
        "no account matches '{input}'. Run sb1 accounts to list them."
    ))
}

fn one_or_ambiguous(input: &str, matches: Vec<&AccountEntry>) -> Result<AccountEntry> {
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        many => {
            let names = many
                .iter()
                .map(|entry| {
                    format!(
                        "{} [{}; profile(s): {}]",
                        entry.account.name,
                        entry.account.key,
                        entry.profiles.join(", ")
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(anyhow!(
                "'{input}' is ambiguous; matches: {names}. Use --profile or an exact account key to select one"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(key: &str, name: &str) -> Account {
        serde_json::from_value(serde_json::json!({"key": key, "name": name})).unwrap()
    }

    #[test]
    fn merge_accounts_deduplicates_by_key_and_collects_profiles() {
        let merged = merge_accounts([
            (
                "alice".into(),
                vec![account("SHARED", "Shared"), account("A", "Alice")],
            ),
            (
                "bob".into(),
                vec![account("SHARED", "Shared"), account("B", "Bob")],
            ),
        ]);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].profiles, ["alice", "bob"]);
        assert_eq!(merged[1].profiles, ["alice"]);
        assert_eq!(merged[2].profiles, ["bob"]);
    }

    #[test]
    fn merge_accounts_does_not_deduplicate_missing_keys() {
        let merged = merge_accounts([
            ("alice".into(), vec![account("", "No key")]),
            ("bob".into(), vec![account("", "No key")]),
        ]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn aggregate_account_names_can_be_ambiguous() {
        let entries = merge_accounts([
            ("alice".into(), vec![account("A", "Shared name")]),
            ("bob".into(), vec![account("B", "Shared name")]),
        ]);
        let error = resolve_account(&entries, "Shared name").unwrap_err();
        assert!(error.to_string().contains("Use --profile"));
    }
}
