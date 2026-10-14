//! API key storage. Desktop builds use the OS keychain via `keyring`; tests
//! use [`MemoryStore`]. Keys are addressed by provider id under one service.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::AiError;

pub const SERVICE: &str = "com.ccharafeddine.mediasmith";

pub trait KeyStore: Send + Sync {
    fn get(&self, provider: &str) -> Result<Option<String>, AiError>;
    fn set(&self, provider: &str, key: &str) -> Result<(), AiError>;
    fn delete(&self, provider: &str) -> Result<(), AiError>;
}

/// The OS keychain.
pub struct KeyringStore;

fn entry(provider: &str) -> Result<keyring::Entry, AiError> {
    keyring::Entry::new(SERVICE, &format!("ai:{provider}"))
        .map_err(|e| AiError::Keychain(e.to_string()))
}

impl KeyStore for KeyringStore {
    fn get(&self, provider: &str) -> Result<Option<String>, AiError> {
        match entry(provider)?.get_password() {
            Ok(k) => Ok(Some(k)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AiError::Keychain(e.to_string())),
        }
    }
    fn set(&self, provider: &str, key: &str) -> Result<(), AiError> {
        entry(provider)?
            .set_password(key.trim())
            .map_err(|e| AiError::Keychain(e.to_string()))
    }
    fn delete(&self, provider: &str) -> Result<(), AiError> {
        match entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AiError::Keychain(e.to_string())),
        }
    }
}

/// In-memory store for tests and for "don't remember" sessions.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl KeyStore for MemoryStore {
    fn get(&self, provider: &str) -> Result<Option<String>, AiError> {
        Ok(self.0.lock().unwrap().get(provider).cloned())
    }
    fn set(&self, provider: &str, key: &str) -> Result<(), AiError> {
        self.0
            .lock()
            .unwrap()
            .insert(provider.into(), key.trim().into());
        Ok(())
    }
    fn delete(&self, provider: &str) -> Result<(), AiError> {
        self.0.lock().unwrap().remove(provider);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trip() {
        let s = MemoryStore::default();
        assert_eq!(s.get("a").unwrap(), None);
        s.set("a", "  sk-123 \n").unwrap();
        assert_eq!(s.get("a").unwrap().as_deref(), Some("sk-123"));
        s.delete("a").unwrap();
        s.delete("a").unwrap();
        assert_eq!(s.get("a").unwrap(), None);
    }

    /// Real OS keychain. Opt-in (MS_KEYCHAIN_TEST=1) so CI and headless
    /// machines without a keyring don't fail; uses a throwaway entry.
    #[test]
    fn os_keychain_round_trip() {
        if std::env::var("MS_KEYCHAIN_TEST").is_err() {
            return;
        }
        let s = KeyringStore;
        let id = format!("selftest-{}", std::process::id());
        s.set(&id, "test-secret").unwrap();
        assert_eq!(s.get(&id).unwrap().as_deref(), Some("test-secret"));
        s.delete(&id).unwrap();
        assert_eq!(s.get(&id).unwrap(), None);
    }
}
