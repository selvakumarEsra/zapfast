//! Older history for chosen chats (REQ-EXTRAS-010): asks the person's own
//! phone, the way scrolling to the top of a chat does, spaced and capped
//! because every request wakes the phone.

use std::time::{Duration, Instant};

/// Time between two requests in one run.
pub const GAP: Duration = Duration::from_secs(15);
/// Requests per chat in one run (the phone answers 50 messages each).
pub const PAGES: u32 = 10;

struct Target {
    chat: String,
    asked: u32,
}

/// One run over the chats the person chose. Pure: the clock, the link and
/// the archive's state are passed in.
#[derive(Default)]
pub struct Fetch {
    targets: Vec<Target>,
    next: usize,
    last: Option<Instant>,
    running: bool,
    asked: u32,
    /// Set once when a run ends, so the dashboard reads the archive again.
    ended: bool,
}

impl Fetch {
    pub fn start(&mut self, chats: impl IntoIterator<Item = String>) {
        self.targets = chats
            .into_iter()
            .map(|chat| Target { chat, asked: 0 })
            .collect();
        self.next = 0;
        self.asked = 0;
        self.running = !self.targets.is_empty();
        self.ended = false;
    }

    pub fn stop(&mut self) {
        if self.running {
            self.running = false;
            self.ended = true;
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    /// Requests sent in this run, and the most it may send.
    pub fn progress(&self) -> (u32, u32) {
        (self.asked, self.targets.len() as u32 * PAGES)
    }

    pub fn is_target(&self, chat: &str) -> bool {
        self.running && self.targets.iter().any(|target| target.chat == chat)
    }

    /// Whether the run has just ended; true once.
    pub fn take_ended(&mut self) -> bool {
        std::mem::take(&mut self.ended)
    }

    /// The chat to ask the phone about now, if any. `at_start` says which
    /// chats the phone already said have nothing older.
    pub fn next(
        &mut self,
        now: Instant,
        connected: bool,
        at_start: impl Fn(&str) -> bool,
    ) -> Option<String> {
        if !self.running || !connected {
            return None;
        }
        if self.last.is_some_and(|last| now.duration_since(last) < GAP) {
            return None;
        }
        self.targets
            .retain(|target| target.asked < PAGES && !at_start(&target.chat));
        if self.targets.is_empty() {
            self.stop();
            return None;
        }
        // Round robin, so every chosen chat moves forward together.
        let index = self.next % self.targets.len();
        self.next = index + 1;
        let target = &mut self.targets[index];
        target.asked += 1;
        self.asked += 1;
        self.last = Some(now);
        Some(target.chat.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_spaced_and_capped() {
        let mut fetch = Fetch::default();
        fetch.start(["a".to_owned(), "b".to_owned()]);
        let t0 = Instant::now();
        assert_eq!(fetch.next(t0, true, |_| false).as_deref(), Some("a"));
        assert_eq!(fetch.next(t0 + GAP / 2, true, |_| false), None, "spaced");
        assert_eq!(fetch.next(t0 + GAP, true, |_| false).as_deref(), Some("b"));
        let mut now = t0 + GAP;
        let mut asked = 2;
        loop {
            now += GAP;
            if fetch.next(now, true, |_| false).is_none() {
                break;
            }
            asked += 1;
        }
        assert_eq!(asked, 2 * PAGES, "capped per chat");
        assert!(!fetch.running());
        assert!(fetch.take_ended());
        assert!(!fetch.take_ended(), "once");
    }

    #[test]
    fn a_chat_at_its_start_is_skipped() {
        let mut fetch = Fetch::default();
        fetch.start(["a".to_owned(), "b".to_owned()]);
        let t0 = Instant::now();
        assert_eq!(
            fetch.next(t0, true, |chat| chat == "a").as_deref(),
            Some("b")
        );
        assert_eq!(
            fetch.next(t0 + GAP, true, |chat| chat == "a").as_deref(),
            Some("b")
        );
        assert_eq!(fetch.next(t0 + GAP * 2, true, |_| true), None);
        assert!(!fetch.running(), "every chat is done");
    }

    #[test]
    fn nothing_is_asked_while_offline_or_stopped() {
        let mut fetch = Fetch::default();
        let t0 = Instant::now();
        assert_eq!(fetch.next(t0, true, |_| false), None, "not started");
        fetch.start(["a".to_owned()]);
        assert_eq!(fetch.next(t0, false, |_| false), None, "offline");
        assert!(fetch.running(), "waits for the phone");
        fetch.stop();
        assert_eq!(fetch.next(t0, true, |_| false), None, "stopped");
        assert!(fetch.take_ended());
    }
}
