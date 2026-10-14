//! Caption rewrite: send caption lines with an instruction, get the same
//! number of lines back. Only text changes; the caller keeps every timestamp,
//! so timing can't drift.

use serde_json::Value;

use crate::client::Client;
use crate::AiError;

pub const SYSTEM: &str = "You edit video captions. You receive a JSON array of caption lines and an \
instruction. Return ONLY a JSON array of strings with exactly the same number of items, in the same \
order, one rewritten line per input line. Keep each line short enough to read on screen. Never merge, \
split, drop or add lines. No commentary.";

pub fn build_user_prompt(lines: &[String], instruction: &str) -> String {
    format!(
        "Instruction: {}\nLines ({}):\n{}",
        instruction.trim(),
        lines.len(),
        serde_json::to_string(lines).expect("strings serialize")
    )
}

/// Pull the JSON array out of a model reply (tolerates ``` fences, leading
/// prose, or a `{"lines": [...]}` wrapper) and check the count.
pub fn parse_reply(reply: &str, expected: usize) -> Result<Vec<String>, AiError> {
    let start = reply
        .find(['[', '{'])
        .ok_or_else(|| AiError::BadResponse("no JSON in reply".into()))?;
    let end = reply
        .rfind([']', '}'])
        .ok_or_else(|| AiError::BadResponse("no JSON in reply".into()))?;
    let v: Value = serde_json::from_str(&reply[start..=end])
        .map_err(|e| AiError::BadResponse(e.to_string()))?;
    let arr = match &v {
        Value::Array(a) => a,
        Value::Object(o) => o
            .get("lines")
            .and_then(Value::as_array)
            .ok_or_else(|| AiError::BadResponse("expected an array".into()))?,
        _ => return Err(AiError::BadResponse("expected an array".into())),
    };
    let lines: Vec<String> = arr
        .iter()
        .map(|x| {
            x.as_str()
                .map(|s| s.trim().to_string())
                .ok_or_else(|| AiError::BadResponse("non-string line".into()))
        })
        .collect::<Result<_, _>>()?;
    if lines.len() != expected {
        return Err(AiError::BadResponse(format!(
            "expected {expected} lines, got {}",
            lines.len()
        )));
    }
    Ok(lines)
}

/// Rewrite `lines` per `instruction` with `model`.
pub fn rewrite_captions(
    client: &Client,
    model: &str,
    lines: &[String],
    instruction: &str,
) -> Result<Vec<String>, AiError> {
    if lines.is_empty() {
        return Ok(Vec::new());
    }
    if instruction.trim().is_empty() {
        return Err(AiError::Config("say how to rewrite the captions".into()));
    }
    let reply = client.chat(model, SYSTEM, &build_user_prompt(lines, instruction), 0.3)?;
    parse_reply(&reply, lines.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::mock::serve;
    use crate::config::{ProviderConfig, ProviderKind};

    fn lines() -> Vec<String> {
        vec!["wait for it".into(), "this is the part".into()]
    }

    #[test]
    fn parses_plain_fenced_and_wrapped_replies() {
        assert_eq!(parse_reply(r#"["A","B"]"#, 2).unwrap(), vec!["A", "B"]);
        assert_eq!(
            parse_reply("Sure!\n```json\n[\" A \", \"B\"]\n```", 2).unwrap(),
            vec!["A", "B"]
        );
        assert_eq!(
            parse_reply(r#"{"lines":["A","B"]}"#, 2).unwrap(),
            vec!["A", "B"]
        );
    }

    #[test]
    fn rejects_wrong_counts_and_junk() {
        assert!(parse_reply(r#"["A"]"#, 2).is_err());
        assert!(parse_reply("no json here", 1).is_err());
        assert!(parse_reply("[1, 2]", 2).is_err());
    }

    #[test]
    fn end_to_end_against_mock_server() {
        let reply =
            r#"{"choices":[{"message":{"content":"[\"Wait for it… 👀\",\"THIS is the part\"]"}}]}"#;
        let m = serve(vec![(200, reply.into())]);
        let mut cfg = ProviderConfig::new("ollama", ProviderKind::Compatible);
        cfg.base_url = m.base.clone();
        let c = Client::new(&cfg, None).unwrap();
        let out = rewrite_captions(&c, "llama3.2", &lines(), "add emoji, more hype").unwrap();
        assert_eq!(out, vec!["Wait for it… 👀", "THIS is the part"]);
        let req = m.requests.lock().unwrap()[0].clone();
        assert!(req.contains("add emoji, more hype") && req.contains("exactly the same number"));
    }

    #[test]
    fn empty_input_skips_the_network_and_blank_instruction_errors() {
        let c = Client::new(&ProviderConfig::new("l", ProviderKind::Compatible), None).unwrap();
        assert!(rewrite_captions(&c, "m", &[], "x").unwrap().is_empty());
        assert!(rewrite_captions(&c, "m", &lines(), "  ").is_err());
    }
}
