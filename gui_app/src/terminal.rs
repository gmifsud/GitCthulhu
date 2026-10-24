//! Debug transcript for provider replies. Git state does not live here.

use core_domain::DomainError;

const CAP: usize = 80;
const BODY_LIMIT: usize = 4_000;

/// Who produced a line. Callers pass their own name; the log does not know them.
pub const SOURCE_KILO: &str = "Kilo";
pub const SOURCE_GIT: &str = "Git";

/// A provider round-trip that is safe to print. The body must not contain a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderExchange {
    pub source: String,
    pub text: String,
}

impl ProviderExchange {
    pub fn new(source: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            text: clip(&text.into(), BODY_LIMIT),
        }
    }

    pub fn kilo(text: impl Into<String>) -> Self {
        Self::new(SOURCE_KILO, text)
    }
}

/// Model ids plus the reply that produced them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelList {
    pub ids: Result<Vec<String>, DomainError>,
    pub exchange: ProviderExchange,
}

/// A commit subject plus the reply that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSuggestion {
    pub subject: Result<String, DomainError>,
    pub exchange: ProviderExchange,
}

impl CommitSuggestion {
    pub fn local_failure(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            subject: Err(DomainError::Unknown(message.clone())),
            exchange: ProviderExchange::new(SOURCE_GIT, message),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranscriptEntry {
    pub source: String,
    pub text: String,
}

/// The lines the terminal shows, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Transcript {
    entries: Vec<TranscriptEntry>,
}

impl Transcript {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn record(&mut self, exchange: ProviderExchange) {
        self.entries.push(TranscriptEntry {
            source: exchange.source,
            text: exchange.text,
        });
        if self.entries.len() > CAP {
            let extra = self.entries.len() - CAP;
            self.entries.drain(0..extra);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn entries(&self) -> &[TranscriptEntry] {
        &self.entries
    }
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max).collect();
    clipped.push_str("\n[truncated]");
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transcript_keeps_the_newest_lines_and_can_be_cleared() {
        let mut transcript = Transcript::new();
        for index in 0..(CAP + 5) {
            transcript.record(ProviderExchange::new("Kilo", index.to_string()));
        }
        assert_eq!(transcript.entries().len(), CAP);
        assert_eq!(transcript.entries()[0].text, "5");
        transcript.clear();
        assert!(transcript.entries().is_empty());
    }

    #[test]
    fn a_local_failure_is_labelled_as_git() {
        let suggestion = CommitSuggestion::local_failure("Select a file first.");
        assert_eq!(suggestion.exchange.source, SOURCE_GIT);
        assert!(suggestion.subject.is_err());
    }
}
