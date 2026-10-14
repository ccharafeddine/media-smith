//! Minimal OpenAI-compatible client: `GET /models` and
//! `POST /chat/completions`. Works with OpenAI, Ollama, LM Studio, OpenRouter,
//! vLLM, and Gemini/Anthropic through their OpenAI-compatible endpoints.

use std::time::Duration;

use serde_json::{json, Value};

use crate::config::ProviderConfig;
use crate::{redact, AiError};

pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    key: Option<String>,
    label: String,
}

impl Client {
    pub fn new(cfg: &ProviderConfig, key: Option<String>) -> Result<Self, AiError> {
        cfg.validate()?;
        if cfg.kind.requires_key() && key.as_deref().is_none_or(str::is_empty) {
            return Err(AiError::NoKey(cfg.label.clone()));
        }
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent(concat!("MediaSmith/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AiError::Http(e.to_string()))?;
        Ok(Self {
            http,
            base: cfg.base_url.trim_end_matches('/').to_string(),
            key,
            label: cfg.label.clone(),
        })
    }

    fn auth(&self, rb: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        match &self.key {
            Some(k) if !k.is_empty() => rb.bearer_auth(k),
            _ => rb,
        }
    }

    fn send(&self, rb: reqwest::blocking::RequestBuilder) -> Result<Value, AiError> {
        let resp = self
            .auth(rb)
            .send()
            .map_err(|e| AiError::Http(redact(&e.to_string(), self.key.as_deref())))?;
        let status = resp.status().as_u16();
        let text = resp.text().map_err(|e| AiError::Http(e.to_string()))?;
        if !(200..300).contains(&status) {
            let body: String = redact(&text, self.key.as_deref())
                .chars()
                .take(400)
                .collect();
            return Err(AiError::Status {
                provider: self.label.clone(),
                status,
                body,
            });
        }
        serde_json::from_str(&text).map_err(|e| AiError::BadResponse(e.to_string()))
    }

    /// Model ids the endpoint offers (doubles as a connection test).
    pub fn list_models(&self) -> Result<Vec<String>, AiError> {
        let v = self.send(self.http.get(format!("{}/models", self.base)))?;
        let data = v
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| AiError::BadResponse("no data[]".into()))?;
        Ok(data
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
            .collect())
    }

    /// One chat completion; returns the first choice's text.
    pub fn chat(
        &self,
        model: &str,
        system: &str,
        user: &str,
        temperature: f32,
    ) -> Result<String, AiError> {
        let body = json!({
            "model": model,
            "temperature": temperature,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ]
        });
        let v = self.send(
            self.http
                .post(format!("{}/chat/completions", self.base))
                .json(&body),
        )?;
        v.pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| AiError::BadResponse("no choices[0].message.content".into()))
    }
}

/// A one-request-at-a-time HTTP server for tests: answers each connection with
/// the next canned (status, body) and records the raw requests it saw.
#[cfg(test)]
pub(crate) mod mock {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    pub struct Mock {
        pub base: String,
        pub requests: Arc<Mutex<Vec<String>>>,
    }

    pub fn serve(responses: Vec<(u16, String)>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        std::thread::spawn(move || {
            for (status, body) in responses {
                let Ok((mut s, _)) = listener.accept() else {
                    return;
                };
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                // Read headers, then Content-Length bytes of body.
                loop {
                    let n = s.read(&mut chunk).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(h) = text.find("\r\n\r\n") {
                        let len = text[..h]
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if buf.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buf).to_string());
                let resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
            }
        });
        Mock { base, requests }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::serve;
    use super::*;
    use crate::config::ProviderKind;

    fn cfg(base: &str, kind: ProviderKind) -> ProviderConfig {
        let mut c = ProviderConfig::new("t", kind);
        c.base_url = base.into();
        c
    }

    #[test]
    fn lists_models_with_bearer_auth() {
        let m = serve(vec![(
            200,
            r#"{"data":[{"id":"gpt-4.1-mini"},{"id":"gpt-image-1"}]}"#.into(),
        )]);
        let c = Client::new(&cfg(&m.base, ProviderKind::OpenAi), Some("sk-test".into())).unwrap();
        assert_eq!(
            c.list_models().unwrap(),
            vec!["gpt-4.1-mini", "gpt-image-1"]
        );
        let req = m.requests.lock().unwrap()[0].clone();
        assert!(req.starts_with("GET /v1/models"));
        assert!(req.to_lowercase().contains("authorization: bearer sk-test"));
    }

    #[test]
    fn chat_posts_messages_and_reads_first_choice() {
        let m = serve(vec![(
            200,
            r#"{"choices":[{"message":{"role":"assistant","content":"hi"}}]}"#.into(),
        )]);
        let c = Client::new(&cfg(&m.base, ProviderKind::Compatible), None).unwrap();
        assert_eq!(c.chat("llama3.2", "sys", "user text", 0.2).unwrap(), "hi");
        let req = m.requests.lock().unwrap()[0].clone();
        assert!(req.starts_with("POST /v1/chat/completions"));
        assert!(req.contains(r#""model":"llama3.2""#) && req.contains("user text"));
        assert!(
            !req.to_lowercase().contains("authorization"),
            "no key, no auth header (local Ollama)"
        );
    }

    #[test]
    fn http_errors_are_reported_and_redacted() {
        let m = serve(vec![(401, r#"{"error":"bad key sk-secret-123"}"#.into())]);
        let c = Client::new(
            &cfg(&m.base, ProviderKind::OpenAi),
            Some("sk-secret-123".into()),
        )
        .unwrap();
        let e = c.list_models().unwrap_err().to_string();
        assert!(
            e.contains("401") && e.contains("[redacted]") && !e.contains("sk-secret-123"),
            "{e}"
        );
    }

    #[test]
    fn hosted_providers_need_a_key() {
        assert!(matches!(
            Client::new(&ProviderConfig::new("o", ProviderKind::OpenAi), None),
            Err(AiError::NoKey(_))
        ));
        assert!(Client::new(&ProviderConfig::new("l", ProviderKind::Compatible), None).is_ok());
    }
}
