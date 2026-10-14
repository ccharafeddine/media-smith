//! Provider settings. Saved as JSON in the app config dir; secrets are never
//! part of this struct, so they can't leak into the file.

use serde::{Deserialize, Serialize};

use crate::AiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAi,
    Gemini,
    Anthropic,
    /// Any OpenAI-compatible server: Ollama, LM Studio, OpenRouter, vLLM…
    Compatible,
}

impl ProviderKind {
    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/openai",
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::Compatible => "http://localhost:11434/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "gpt-4.1-mini",
            Self::Gemini => "gemini-2.5-flash",
            Self::Anthropic => "claude-sonnet-4-5",
            Self::Compatible => "llama3.2",
        }
    }

    /// Hosted providers need a key; a local compatible server may not.
    pub fn requires_key(self) -> bool {
        self != Self::Compatible
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub label: String,
    pub base_url: String,
    pub model: String,
}

impl ProviderConfig {
    pub fn new(id: &str, kind: ProviderKind) -> Self {
        Self {
            id: id.into(),
            kind,
            label: id.into(),
            base_url: kind.default_base_url().into(),
            model: kind.default_model().into(),
        }
    }

    pub fn validate(&self) -> Result<(), AiError> {
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(AiError::Config(
                "provider id must be letters, digits, - or _".into(),
            ));
        }
        if !(self.base_url.starts_with("https://") || self.base_url.starts_with("http://")) {
            return Err(AiError::Config(
                "base URL must start with http:// or https://".into(),
            ));
        }
        if self.model.trim().is_empty() {
            return Err(AiError::Config("model can't be empty".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AiSettings {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    /// Provider used for caption rewrite (and other text features for now).
    #[serde(default)]
    pub text_provider: Option<String>,
}

impl AiSettings {
    pub fn from_json(s: &str) -> Result<Self, AiError> {
        serde_json::from_str(s).map_err(|e| AiError::Config(e.to_string()))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialize")
    }

    pub fn get(&self, id: &str) -> Result<&ProviderConfig, AiError> {
        self.providers
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AiError::UnknownProvider(id.into()))
    }

    /// Insert or replace by id.
    pub fn upsert(&mut self, p: ProviderConfig) -> Result<(), AiError> {
        p.validate()?;
        match self.providers.iter_mut().find(|x| x.id == p.id) {
            Some(slot) => *slot = p,
            None => self.providers.push(p),
        }
        if self.text_provider.is_none() {
            self.text_provider = self.providers.first().map(|p| p.id.clone());
        }
        Ok(())
    }

    pub fn remove(&mut self, id: &str) {
        self.providers.retain(|p| p.id != id);
        if self.text_provider.as_deref() == Some(id) {
            self.text_provider = self.providers.first().map(|p| p.id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_per_kind() {
        let p = ProviderConfig::new("local", ProviderKind::Compatible);
        assert_eq!(p.base_url, "http://localhost:11434/v1");
        assert!(!ProviderKind::Compatible.requires_key());
        assert!(ProviderKind::Gemini.default_base_url().ends_with("/openai"));
    }

    #[test]
    fn upsert_remove_and_default_text_provider() {
        let mut s = AiSettings::default();
        s.upsert(ProviderConfig::new("openai", ProviderKind::OpenAi))
            .unwrap();
        s.upsert(ProviderConfig::new("ollama", ProviderKind::Compatible))
            .unwrap();
        assert_eq!(s.text_provider.as_deref(), Some("openai"));
        let mut changed = ProviderConfig::new("openai", ProviderKind::OpenAi);
        changed.model = "gpt-4.1".into();
        s.upsert(changed).unwrap();
        assert_eq!(s.providers.len(), 2);
        assert_eq!(s.get("openai").unwrap().model, "gpt-4.1");
        s.remove("openai");
        assert_eq!(s.text_provider.as_deref(), Some("ollama"));
        assert!(s.get("openai").is_err());
    }

    #[test]
    fn validation_rejects_bad_input() {
        let mut p = ProviderConfig::new("a b", ProviderKind::OpenAi);
        assert!(p.validate().is_err());
        p.id = "ok".into();
        p.base_url = "file:///etc".into();
        assert!(p.validate().is_err());
    }

    #[test]
    fn json_round_trip_has_no_secret_fields() {
        let mut s = AiSettings::default();
        s.upsert(ProviderConfig::new("openai", ProviderKind::OpenAi))
            .unwrap();
        let j = s.to_json();
        assert!(!j.to_lowercase().contains("key"));
        assert_eq!(AiSettings::from_json(&j).unwrap(), s);
        assert_eq!(AiSettings::from_json("{}").unwrap(), AiSettings::default());
    }
}
