//! How the person writes, measured from their own sent messages, so the
//! twin's drafts sound like them. The persisted [`StyleProfile`] holds
//! numbers only; example messages stay in memory ([`Examples`]).

use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleProfile {
    pub messages: usize,
    pub average_words: f32,
    /// Share of messages starting with a lowercase letter.
    pub lowercase_start: f32,
    /// Share ending with a full stop.
    pub full_stop: f32,
    /// Share with at least one emoji.
    pub emoji: f32,
    pub exclamation: f32,
    /// Share with "haha", "lol", "hehe" and the like.
    pub laughter: f32,
    /// Most-used emoji, most first (at most three).
    pub top_emoji: Vec<String>,
}

pub fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0x2B00..=0x2BFF)
}

impl StyleProfile {
    pub fn measure<'a>(texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut profile = StyleProfile::default();
        let (mut words, mut lower, mut stop, mut emoji, mut bang, mut laugh) =
            (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
        let mut counts: BTreeMap<char, usize> = BTreeMap::new();
        for text in texts {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            profile.messages += 1;
            words += text.split_whitespace().count();
            if text
                .chars()
                .find(|c| c.is_alphabetic())
                .is_some_and(char::is_lowercase)
            {
                lower += 1;
            }
            if text.ends_with('.') && !text.ends_with("...") {
                stop += 1;
            }
            let mut any = false;
            for c in text.chars().filter(|c| is_emoji(*c)) {
                any = true;
                *counts.entry(c).or_default() += 1;
            }
            emoji += usize::from(any);
            bang += usize::from(text.contains('!'));
            let lowered = text.to_lowercase();
            laugh += usize::from(
                ["haha", "lol", "hehe", "lmao", "😂", "🤣"]
                    .iter()
                    .any(|l| lowered.contains(l)),
            );
        }
        if profile.messages == 0 {
            return profile;
        }
        let n = profile.messages as f32;
        profile.average_words = words as f32 / n;
        profile.lowercase_start = lower as f32 / n;
        profile.full_stop = stop as f32 / n;
        profile.emoji = emoji as f32 / n;
        profile.exclamation = bang as f32 / n;
        profile.laughter = laugh as f32 / n;
        let mut top: Vec<(char, usize)> = counts.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        profile.top_emoji = top
            .into_iter()
            .take(3)
            .map(|(c, _)| c.to_string())
            .collect();
        profile
    }

    /// Plain-English instructions describing the style, for prompts.
    pub fn describe(&self) -> String {
        if self.messages < 3 {
            return "Not much is known about how they write yet: keep it short, warm and natural."
                .into();
        }
        let mut lines = Vec::new();
        lines.push(match self.average_words {
            w if w < 6.0 => "Very short messages (a few words).".to_owned(),
            w if w < 14.0 => format!("Short messages, about {} words.", w.round() as u32),
            w => format!("Fairly long messages, about {} words.", w.round() as u32),
        });
        if self.lowercase_start > 0.6 {
            lines.push("Usually starts in lowercase.".into());
        }
        lines.push(if self.full_stop < 0.2 {
            "Rarely ends with a full stop.".into()
        } else {
            "Uses full stops.".into()
        });
        if self.emoji > 0.3 {
            let favourites = if self.top_emoji.is_empty() {
                String::new()
            } else {
                format!(" (favourites: {})", self.top_emoji.join(" "))
            };
            lines.push(format!("Uses emoji often{favourites}."));
        } else if self.emoji < 0.05 {
            lines.push("Almost never uses emoji.".into());
        }
        if self.exclamation > 0.3 {
            lines.push("Uses exclamation marks a lot.".into());
        }
        if self.laughter > 0.15 {
            lines.push("Often laughs in text (haha, lol).".into());
        }
        lines.join(" ")
    }
}

/// Recent messages the person sent, in memory only, for few-shot style.
#[derive(Default)]
pub struct Examples {
    /// (account/chat key, text), newest last.
    items: VecDeque<(String, String)>,
}

const MAX_EXAMPLES: usize = 300;

impl Examples {
    pub fn add(&mut self, chat_key: &str, text: &str) {
        let text = text.trim();
        if text.is_empty() || text.len() > 500 {
            return;
        }
        if self
            .items
            .iter()
            .any(|(key, known)| key == chat_key && known == text)
        {
            return;
        }
        self.items.push_back((chat_key.to_owned(), text.to_owned()));
        while self.items.len() > MAX_EXAMPLES {
            self.items.pop_front();
        }
    }

    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|(_, text)| text.as_str())
    }

    /// Every example sent from `account`, oldest first.
    pub fn texts_for_account(&self, account: &str) -> Vec<&str> {
        let prefix = format!("{account}/");
        self.items
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .map(|(_, text)| text.as_str())
            .collect()
    }

    /// Up to `n` examples, those from `chat_key` first, newest first.
    pub fn pick(&self, chat_key: &str, n: usize) -> Vec<String> {
        let mut out: Vec<String> = self
            .items
            .iter()
            .rev()
            .filter(|(key, _)| key == chat_key)
            .map(|(_, text)| text.clone())
            .take(n)
            .collect();
        for (key, text) in self.items.iter().rev() {
            if out.len() >= n {
                break;
            }
            if key != chat_key {
                out.push(text.clone());
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lowercase_no_stop_emoji_writer_is_described_that_way() {
        let profile = StyleProfile::measure([
            "haha yes 😂",
            "omw",
            "ok see u soon 😂",
            "lol no way",
            "sure thing",
        ]);
        assert_eq!(profile.messages, 5);
        assert!(profile.lowercase_start > 0.9);
        assert!(profile.full_stop < 0.1);
        let text = profile.describe();
        assert!(text.contains("lowercase"), "{text}");
        assert!(text.contains("Rarely ends with a full stop"), "{text}");
        assert!(text.contains("laughs"), "{text}");
        assert_eq!(profile.top_emoji, vec!["😂"]);
    }

    #[test]
    fn a_formal_writer_is_described_that_way() {
        let profile = StyleProfile::measure([
            "Thank you for the update.",
            "I will review the document tomorrow morning.",
            "Please let me know if anything changes.",
        ]);
        let text = profile.describe();
        assert!(text.contains("Uses full stops"), "{text}");
        assert!(text.contains("Almost never uses emoji"), "{text}");
    }

    #[test]
    fn the_profile_serializes_without_message_text() {
        let profile = StyleProfile::measure(["secret plans for friday 🎉"]);
        let json = serde_json::to_string(&profile).unwrap();
        assert!(!json.contains("secret"));
        assert!(!json.contains("friday"));
    }

    #[test]
    fn examples_prefer_the_chat_and_skip_duplicates() {
        let mut examples = Examples::default();
        examples.add("1/a", "hi mom");
        examples.add("1/b", "yo");
        examples.add("1/a", "hi mom");
        examples.add("1/a", "love you");
        assert_eq!(examples.len(), 3);
        assert_eq!(examples.pick("1/a", 2), vec!["love you", "hi mom"]);
        assert_eq!(examples.pick("1/a", 3).last().unwrap(), "yo");
    }
}
