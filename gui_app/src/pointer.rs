//! Where the pointer is, kept out of the message loop.
//!
//! iced rebuilds the whole window after every message. A message per pointer move made every
//! pane rebuild at pointer rate for a value only two places read: the right-click menu, to
//! open where the pointer is, and the terminal edge, while it is dragged. The event
//! subscription writes the position here and sends no message; the drag alone subscribes to
//! moves as messages, and only while it lasts.
//!
//! The subscription runs on the executor and `update` on the window thread, so the cells are
//! atomics, not a thread-local.

use std::sync::atomic::{AtomicU32, Ordering};

static X: AtomicU32 = AtomicU32::new(0);
static Y: AtomicU32 = AtomicU32::new(0);

pub(crate) fn record(position: iced::Point) {
    X.store(position.x.to_bits(), Ordering::Relaxed);
    Y.store(position.y.to_bits(), Ordering::Relaxed);
}

/// The last recorded position, in window pixels.
pub(crate) fn position() -> (f32, f32) {
    (
        f32::from_bits(X.load(Ordering::Relaxed)),
        f32::from_bits(Y.load(Ordering::Relaxed)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_recorded_position_is_read_back() {
        record(iced::Point::new(12.5, 340.0));
        assert_eq!(position(), (12.5, 340.0));
    }
}
