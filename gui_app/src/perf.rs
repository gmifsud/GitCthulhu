//! How long each message costs: `update`, then the `view` rebuild iced does after it.
//!
//! Off unless the process starts with `GITCTHULHU_PERF=1`. Then one line per message goes to
//! stderr, so a run can be redirected to a file and read for the messages that dominate.
//! The message's name is read from its `Debug` form, cut off early: a history chunk's payload
//! would otherwise be formatted in full to learn one word.

use std::cell::Cell;
use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use iced::{Element, Task};

use crate::app::{GitApp, Message};

const ENV_VAR: &str = "GITCTHULHU_PERF";
/// Bytes of `Debug` output read to find the variant's name.
const NAME_BYTES: usize = 48;

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os(ENV_VAR).is_some_and(|value| value == "1"))
}

thread_local! {
    /// The last update's name and cost, reported with the view that follows it.
    static LAST_UPDATE: Cell<Option<(String, Duration)>> = const { Cell::new(None) };
}

/// `GitApp::update`, timed when enabled.
pub(crate) fn update(app: &mut GitApp, message: Message) -> Task<Message> {
    if !enabled() {
        return app.update(message);
    }
    let name = message_name(&message);
    let started = Instant::now();
    let task = app.update(message);
    LAST_UPDATE.with(|last| last.set(Some((name, started.elapsed()))));
    task
}

/// `GitApp::view`, timed when enabled, printed with the update that caused it.
pub(crate) fn view(app: &GitApp) -> Element<'_, Message> {
    if !enabled() {
        return app.view();
    }
    let started = Instant::now();
    let element = app.view();
    let view_cost = started.elapsed();
    let (name, update_cost) = LAST_UPDATE
        .with(Cell::take)
        .unwrap_or_else(|| ("(no update)".to_string(), Duration::ZERO));
    eprintln!(
        "perf msg={name} update={:.3}ms view={:.3}ms",
        update_cost.as_secs_f64() * 1_000.0,
        view_cost.as_secs_f64() * 1_000.0
    );
    element
}

/// The variant's name: `Debug` output up to its first space or bracket.
pub(crate) fn message_name(message: &Message) -> String {
    let mut head = Head::default();
    let _ = write!(head, "{message:?}");
    let text = head.text;
    let end = text.find([' ', '(', '{']).unwrap_or(text.len());
    text[..end].to_string()
}

/// A writer that refuses more than `NAME_BYTES`, so `Debug` stops formatting early.
#[derive(Default)]
struct Head {
    text: String,
}

impl std::fmt::Write for Head {
    fn write_str(&mut self, piece: &str) -> std::fmt::Result {
        if self.text.len() >= NAME_BYTES {
            return Err(std::fmt::Error);
        }
        let room = NAME_BYTES - self.text.len();
        let cut = piece
            .char_indices()
            .map(|(at, _)| at)
            .find(|at| *at >= room)
            .unwrap_or(piece.len());
        self.text.push_str(&piece[..cut]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_named_by_its_variant_alone() {
        assert_eq!(message_name(&Message::CloseContextMenu), "CloseContextMenu");
        assert_eq!(message_name(&Message::TerminalDragMoved { y: 2.0 }), "TerminalDragMoved");
        assert_eq!(message_name(&Message::RefFilterChanged("abc".into())), "RefFilterChanged");
    }

    #[test]
    fn naming_stops_formatting_after_a_few_bytes() {
        let mut head = Head::default();
        let _ = write!(head, "{:?}", "x".repeat(10_000));
        assert!(head.text.len() <= NAME_BYTES);
    }
}
