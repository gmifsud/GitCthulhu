//! Kilo gateway. The window never sees these URLs or the token.

use std::sync::Arc;
use std::time::Duration;

use core_domain::DomainError;
use gui_app::{CommitSuggester, CommitSuggestion, ModelList, ModelLister, ProviderExchange};

const MODELS_URL: &str = "https://api.kilo.ai/api/gateway/models";
const COMPLETIONS_URL: &str = "https://api.kilo.ai/api/gateway/chat/completions";
const DIFF_IN_PROMPT: usize = 12_000;

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
    let payload = serde_json::json!({
        "model": model,
        "temperature": 0.2,
        "max_tokens": 80,
        "messages": [
            {
                "role": "system",
                "content": "Reply with one git commit subject only. At most 72 characters. Sentence case. No quotes and no markdown."
            },
            { "role": "user", "content": diff }
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
        .timeout(Duration::from_secs(45))
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
            HttpFailure {
                status: 0,
                error: DomainError::Unknown(detail.clone()),
                detail,
            }
        }
    }
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
    if let Some(message) = parsed
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(|message| message.as_str())
    {
        return Err(DomainError::Unknown(clip(message, 300)));
    }
    let content = parsed
        .pointer("/choices/0/message/content")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| DomainError::Unknown("Kilo returned no commit message.".to_string()))?;
    let line = content.lines().next().unwrap_or(content).trim();
    let line = line.trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`');
    let line = line.trim();
    if line.is_empty() {
        Err(DomainError::Unknown(
            "Kilo returned no commit message.".to_string(),
        ))
    } else {
        Ok(clip(line, 72))
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
    clipped.push_str("…");
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
}
