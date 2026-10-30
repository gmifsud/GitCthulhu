//! The terminal's scrollback and the provider replies it also prints. Git state does not live here.

use std::sync::Arc;

use core_domain::DomainError;

/// Lines kept in the scrollback. Past this the oldest are dropped in one go.
const SCROLLBACK: usize = 5_000;
/// Lines left after a trim, so trimming (which redraws the whole log) is rare.
const SCROLLBACK_AFTER_TRIM: usize = 4_000;
const BODY_LIMIT: usize = 4_000;
/// Commands kept for Up / Down.
const HISTORY: usize = 200;

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

/// What printed a line, which decides its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineKind {
    /// The prompt and the command typed after it.
    Prompt,
    /// The command's standard output.
    Output,
    /// The command's standard error.
    Error,
    /// A reply from Kilo or a message from the app itself.
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConsoleLine {
    pub kind: LineKind,
    pub text: String,
}

/// What a push did to the scrollback, so the window can mirror it in the text editor
/// without rebuilding it on every line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConsoleChange {
    /// These lines were added at the end. `first` when the scrollback was empty before.
    Appended { text: String, first: bool },
    /// The oldest lines were dropped: redraw everything.
    Trimmed,
}

/// Scrollback and command history of the one terminal the window owns.
#[derive(Debug, Clone)]
pub(crate) struct Console {
    lines: Vec<ConsoleLine>,
    /// One kind per line, shared with the highlighter. Replaced (never mutated) on change,
    /// so the highlighter sees a new value and recolours.
    kinds: Arc<Vec<LineKind>>,
    history: Vec<String>,
    /// Position while stepping through history with Up / Down; `None` is the draft line.
    history_at: Option<usize>,
}

impl Console {
    pub(crate) fn new() -> Self {
        Self {
            lines: Vec::new(),
            kinds: Arc::new(Vec::new()),
            history: Vec::new(),
            history_at: None,
        }
    }

    pub(crate) fn kinds(&self) -> Arc<Vec<LineKind>> {
        Arc::clone(&self.kinds)
    }

    /// Every line joined, for rebuilding the text editor.
    pub(crate) fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Add `text` as one or more lines of `kind`. Escape sequences are dropped, since the
    /// log is plain text.
    pub(crate) fn push(&mut self, kind: LineKind, text: &str) -> ConsoleChange {
        let added: Vec<ConsoleLine> = text
            .split('\n')
            .map(|line| ConsoleLine {
                kind,
                text: strip_ansi(line.trim_end_matches('\r')),
            })
            .collect();
        let appended = added
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let first = self.lines.is_empty();
        self.lines.extend(added);
        let change = if self.lines.len() > SCROLLBACK {
            let extra = self.lines.len() - SCROLLBACK_AFTER_TRIM;
            self.lines.drain(0..extra);
            ConsoleChange::Trimmed
        } else {
            ConsoleChange::Appended { text: appended, first }
        };
        self.kinds = Arc::new(self.lines.iter().map(|line| line.kind).collect());
        change
    }

    /// A provider reply, labelled with who sent it.
    pub(crate) fn record(&mut self, exchange: ProviderExchange) -> ConsoleChange {
        self.push(LineKind::Note, &format!("[{}] {}", exchange.source, exchange.text))
    }

    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.kinds = Arc::new(Vec::new());
    }

    #[cfg(test)]
    pub(crate) fn lines(&self) -> &[ConsoleLine] {
        &self.lines
    }

    /// Remember a command for Up / Down. Repeating the last one is not stored twice.
    pub(crate) fn remember(&mut self, command: &str) {
        self.history_at = None;
        if self.history.last().map(String::as_str) == Some(command) {
            return;
        }
        self.history.push(command.to_string());
        if self.history.len() > HISTORY {
            self.history.remove(0);
        }
    }

    /// Step back (`-1`, Up) or forward (`1`, Down) through history. Forward past the newest
    /// returns to an empty draft, like a shell.
    pub(crate) fn step_history(&mut self, step: i32) -> Option<String> {
        if self.history.is_empty() {
            return None;
        }
        let last = self.history.len() - 1;
        let next = match (self.history_at, step < 0) {
            (None, true) => Some(last),
            (None, false) => return None,
            (Some(at), true) => Some(at.saturating_sub(1)),
            (Some(at), false) if at >= last => None,
            (Some(at), false) => Some(at + 1),
        };
        self.history_at = next;
        Some(next.map(|at| self.history[at].clone()).unwrap_or_default())
    }
}

/// What a typed line asks of the terminal before any shell sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Builtin {
    /// `clear`, `cls`, `Clear-Host`.
    Clear,
    /// `exit`: fold the terminal away.
    Exit,
}

pub(crate) fn builtin(command: &str) -> Option<Builtin> {
    match command.trim().to_ascii_lowercase().as_str() {
        "clear" | "cls" | "clear-host" => Some(Builtin::Clear),
        "exit" => Some(Builtin::Exit),
        _ => None,
    }
}

/// The prompt printed before a command. PowerShell's own shape on Windows, a POSIX one
/// elsewhere. The branch is added when the folder is inside the open repository.
pub(crate) fn prompt(cwd: &str, branch: Option<&str>) -> String {
    let branch = branch
        .filter(|name| !name.is_empty())
        .map(|name| format!(" [{name}]"))
        .unwrap_or_default();
    if cfg!(windows) {
        format!("PS {cwd}{branch}> ")
    } else {
        format!("{cwd}{branch} $ ")
    }
}

/// Drop CSI (`ESC [ … letter`) and OSC (`ESC ] … BEL`) sequences, and any lone ESC.
pub(crate) fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
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
    fn pushed_text_becomes_one_line_per_newline_with_its_kind() {
        let mut console = Console::new();
        assert_eq!(
            console.push(LineKind::Prompt, "PS D:\\r> git status"),
            ConsoleChange::Appended { text: "PS D:\\r> git status".into(), first: true }
        );
        assert_eq!(
            console.push(LineKind::Output, "a\r\nb"),
            ConsoleChange::Appended { text: "a\nb".into(), first: false }
        );
        assert_eq!(console.text(), "PS D:\\r> git status\na\nb");
        assert_eq!(*console.kinds(), vec![LineKind::Prompt, LineKind::Output, LineKind::Output]);
    }

    #[test]
    fn the_scrollback_drops_the_oldest_lines_past_its_limit() {
        let mut console = Console::new();
        for index in 0..SCROLLBACK {
            console.push(LineKind::Output, &index.to_string());
        }
        assert_eq!(console.push(LineKind::Output, "last"), ConsoleChange::Trimmed);
        assert_eq!(console.lines().len(), SCROLLBACK_AFTER_TRIM);
        assert_eq!(console.lines().last().map(|line| line.text.as_str()), Some("last"));
        assert_eq!(console.kinds().len(), SCROLLBACK_AFTER_TRIM);
    }

    #[test]
    fn provider_replies_are_labelled_notes() {
        let mut console = Console::new();
        console.record(ProviderExchange::kilo("model list is not connected"));
        assert_eq!(console.text(), "[Kilo] model list is not connected");
        assert_eq!(*console.kinds(), vec![LineKind::Note]);
        console.clear();
        assert!(console.lines().is_empty());
    }

    #[test]
    fn history_steps_back_and_forward_to_an_empty_draft() {
        let mut console = Console::new();
        assert_eq!(console.step_history(-1), None);
        console.remember("git status");
        console.remember("git log");
        console.remember("git log");
        assert_eq!(console.step_history(-1).as_deref(), Some("git log"));
        assert_eq!(console.step_history(-1).as_deref(), Some("git status"));
        assert_eq!(console.step_history(-1).as_deref(), Some("git status"));
        assert_eq!(console.step_history(1).as_deref(), Some("git log"));
        assert_eq!(console.step_history(1).as_deref(), Some(""));
        assert_eq!(console.step_history(1), None);
    }

    #[test]
    fn clear_and_exit_are_handled_before_the_shell() {
        assert_eq!(builtin(" cls "), Some(Builtin::Clear));
        assert_eq!(builtin("Clear-Host"), Some(Builtin::Clear));
        assert_eq!(builtin("exit"), Some(Builtin::Exit));
        assert_eq!(builtin("git status"), None);
    }

    #[test]
    fn the_prompt_names_the_folder_and_branch() {
        let shown = prompt("D:\\Repos\\ISL", Some("development"));
        assert!(shown.contains("D:\\Repos\\ISL [development]"), "{shown}");
        assert!(!prompt("D:\\x", None).contains('['));
    }

    #[test]
    fn colour_codes_are_stripped() {
        assert_eq!(strip_ansi("\u{1b}[31merror\u{1b}[0m: no"), "error: no");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}text"), "text");
    }

    #[test]
    fn a_local_failure_is_labelled_as_git() {
        let suggestion = CommitSuggestion::local_failure("Select a file first.");
        assert_eq!(suggestion.exchange.source, SOURCE_GIT);
        assert!(suggestion.subject.is_err());
    }
}
