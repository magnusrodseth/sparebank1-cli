//! Local profile registry. It contains labels and storage references, never secrets.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::secrets;

const REGISTRY_KEY: &str = "profiles";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    /// The upgraded single-user profile continues to use its original entries.
    #[serde(default)]
    pub legacy: bool,
}

impl Profile {
    pub fn credential_key(&self) -> String {
        if self.legacy {
            "client-credentials".into()
        } else {
            format!("profile-{}-client-credentials", encode_name(&self.name))
        }
    }

    pub fn token_key(&self) -> String {
        if self.legacy {
            "oauth-token".into()
        } else {
            format!("profile-{}-oauth-token", encode_name(&self.name))
        }
    }
}

fn encode_name(name: &str) -> String {
    name.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Registry {
    pub default_profile: Option<String>,
    pub profiles: Vec<Profile>,
}

impl Registry {
    pub fn load() -> Result<Self> {
        let _lock = secrets::lock_profiles()?;
        Self::load_locked()
    }

    /// Apply a registry mutation against the latest persisted snapshot.
    /// The lock remains held through the read-modify-write sequence.
    pub fn update<T>(mutate: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let _lock = secrets::lock_profiles()?;
        let mut registry = Self::load_locked()?;
        let result = mutate(&mut registry)?;
        registry.save()?;
        Ok(result)
    }

    fn load_locked() -> Result<Self> {
        if let Some(json) = secrets::kv_get(REGISTRY_KEY)? {
            return Ok(serde_json::from_str(&json)?);
        }

        // Read both legacy entries before writing the registry. A failed read or
        // parse leaves the old installation untouched and migration retryable.
        let credentials = secrets::load_credentials()?;
        let token = secrets::load_token()?;
        let registry = if credentials.is_some() || token.is_some() {
            Self {
                default_profile: Some("default".into()),
                profiles: vec![Profile {
                    name: "default".into(),
                    legacy: true,
                }],
            }
        } else {
            Self::default()
        };
        registry.save()?;
        Ok(registry)
    }

    pub fn save(&self) -> Result<()> {
        secrets::kv_set(REGISTRY_KEY, &serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Result<Profile> {
        self.profiles
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .ok_or_else(|| {
                anyhow!("profile '{name}' does not exist; run `sb1 status` to list profiles")
            })
    }

    pub fn selected_default(&self) -> Result<Profile> {
        let name = self
            .default_profile
            .as_deref()
            .ok_or_else(|| anyhow!("no profile configured; run `sb1 login` first"))?;
        self.get(name)
    }

    pub fn select(
        &self,
        name: Option<&str>,
        require_explicit_if_multiple: bool,
    ) -> Result<Profile> {
        if let Some(name) = name {
            return self.get(name);
        }
        if require_explicit_if_multiple && self.profiles.len() > 1 {
            bail!("multiple profiles configured; select one with --profile <name>");
        }
        self.selected_default()
    }

    pub fn set_default(&mut self, name: &str) -> Result<()> {
        self.get(name)?;
        self.default_profile = Some(name.to_owned());
        Ok(())
    }

    fn rename(&mut self, old_name: &str, new_name: &str) -> Result<Profile> {
        validate_name(new_name)?;
        let old = self.get(old_name)?;
        if old_name == new_name {
            return Ok(old);
        }
        if self.get_optional(new_name).is_some() {
            bail!("profile '{new_name}' already exists");
        }

        let profile = self
            .profiles
            .iter_mut()
            .find(|profile| profile.name == old_name)
            .expect("profile was found above");
        profile.name = new_name.to_owned();
        if self.default_profile.as_deref() == Some(old_name) {
            self.default_profile = Some(new_name.to_owned());
        }
        Ok(old)
    }

    pub fn get_optional(&self, name: &str) -> Option<Profile> {
        self.profiles.iter().find(|p| p.name == name).cloned()
    }

    pub fn add(&mut self, profile: Profile) -> Result<()> {
        validate_name(&profile.name)?;
        if self.get_optional(&profile.name).is_some() {
            bail!("profile '{}' already exists", profile.name);
        }
        self.profiles.push(profile);
        if self.default_profile.is_none() {
            self.default_profile = self.profiles.last().map(|p| p.name.clone());
        }
        Ok(())
    }
}

/// Rename a profile while holding the registry lock through secret migration.
/// Legacy profiles keep their original name-independent secret keys.
pub fn rename_profile(old_name: &str, new_name: &str) -> Result<Option<String>> {
    validate_name(new_name)?;
    let _lock = secrets::lock_profiles()?;
    let mut registry = Registry::load_locked()?;
    let old_profile = registry.rename(old_name, new_name)?;
    if old_name == new_name {
        return Ok(None);
    }

    let new_profile = registry.get(new_name)?;
    if !old_profile.legacy {
        secrets::copy_profile_secrets(&old_profile, &new_profile)
            .context("copying profile secrets for rename")?;
    }

    if let Err(error) = registry.save() {
        if !old_profile.legacy {
            if let Err(cleanup) = secrets::delete_profile_secrets(&new_profile) {
                bail!(
                    "saving renamed profile failed: {error}; destination cleanup failed: {cleanup}"
                );
            }
        }
        return Err(error).context("saving renamed profile");
    }

    if !old_profile.legacy {
        if let Err(error) = secrets::delete_profile_secrets(&old_profile) {
            return Ok(Some(error.to_string()));
        }
    }
    Ok(None)
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.trim() != name
        || name.len() > 64
        || name.chars().any(char::is_control)
    {
        bail!(
            "profile name must be 1–64 bytes, without surrounding whitespace or control characters"
        );
    }
    Ok(())
}
