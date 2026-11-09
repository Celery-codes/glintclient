use std::{collections::HashMap, sync::Mutex};

use serde::{Deserialize, Serialize};

use super::AuthError;
use crate::{Result, fs_util, paths::Paths};

const KEYRING_SERVICE: &str = "dev.glintclient.launcher";
/// Windows Credential Manager caps a secret at 2560 bytes and stores text as UTF-16, so
/// one entry holds at most 1280 characters. Refresh tokens can be longer, so they are
/// split across entries.
const CHUNK_CHARS: usize = 1000;

/// A remembered account. Nothing secret: the refresh token lives in the OS credential
/// store, keyed by `uuid`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub uuid: String,
    pub username: String,
    pub added_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Accounts {
    pub active: Option<String>,
    pub accounts: Vec<Account>,
}

impl Accounts {
    pub async fn load(paths: &Paths) -> Result<Self> {
        Ok(fs_util::read_json_opt(&paths.accounts_file())
            .await?
            .unwrap_or_default())
    }

    pub async fn save(&self, paths: &Paths) -> Result<()> {
        fs_util::write_json(&paths.accounts_file(), self).await
    }

    pub fn active(&self) -> Option<&Account> {
        let uuid = self.active.as_deref()?;
        self.accounts.iter().find(|a| a.uuid == uuid)
    }

    /// Add or update an account and make it the active one.
    pub fn upsert(&mut self, uuid: &str, username: &str) -> Account {
        let account = match self.accounts.iter_mut().find(|a| a.uuid == uuid) {
            Some(existing) => {
                // Usernames can change; the UUID is the identity.
                existing.username = username.to_string();
                existing.clone()
            }
            None => {
                let account = Account {
                    uuid: uuid.to_string(),
                    username: username.to_string(),
                    added_at: fs_util::now_unix(),
                };
                self.accounts.push(account.clone());
                account
            }
        };
        self.active = Some(account.uuid.clone());
        account
    }

    pub fn set_active(&mut self, uuid: &str) -> bool {
        let known = self.accounts.iter().any(|a| a.uuid == uuid);
        if known {
            self.active = Some(uuid.to_string());
        }
        known
    }

    /// Remove an account. If it was active, the first remaining account takes over.
    pub fn remove(&mut self, uuid: &str) {
        self.accounts.retain(|a| a.uuid != uuid);
        if self.active.as_deref() == Some(uuid) {
            self.active = self.accounts.first().map(|a| a.uuid.clone());
        }
    }
}

/// Where secrets are kept. Abstracted so tests don't touch the real credential store.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// Windows Credential Manager, macOS Keychain, or the Secret Service on Linux.
pub struct KeyringStore;

impl KeyringStore {
    fn entry(key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, key).map_err(keyring_error)
    }
}

fn keyring_error(e: keyring::Error) -> crate::Error {
    AuthError::Keyring(e.to_string()).into()
}

impl SecretStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        Self::entry(key)?.set_password(value).map_err(keyring_error)
    }

    fn delete(&self, key: &str) -> Result<()> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        }
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl MemoryStore {
    pub fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.0.lock().unwrap().insert(key.into(), value.into());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

fn count_key(uuid: &str) -> String {
    format!("{uuid}.refresh.count")
}

fn chunk_key(uuid: &str, index: usize) -> String {
    format!("{uuid}.refresh.{index}")
}

fn stored_chunks(secrets: &dyn SecretStore, uuid: &str) -> Result<usize> {
    Ok(secrets
        .get(&count_key(uuid))?
        .and_then(|n| n.parse().ok())
        .unwrap_or(0))
}

pub fn save_refresh_token(secrets: &dyn SecretStore, uuid: &str, token: &str) -> Result<()> {
    let previous = stored_chunks(secrets, uuid)?;
    let chars: Vec<char> = token.chars().collect();
    let chunks: Vec<String> = chars
        .chunks(CHUNK_CHARS)
        .map(|c| c.iter().collect())
        .collect();
    for (index, chunk) in chunks.iter().enumerate() {
        secrets.set(&chunk_key(uuid, index), chunk)?;
    }
    secrets.set(&count_key(uuid), &chunks.len().to_string())?;
    // A shorter token than last time leaves stale tail entries behind; clear them.
    for index in chunks.len()..previous {
        secrets.delete(&chunk_key(uuid, index))?;
    }
    Ok(())
}

pub fn load_refresh_token(secrets: &dyn SecretStore, uuid: &str) -> Result<Option<String>> {
    let count = stored_chunks(secrets, uuid)?;
    if count == 0 {
        return Ok(None);
    }
    let mut token = String::new();
    for index in 0..count {
        match secrets.get(&chunk_key(uuid, index))? {
            Some(chunk) => token.push_str(&chunk),
            // A missing piece means the token is unusable; treat it as signed out.
            None => return Ok(None),
        }
    }
    Ok(Some(token))
}

pub fn delete_refresh_token(secrets: &dyn SecretStore, uuid: &str) -> Result<()> {
    for index in 0..stored_chunks(secrets, uuid)? {
        secrets.delete(&chunk_key(uuid, index))?;
    }
    secrets.delete(&count_key(uuid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_tokens_roundtrip_in_chunks() {
        let secrets = MemoryStore::default();
        let token: String = "abcdefghij".repeat(250) + "tail";
        save_refresh_token(&secrets, "u1", &token).unwrap();
        // 2504 characters: three chunks plus the count.
        assert_eq!(secrets.len(), 4);
        assert_eq!(load_refresh_token(&secrets, "u1").unwrap(), Some(token));

        // A shorter replacement cleans up the leftover chunks.
        save_refresh_token(&secrets, "u1", "short").unwrap();
        assert_eq!(secrets.len(), 2);
        assert_eq!(
            load_refresh_token(&secrets, "u1").unwrap().as_deref(),
            Some("short")
        );

        delete_refresh_token(&secrets, "u1").unwrap();
        assert!(secrets.is_empty());
        assert_eq!(load_refresh_token(&secrets, "u1").unwrap(), None);
    }

    #[test]
    fn a_missing_chunk_reads_as_signed_out() {
        let secrets = MemoryStore::default();
        save_refresh_token(&secrets, "u1", &"x".repeat(1500)).unwrap();
        secrets.delete(&chunk_key("u1", 1)).unwrap();
        assert_eq!(load_refresh_token(&secrets, "u1").unwrap(), None);
    }

    #[test]
    fn account_bookkeeping() {
        let mut accounts = Accounts::default();
        accounts.upsert("a", "Alice");
        accounts.upsert("b", "Bob");
        assert_eq!(accounts.active().unwrap().username, "Bob");

        // Signing in again updates the name instead of adding a duplicate.
        accounts.upsert("a", "Alicia");
        assert_eq!(accounts.accounts.len(), 2);
        assert_eq!(accounts.active().unwrap().username, "Alicia");

        assert!(!accounts.set_active("nobody"));
        accounts.remove("a");
        assert_eq!(accounts.active().unwrap().uuid, "b");
        accounts.remove("b");
        assert!(accounts.active().is_none());
    }
}
