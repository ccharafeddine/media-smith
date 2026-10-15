//! The settings + keychain logic behind the desktop shell's AI commands,
//! generic over [`KeyStore`] so it is tested with [`crate::keys::MemoryStore`]
//! and a mock HTTP server, and runs on the OS keychain in the app.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

use crate::client::Client;
use crate::config::{AiSettings, ProviderConfig};
use crate::keys::KeyStore;

#[derive(Debug, Serialize, PartialEq)]
pub struct ProviderView {
    #[serde(flatten)]
    pub config: ProviderConfig,
    pub has_key: bool,
}

/// What the Settings page shows. Never contains a key.
#[derive(Debug, Serialize, PartialEq)]
pub struct AiView {
    pub providers: Vec<ProviderView>,
    pub text_provider: Option<String>,
}

pub struct AiService<K: KeyStore> {
    path: PathBuf,
    settings: Mutex<AiSettings>,
    keys: K,
}

impl<K: KeyStore> AiService<K> {
    /// Load settings from `path` (missing or corrupt file = empty settings).
    pub fn load(path: PathBuf, keys: K) -> Self {
        let settings = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| AiSettings::from_json(&s).ok())
            .unwrap_or_default();
        Self {
            path,
            settings: Mutex::new(settings),
            keys,
        }
    }

    fn save(&self, s: &AiSettings) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        // Write-then-rename so a crash never leaves a half-written file.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, s.to_json()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    fn view_of(&self, s: &AiSettings) -> AiView {
        AiView {
            providers: s
                .providers
                .iter()
                .map(|p| ProviderView {
                    config: p.clone(),
                    has_key: self.keys.get(&p.id).ok().flatten().is_some(),
                })
                .collect(),
            text_provider: s.text_provider.clone(),
        }
    }

    pub fn view(&self) -> AiView {
        self.view_of(&self.settings.lock().unwrap())
    }

    /// Save a provider. `key`: `Some("")` deletes the stored key, `None`
    /// keeps it. Settings are only changed if the key store accepted the key.
    pub fn save_provider(
        &self,
        config: ProviderConfig,
        key: Option<String>,
        make_default: bool,
    ) -> Result<AiView, String> {
        let mut s = self.settings.lock().unwrap();
        let mut next = s.clone();
        let id = config.id.clone();
        next.upsert(config).map_err(|e| e.to_string())?;
        match key.as_deref().map(str::trim) {
            Some("") => self.keys.delete(&id).map_err(|e| e.to_string())?,
            Some(k) => self.keys.set(&id, k).map_err(|e| e.to_string())?,
            None => {}
        }
        if make_default || next.text_provider.is_none() {
            next.text_provider = Some(id);
        }
        self.save(&next)?;
        *s = next;
        Ok(self.view_of(&s))
    }

    pub fn delete_provider(&self, id: &str) -> Result<AiView, String> {
        let mut s = self.settings.lock().unwrap();
        s.remove(id);
        self.keys.delete(id).map_err(|e| e.to_string())?;
        self.save(&s)?;
        Ok(self.view_of(&s))
    }

    /// A client for provider `id` (or the default text provider) + its model.
    pub fn client(&self, id: Option<&str>) -> Result<(Client, String), String> {
        let s = self.settings.lock().unwrap();
        let id = id
            .map(str::to_string)
            .or_else(|| s.text_provider.clone())
            .ok_or("Add an AI provider in Settings first.")?;
        let cfg = s.get(&id).map_err(|e| e.to_string())?.clone();
        drop(s);
        let key = self.keys.get(&cfg.id).map_err(|e| e.to_string())?;
        let model = cfg.model.clone();
        Ok((Client::new(&cfg, key).map_err(|e| e.to_string())?, model))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::mock::serve;
    use crate::config::ProviderKind;
    use crate::keys::MemoryStore;
    use crate::AiError;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ms-ai-svc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("ai-providers.json")
    }

    /// A key store that refuses writes, like a locked keychain.
    #[derive(Default)]
    struct Locked;
    impl KeyStore for Locked {
        fn get(&self, _: &str) -> Result<Option<String>, AiError> {
            Ok(None)
        }
        fn set(&self, _: &str, _: &str) -> Result<(), AiError> {
            Err(AiError::Keychain("locked".into()))
        }
        fn delete(&self, _: &str) -> Result<(), AiError> {
            Ok(())
        }
    }

    #[test]
    fn save_keeps_key_out_of_the_settings_file_and_reloads() {
        let path = tmp("save");
        let svc = AiService::load(path.clone(), MemoryStore::default());
        let v = svc
            .save_provider(
                ProviderConfig::new("openai", ProviderKind::OpenAi),
                Some(" sk-live-123 ".into()),
                false,
            )
            .unwrap();
        assert!(v.providers[0].has_key);
        assert_eq!(
            v.text_provider.as_deref(),
            Some("openai"),
            "first provider becomes default"
        );
        let file = std::fs::read_to_string(&path).unwrap();
        assert!(!file.contains("sk-live-123"), "{file}");
        assert!(serde_json::to_string(&v).unwrap().find("sk-live").is_none());
        // None keeps the key, Some("") deletes it.
        let v = svc
            .save_provider(
                ProviderConfig::new("openai", ProviderKind::OpenAi),
                None,
                false,
            )
            .unwrap();
        assert!(v.providers[0].has_key);
        let v = svc
            .save_provider(
                ProviderConfig::new("openai", ProviderKind::OpenAi),
                Some("".into()),
                false,
            )
            .unwrap();
        assert!(!v.providers[0].has_key);
        let again = AiService::load(path.clone(), MemoryStore::default());
        assert_eq!(again.view().providers.len(), 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn delete_and_defaults() {
        let path = tmp("del");
        let svc = AiService::load(path.clone(), MemoryStore::default());
        svc.save_provider(
            ProviderConfig::new("a", ProviderKind::Compatible),
            None,
            false,
        )
        .unwrap();
        let v = svc
            .save_provider(
                ProviderConfig::new("b", ProviderKind::Compatible),
                Some("k".into()),
                true,
            )
            .unwrap();
        assert_eq!(v.text_provider.as_deref(), Some("b"));
        let v = svc.delete_provider("b").unwrap();
        assert_eq!(v.providers.len(), 1);
        assert!(svc.client(Some("b")).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn locked_keychain_leaves_settings_unchanged() {
        let path = tmp("locked");
        let svc = AiService::load(path.clone(), Locked);
        let e = svc
            .save_provider(
                ProviderConfig::new("o", ProviderKind::OpenAi),
                Some("sk".into()),
                true,
            )
            .unwrap_err();
        assert!(e.contains("locked"), "{e}");
        assert!(svc.view().providers.is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn corrupt_settings_file_loads_empty() {
        let path = tmp("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{nope").unwrap();
        assert!(AiService::load(path.clone(), MemoryStore::default())
            .view()
            .providers
            .is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn client_uses_stored_key_against_mock_server() {
        let path = tmp("client");
        let m = serve(vec![(200, r#"{"data":[{"id":"m1"}]}"#.into())]);
        let svc = AiService::load(path.clone(), MemoryStore::default());
        assert!(svc.client(None).unwrap_err().contains("Add an AI provider"));
        let mut cfg = ProviderConfig::new("o", ProviderKind::OpenAi);
        cfg.base_url = m.base.clone();
        svc.save_provider(cfg.clone(), None, true).unwrap();
        assert!(svc
            .client(None)
            .unwrap_err()
            .to_lowercase()
            .contains("no api key"));
        svc.save_provider(cfg, Some("sk-abc".into()), true).unwrap();
        let (c, model) = svc.client(None).unwrap();
        assert!(!model.is_empty());
        assert_eq!(c.list_models().unwrap(), vec!["m1"]);
        let req = m.requests.lock().unwrap()[0].to_lowercase();
        assert!(req.contains("authorization: bearer sk-abc"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
