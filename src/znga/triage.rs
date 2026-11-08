//! Local, model-free scoring of how much an incoming message needs the
//! person now. Runs on every message, so it must stay cheap and offline.

/// A score at or above this raises an alert.
pub const ALERT: u32 = 3;

#[derive(Clone, Copy, Debug, Default)]
pub struct Signals<'a> {
    pub text: &'a str,
    pub group: bool,
    pub mentions_me: bool,
    pub muted: bool,
    pub from_me: bool,
    /// Messages from the same sender in this chat within the last five
    /// minutes, this one included.
    pub burst: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Score {
    pub value: u32,
    pub reasons: Vec<&'static str>,
}

impl Score {
    pub fn alerts(&self) -> bool {
        self.value >= ALERT
    }

    pub fn summary(&self) -> String {
        self.reasons.join(", ")
    }
}

const URGENT: &[&str] = &[
    "urgent",
    "asap",
    "emergency",
    "help",
    "important",
    "call me",
    "call back",
    "pls call",
    "please call",
    "hospital",
    "accident",
    "right now",
    "immediately",
    "sos",
];
const TIME: &[&str] = &[
    "today",
    "tonight",
    "now",
    "tomorrow",
    "deadline",
    "this morning",
    "this evening",
    "in an hour",
    "by tonight",
    "by tomorrow",
];
const MONEY: &[&str] = &[
    "pay",
    "payment",
    "send money",
    "transfer",
    "invoice",
    "owe",
    "bill",
];
const QUESTION_STARTS: &[&str] = &[
    "can you",
    "could you",
    "will you",
    "are you",
    "did you",
    "do you",
    "have you",
    "where",
    "when",
    "what",
    "why",
    "how",
    "who",
    "should we",
    "shall we",
];

/// `text` lowercased with punctuation as spaces, padded so `" word "`
/// matches whole words and phrases.
fn words(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    for c in text.chars().flat_map(char::to_lowercase) {
        out.push(if c.is_alphanumeric() { c } else { ' ' });
    }
    out.push(' ');
    // Collapse runs of spaces so phrases match across punctuation.
    let mut collapsed = String::with_capacity(out.len());
    let mut space = false;
    for c in out.chars() {
        if c == ' ' {
            if !space {
                collapsed.push(c);
            }
            space = true;
        } else {
            collapsed.push(c);
            space = false;
        }
    }
    collapsed
}

/// A clock time such as "5pm", "10am" or "at 7".
fn has_clock_time(words: &str) -> bool {
    let tokens: Vec<&str> = words.split_whitespace().collect();
    tokens.iter().enumerate().any(|(i, token)| {
        let digits = token.trim_end_matches(|c: char| c.is_alphabetic());
        let suffix = &token[digits.len()..];
        let numeric = !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit());
        (numeric && matches!(suffix, "am" | "pm"))
            || (numeric
                && suffix.is_empty()
                && tokens
                    .get(i + 1)
                    .is_some_and(|next| matches!(*next, "am" | "pm")))
            || (*token == "at"
                && tokens.get(i + 1).is_some_and(|next| {
                    !next.is_empty() && next.chars().all(|c| c.is_ascii_digit())
                }))
    })
}

fn has_any(words: &str, list: &[&str]) -> bool {
    list.iter().any(|item| words.contains(&format!(" {item} ")))
}

pub fn score(signals: Signals<'_>) -> Score {
    let mut score = Score::default();
    if signals.from_me || signals.muted {
        return score;
    }
    let text = signals.text.trim();
    let w = words(text);
    let urgent = has_any(&w, URGENT);
    if urgent {
        score.value += 3;
        score.reasons.push("sounds urgent");
    }
    if signals.group && signals.mentions_me {
        score.value += 2;
        score.reasons.push("mentions you");
    }
    let asks = text.contains('?')
        || QUESTION_STARTS
            .iter()
            .any(|start| w.starts_with(&format!(" {start} ")));
    if asks && (!signals.group || signals.mentions_me) {
        score.value += 1;
        score.reasons.push("asks you something");
    }
    if has_any(&w, TIME) || has_clock_time(&w) {
        score.value += 1;
        score.reasons.push("time-sensitive");
    }
    if has_any(&w, MONEY) {
        score.value += 1;
        score.reasons.push("about money");
    }
    if signals.burst >= 3 {
        score.value += 2;
        score.reasons.push("writing repeatedly");
    }
    // A busy group is not an emergency unless it is about us or urgent.
    if signals.group && !signals.mentions_me && !urgent {
        score.value = score.value.min(ALERT - 1);
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct(text: &str) -> Signals<'_> {
        Signals {
            text,
            burst: 1,
            ..Signals::default()
        }
    }

    #[test]
    fn urgent_questions_alert_and_small_talk_does_not() {
        let urgent = score(direct("URGENT can you call me?"));
        assert!(urgent.alerts(), "{urgent:?}");
        assert!(urgent.reasons.contains(&"sounds urgent"));
        assert!(!score(direct("ok 👍")).alerts());
        assert!(!score(direct("haha nice")).alerts());
    }

    #[test]
    fn bursts_raise_the_score() {
        let calm = score(direct("are you there?"));
        let burst = score(Signals {
            burst: 3,
            ..direct("are you there?")
        });
        assert!(burst.value > calm.value);
        assert!(burst.alerts());
    }

    #[test]
    fn muted_chats_and_our_own_messages_never_alert() {
        let muted = Signals {
            muted: true,
            ..direct("URGENT call me now")
        };
        assert_eq!(score(muted).value, 0);
        let mine = Signals {
            from_me: true,
            ..direct("URGENT call me now")
        };
        assert_eq!(score(mine).value, 0);
    }

    #[test]
    fn groups_alert_only_when_about_us_or_urgent() {
        let chatter = Signals {
            group: true,
            burst: 5,
            ..direct("who's coming tonight? pay me back tomorrow")
        };
        assert!(!score(chatter).alerts());
        let mention = Signals {
            group: true,
            mentions_me: true,
            ..direct("@you can you bring the cake?")
        };
        assert!(score(mention).alerts());
    }

    #[test]
    fn words_match_whole_words_only() {
        assert!(!has_any(&words("helpful tip"), URGENT));
        assert!(has_any(&words("Help!!"), URGENT));
        assert!(has_any(&words("please, call me."), URGENT));
    }

    #[test]
    fn clock_times_count_but_the_word_am_does_not() {
        assert!(has_clock_time(&words("see you at 7")));
        assert!(has_clock_time(&words("dinner 8pm?")));
        assert!(has_clock_time(&words("by 10 am")));
        assert!(!has_clock_time(&words("I am fine")));
    }
}
