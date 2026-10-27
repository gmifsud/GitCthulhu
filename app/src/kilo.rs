//! Kilo AI Gateway. The window never sees these URLs or the token.
//!
//! Base URL matches https://kilo.ai/docs/gateway — OpenAI-compatible `/api/v1`.

use std::sync::Arc;
use std::time::Duration;

use core_domain::DomainError;
use gui_app::{CommitSuggester, CommitSuggestion, ModelList, ModelLister, ProviderExchange};

const MODELS_URL: &str = "https://ai-gateway.kilo.ai/api/v1/models";
const COMPLETIONS_URL: &str = "https://ai-gateway.kilo.ai/api/v1/chat/completions";
const DIFF_IN_PROMPT: usize = 12_000;
/// Free / auto models can be slow on a cold route; 45s was timing out as WSAETIMEDOUT.
const HTTP_TIMEOUT: Duration = Duration::from_secs(90);

pub fn lister() -> ModelLister {
    Arc::new(|token| list_models(&token))
}

pub fn suggester() -> CommitSuggester {
    Arc::new(|token, model, diff| suggest(&token, &model, &diff))
}

fn list_models(token: &str) -> ModelList {
    match get(MODELS_URL, token) {
        Ok(body) => ModelList {
            ids: parse_model_ids(&body),
            exchange: reply("GET", MODELS_URL, 200, &body, token),
        },
        Err(failure) => ModelList {
            ids: Err(failure.error),
            exchange: reply("GET", MODELS_URL, failure.status, &failure.detail, token),
        },
    }
}

fn suggest(token: &str, model: &str, diff: &str) -> CommitSuggestion {
    let model = model.trim();
    if model.is_empty() {
        return CommitSuggestion {
            subject: Err(DomainError::Unknown(
                "Choose a model under Settings.".to_string(),
            )),
            exchange: ProviderExchange::kilo("Choose a model under Settings."),
        };
    }
    let diff = clip(diff, DIFF_IN_PROMPT);
    let user = format!("Write a one-line subject for this diff:\n\n{diff}");
    let payload = serde_json::json!({
        "model": model,
        "temperature": 0.2,
        "max_tokens": 120,
        "stream": false,
        "messages": [
            {
                "role": "system",
                "content": "You write git subjects. Reply with exactly one line: at most 72 characters, sentence case, no quotes, no markdown, no trailing period unless needed."
            },
            { "role": "user", "content": user }
        ]
    });
    match post(COMPLETIONS_URL, token, payload) {
        Ok(body) => CommitSuggestion {
            subject: parse_commit_subject(&body),
            exchange: reply("POST", COMPLETIONS_URL, 200, &body, token),
        },
        Err(failure) => CommitSuggestion {
            subject: Err(failure.error),
            exchange: reply(
                "POST",
                COMPLETIONS_URL,
                failure.status,
                &failure.detail,
                token,
            ),
        },
    }
}

struct HttpFailure {
    status: u16,
    detail: String,
    error: DomainError,
}

fn reply(method: &str, url: &str, status: u16, body: &str, token: &str) -> ProviderExchange {
    let body = redact(&clip(body, 4_000), token);
    ProviderExchange::kilo(format!("{method} {url}\nstatus {status}\n{body}"))
}

fn get(url: &str, token: &str) -> Result<String, HttpFailure> {
    let response = authorize(agent().get(url), token)
        .call()
        .map_err(|error| http_failure(error, token))?;
    response.into_string().map_err(|error| HttpFailure {
        status: 0,
        detail: error.to_string(),
        error: DomainError::Unknown(error.to_string()),
    })
}

fn post(url: &str, token: &str, payload: serde_json::Value) -> Result<String, HttpFailure> {
    let response = authorize(agent().post(url), token)
        .send_json(payload)
        .map_err(|error| http_failure(error, token))?;
    response.into_string().map_err(|error| HttpFailure {
        status: 0,
        detail: error.to_string(),
        error: DomainError::Unknown(error.to_string()),
    })
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(HTTP_TIMEOUT)
        .user_agent("GitCthulhu/0.1")
        .build()
}

fn authorize(request: ureq::Request, token: &str) -> ureq::Request {
    let token = token.trim();
    if token.is_empty() {
        request
    } else {
        request.set("Authorization", &format!("Bearer {token}"))
    }
}

fn http_failure(error: ureq::Error, token: &str) -> HttpFailure {
    match error {
        ureq::Error::Status(code, response) => {
            let body = redact(&response.into_string().unwrap_or_default(), token);
            let message = api_message(&body).unwrap_or_else(|| format!("Kilo returned {code}."));
            let error = if code == 401 {
                DomainError::AuthenticationFailed
            } else {
                DomainError::Unknown(clip(&message, 300))
            };
            HttpFailure {
                status: code,
                detail: body,
                error,
            }
        }
        ureq::Error::Transport(error) => {
            let detail = redact(&error.to_string(), token);
            let message = transport_hint(&detail);
            HttpFailure {
                status: 0,
                error: DomainError::Unknown(message),
                detail,
            }
        }
    }
}

/// Turn opaque WSAETIMEDOUT / DNS failures into an actionable Settings hint.
fn transport_hint(detail: &str) -> String {
    let lower = detail.to_ascii_lowercase();
    if lower.contains("timed out")
        || lower.contains("10060")
        || lower.contains("failed to respond")
        || lower.contains("network error")
    {
        return format!(
            "Could not reach the Kilo gateway (timed out). Check your network or proxy, then retry. Details: {detail}"
        );
    }
    if lower.contains("dns") || lower.contains("name resolution") || lower.contains("no such host")
    {
        return format!(
            "Could not resolve ai-gateway.kilo.ai. Check DNS or VPN. Details: {detail}"
        );
    }
    format!("Kilo gateway network error: {detail}")
}

fn redact(text: &str, token: &str) -> String {
    let token = token.trim();
    if token.is_empty() {
        text.to_string()
    } else {
        text.replace(token, "[token]")
    }
}

pub(crate) fn parse_model_ids(body: &str) -> Result<Vec<String>, DomainError> {
    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|error| DomainError::Unknown(error.to_string()))?;
    let rows = parsed
        .get("data")
        .and_then(|value| value.as_array())
        .ok_or_else(|| DomainError::Unknown("Kilo returned no model list.".to_string()))?;
    let mut ids = Vec::new();
    for row in rows {
        if let Some(id) = row.get("id").and_then(|value| value.as_str()) {
            let id = id.trim();
            if !id.is_empty() && !ids.iter().any(|existing: &String| existing == id) {
                ids.push(id.to_string());
            }
        }
    }
    if ids.is_empty() {
        Err(DomainError::Unknown(
            "Kilo returned no model list.".to_string(),
        ))
    } else {
        Ok(ids)
    }
}

pub(crate) fn parse_commit_subject(body: &str) -> Result<String, DomainError> {
    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|error| DomainError::Unknown(error.to_string()))?;
    if let Some(message) = gateway_error_message(&parsed) {
        return Err(DomainError::Unknown(clip(&message, 300)));
    }
    let content = extract_assistant_text(&parsed).ok_or_else(|| {
        let reason = parsed
            .pointer("/choices/0/finish_reason")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        DomainError::Unknown(format!(
            "Kilo returned no commit message (finish_reason={reason}). Try another model under Settings."
        ))
    })?;
    let line = content.lines().next().unwrap_or(content.as_str()).trim();
    let line = line.trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`');
    let line = line.trim().trim_start_matches(['*', '-', '#']).trim();
    if line.is_empty() {
        Err(DomainError::Unknown(
            "Kilo returned an empty commit message. Try another model under Settings.".to_string(),
        ))
    } else {
        Ok(clip(line, 72))
    }
}

fn gateway_error_message(parsed: &serde_json::Value) -> Option<String> {
    if let Some(message) = parsed
        .pointer("/error/message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(message.to_string());
    }
    // Some gateway failures are a flat `{ "error": "...", "message": "..." }`.
    if let Some(message) = parsed
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        if parsed.get("error").is_some() || parsed.get("error_type").is_some() {
            return Some(message.to_string());
        }
    }
    if let Some(error) = parsed
        .get("error")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(error.to_string());
    }
    None
}

/// OpenAI-compatible content may be a string or an array of text parts.
fn extract_assistant_text(parsed: &serde_json::Value) -> Option<String> {
    let message = parsed.pointer("/choices/0/message")?;
    if let Some(text) = value_as_text(message.get("content")?) {
        return Some(text);
    }
    // Fallbacks used by some gateway models.
    for key in ["text", "reasoning", "refusal"] {
        if let Some(text) = message.get(key).and_then(value_as_text) {
            return Some(text);
        }
    }
    parsed
        .pointer("/choices/0/text")
        .and_then(value_as_text)
}

fn value_as_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
        serde_json::Value::Array(parts) => {
            let mut out = String::new();
            for part in parts {
                if let Some(text) = part
                    .get("text")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(text);
                    continue;
                }
                if part.get("type").and_then(|value| value.as_str()) == Some("text") {
                    if let Some(text) = part
                        .get("text")
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                    {
                        if !out.is_empty() {
                            out.push(' ');
                        }
                        out.push_str(text);
                    }
                }
            }
            (!out.is_empty()).then_some(out)
        }
        _ => None,
    }
}

fn api_message(body: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
    parsed
        .pointer("/error/message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_domain::AppSettings;

    #[test]
    fn model_ids_come_from_the_data_array() {
        let body = r#"{"data":[{"id":"kilo-auto/free","name":"Free"},{"id":" "},{"id":"kilo-auto/free"}]}"#;
        assert_eq!(
            parse_model_ids(body).expect("models"),
            vec![AppSettings::DEFAULT_AGENT_MODEL.to_string()]
        );
    }

    #[test]
    fn a_completion_keeps_the_first_line_without_quotes() {
        let body = r#"{"choices":[{"message":{"content":"\"Record the open folder.\"\nmore"}}]}"#;
        assert_eq!(
            parse_commit_subject(body).expect("subject"),
            "Record the open folder."
        );
    }

    #[test]
    fn a_completion_accepts_array_content_parts() {
        let body = r#"{"choices":[{"message":{"content":[{"type":"text","text":"Update the packages"}]}}]}"#;
        assert_eq!(
            parse_commit_subject(body).expect("subject"),
            "Update the packages"
        );
    }

    #[test]
    fn a_flat_gateway_error_is_surfaced() {
        let body = r#"{"error":"The upstream provider was unable to process the request","error_type":"upstream_error","message":"The upstream provider was unable to process the request"}"#;
        let error = parse_commit_subject(body).expect_err("error");
        assert!(matches!(
            error,
            DomainError::Unknown(message) if message.contains("upstream provider")
        ));
    }

    #[test]
    fn the_token_is_not_left_in_a_reply() {
        let exchange = reply(
            "POST",
            COMPLETIONS_URL,
            401,
            "rejected secret-token",
            "secret-token",
        );
        assert!(!exchange.text.contains("secret-token"));
        assert!(exchange.text.contains("[token]"));
        assert_eq!(exchange.source, "Kilo");
    }

    #[test]
    fn an_error_body_is_the_message() {
        let body = r#"{"error":{"message":"Rate limit exceeded"}}"#;
        let error = parse_commit_subject(body).expect_err("error");
        assert_eq!(
            error,
            DomainError::Unknown("Rate limit exceeded".to_string())
        );
    }

    #[test]
    fn transport_timeout_gets_a_clear_hint() {
        let hint = transport_hint("Network Error: ... 10060 ... failed to respond");
        assert!(hint.contains("timed out"));
        assert!(hint.contains("proxy"));
    }

    #[test]
    fn gateway_urls_use_the_documented_host() {
        assert!(MODELS_URL.starts_with("https://ai-gateway.kilo.ai/api/v1/"));
        assert!(COMPLETIONS_URL.starts_with("https://ai-gateway.kilo.ai/api/v1/"));
    }
}
