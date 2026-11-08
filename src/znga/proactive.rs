//! What Znga has seen, and when it should speak up on its own: alerts,
//! briefings, notes to ourselves, and check-ins with close ones.
//! Pure state machines with time passed in, so they are unit-tested.

use std::collections::{HashMap, VecDeque};

use super::config::{Config, chat_key};
use super::triage::{self, Score, Signals};

/// One message Znga observed from a visible chat. In memory only.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub account: String,
    pub chat: String,
    pub chat_title: String,
    pub group: bool,
    pub sender: String,
    pub sender_name: String,
    pub text: String,
    /// Unix seconds.
    pub at: i64,
    pub score: Score,
}

impl Seen {
    pub fn key(&self) -> String {
        chat_key(&self.account, &self.chat)
    }
}

/// An incoming message the person should look at now.
#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub account: String,
    pub chat: String,
    pub chat_title: String,
    pub sender_name: String,
    pub text: String,
    pub reason: String,
    pub at: i64,
}

/// What [`Activity::observe`] is told about a message besides its text.
#[derive(Clone, Copy, Debug, Default)]
pub struct Context {
    pub mentions_me: bool,
    pub muted: bool,
}

const KEEP: usize = 600;
const BURST_WINDOW: i64 = 5 * 60;
const ALERT_GAP: i64 = 10 * 60;

#[derive(Default)]
pub struct Activity {
    seen: VecDeque<Seen>,
    /// Messages seen since the last briefing.
    since_briefing: usize,
    last_briefing: Option<i64>,
    last_alert: HashMap<String, i64>,
    bursts: HashMap<(String, String), VecDeque<i64>>,
}

impl Activity {
    /// Records `seen` (with its score filled in) and returns an alert when
    /// it needs the person and this chat has not alerted recently.
    pub fn observe(&mut self, mut seen: Seen, context: Context) -> Option<Alert> {
        let burst_key = (seen.key(), seen.sender.clone());
        let times = self.bursts.entry(burst_key).or_default();
        times.push_back(seen.at);
        while times.front().is_some_and(|t| seen.at - t > BURST_WINDOW) {
            times.pop_front();
        }
        seen.score = triage::score(Signals {
            text: &seen.text,
            group: seen.group,
            mentions_me: context.mentions_me,
            muted: context.muted,
            from_me: false,
            burst: times.len(),
        });
        let alert = (seen.score.alerts()
            && self
                .last_alert
                .get(&seen.key())
                .is_none_or(|last| seen.at - last >= ALERT_GAP))
        .then(|| Alert {
            account: seen.account.clone(),
            chat: seen.chat.clone(),
            chat_title: seen.chat_title.clone(),
            sender_name: seen.sender_name.clone(),
            text: seen.text.clone(),
            reason: seen.score.summary(),
            at: seen.at,
        });
        if let Some(alert) = &alert {
            self.last_alert.insert(seen.key(), alert.at);
        }
        self.seen.push_back(seen);
        self.since_briefing += 1;
        while self.seen.len() > KEEP {
            self.seen.pop_front();
            self.since_briefing = self.since_briefing.min(self.seen.len());
        }
        alert
    }

    /// Whether a proactive briefing is due: the person is away, enough
    /// happened, and the last briefing was long enough ago.
    pub fn due(&self, config: &Config, now: i64, away: bool) -> bool {
        config.briefings
            && away
            && self.since_briefing >= config.briefing_min_messages.max(1)
            && self
                .last_briefing
                .is_none_or(|last| now - last >= config.briefing_interval_minutes as i64 * 60)
    }

    /// Messages since the last briefing, oldest first.
    pub fn since_last_briefing(&self) -> Vec<Seen> {
        let skip = self.seen.len().saturating_sub(self.since_briefing);
        self.seen.iter().skip(skip).cloned().collect()
    }

    pub fn pending(&self) -> usize {
        self.since_briefing
    }

    pub fn mark_briefed(&mut self, now: i64) {
        self.since_briefing = 0;
        self.last_briefing = Some(now);
    }

    /// The last `n` messages seen in a chat, oldest first.
    pub fn recent_in(&self, account: &str, chat: &str, n: usize) -> Vec<Seen> {
        let mut out: Vec<Seen> = self
            .seen
            .iter()
            .rev()
            .filter(|seen| seen.account == account && seen.chat == chat)
            .take(n)
            .cloned()
            .collect();
        out.reverse();
        out
    }

    /// The last `n` messages seen anywhere, oldest first.
    pub fn recent(&self, n: usize) -> Vec<Seen> {
        let skip = self.seen.len().saturating_sub(n);
        self.seen.iter().skip(skip).cloned().collect()
    }

    pub fn forget_chat(&mut self, account: &str, chat: &str) {
        let before = self.seen.len();
        self.seen
            .retain(|seen| !(seen.account == account && seen.chat == chat));
        let removed = before - self.seen.len();
        self.since_briefing = self.since_briefing.saturating_sub(removed);
    }
}

/// Briefings copied to our own chat: at most [`SelfNotes::PER_DAY`] a day.
#[derive(Default)]
pub struct SelfNotes {
    day: i64,
    sent: u32,
}

impl SelfNotes {
    pub const PER_DAY: u32 = 6;

    /// Takes one of today's notes, if any are left.
    pub fn take(&mut self, config: &Config, now: i64) -> bool {
        if !config.self_notes {
            return false;
        }
        let day = now.div_euclid(86_400);
        if day != self.day {
            self.day = day;
            self.sent = 0;
        }
        if self.sent >= Self::PER_DAY {
            return false;
        }
        self.sent += 1;
        true
    }
}

/// Our own chat's id from our JID (`123:4@s.whatsapp.net` → `123@s.whatsapp.net`).
pub fn own_chat(me: &str) -> Option<String> {
    let (user, server) = me.split_once('@')?;
    let user = user.split(':').next()?;
    (server == "s.whatsapp.net" && !user.is_empty() && user.chars().all(|c| c.is_ascii_digit()))
        .then(|| format!("{user}@s.whatsapp.net"))
}

/// Twin chats quiet for `config.checkin_days` and not suggested within that
/// period. `chats` holds (account, chat, last activity in Unix seconds).
pub fn checkins_due(
    config: &Config,
    chats: &[(String, String, i64)],
    suggested: &HashMap<String, i64>,
    now: i64,
) -> Vec<(String, String)> {
    let period = i64::from(config.checkin_days.max(1)) * 86_400;
    chats
        .iter()
        .filter(|(account, chat, last)| {
            let key = chat_key(account, chat);
            config.twin_chats.contains(&key)
                && *last > 0
                && now - last >= period
                && suggested.get(&key).is_none_or(|at| now - at >= period)
        })
        .map(|(account, chat, _)| (account.clone(), chat.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(chat: &str, sender: &str, text: &str, at: i64) -> Seen {
        Seen {
            account: "1".into(),
            chat: chat.into(),
            chat_title: chat.into(),
            group: false,
            sender: sender.into(),
            sender_name: sender.into(),
            text: text.into(),
            at,
            score: Score::default(),
        }
    }

    #[test]
    fn an_urgent_message_alerts_once_per_ten_minutes_per_chat() {
        let mut activity = Activity::default();
        let first = activity.observe(seen("a", "x", "URGENT call me", 1000), Context::default());
        assert!(first.is_some());
        let again = activity.observe(seen("a", "x", "URGENT call me", 1300), Context::default());
        assert!(again.is_none());
        let other = activity.observe(seen("b", "y", "URGENT call me", 1300), Context::default());
        assert!(other.is_some());
        let later = activity.observe(
            seen("a", "x", "URGENT call me", 1000 + 601),
            Context::default(),
        );
        assert!(later.is_some());
    }

    #[test]
    fn three_quick_messages_from_one_sender_alert() {
        let mut activity = Activity::default();
        assert!(
            activity
                .observe(seen("a", "x", "hey?", 0), Context::default())
                .is_none()
        );
        assert!(
            activity
                .observe(seen("a", "x", "hello?", 60), Context::default())
                .is_none()
        );
        assert!(
            activity
                .observe(seen("a", "x", "you there?", 120), Context::default())
                .is_some()
        );
    }

    #[test]
    fn muted_chats_never_alert() {
        let mut activity = Activity::default();
        let muted = Context {
            muted: true,
            ..Context::default()
        };
        assert!(
            activity
                .observe(seen("a", "x", "URGENT", 0), muted)
                .is_none()
        );
    }

    #[test]
    fn briefings_wait_for_absence_volume_and_interval() {
        let config = Config {
            briefing_min_messages: 2,
            briefing_interval_minutes: 30,
            ..Config::default()
        };
        let mut activity = Activity::default();
        activity.observe(seen("a", "x", "one", 0), Context::default());
        assert!(!activity.due(&config, 100, true), "too few");
        activity.observe(seen("a", "x", "two", 10), Context::default());
        assert!(!activity.due(&config, 100, false), "reading the window");
        assert!(activity.due(&config, 100, true));
        assert_eq!(activity.since_last_briefing().len(), 2);
        activity.mark_briefed(100);
        activity.observe(seen("a", "x", "three", 200), Context::default());
        activity.observe(seen("a", "x", "four", 210), Context::default());
        assert_eq!(activity.since_last_briefing().len(), 2);
        assert!(!activity.due(&config, 100 + 29 * 60, true), "interval");
        assert!(activity.due(&config, 100 + 30 * 60, true));
        let off = Config {
            briefings: false,
            ..config
        };
        assert!(!activity.due(&off, 100 + 30 * 60, true));
    }

    #[test]
    fn self_notes_are_off_by_default_and_capped_per_day() {
        let mut notes = SelfNotes::default();
        assert!(!notes.take(&Config::default(), 0));
        let on = Config {
            self_notes: true,
            ..Config::default()
        };
        for _ in 0..SelfNotes::PER_DAY {
            assert!(notes.take(&on, 10));
        }
        assert!(!notes.take(&on, 20), "the seventh note waits");
        assert!(notes.take(&on, 86_400 + 5), "a new day");
    }

    #[test]
    fn our_own_chat_drops_the_device_suffix() {
        assert_eq!(
            own_chat("4915123:12@s.whatsapp.net").as_deref(),
            Some("4915123@s.whatsapp.net")
        );
        assert_eq!(
            own_chat("4915123@s.whatsapp.net").as_deref(),
            Some("4915123@s.whatsapp.net")
        );
        assert_eq!(own_chat("abc@lid"), None);
    }

    #[test]
    fn check_ins_come_once_per_period_for_quiet_twin_chats() {
        let mut config = Config {
            checkin_days: 7,
            ..Config::default()
        };
        config.twin_chats.insert(chat_key("1", "mom"));
        let day = 86_400;
        let chats = vec![
            ("1".to_owned(), "mom".to_owned(), 1),
            ("1".to_owned(), "bob".to_owned(), 1),
        ];
        let mut suggested = HashMap::new();
        assert!(checkins_due(&config, &chats, &suggested, 6 * day).is_empty());
        let due = checkins_due(&config, &chats, &suggested, 8 * day);
        assert_eq!(due, vec![("1".to_owned(), "mom".to_owned())]);
        suggested.insert(chat_key("1", "mom"), 8 * day);
        assert!(checkins_due(&config, &chats, &suggested, 10 * day).is_empty());
        assert_eq!(checkins_due(&config, &chats, &suggested, 15 * day).len(), 1);
    }
}
