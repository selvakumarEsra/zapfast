//! Insights' numbers (REQ-EXTRAS-003/004), computed from records that carry
//! only time, chat, direction and kind. Text is present only when the person
//! turned content analysis on. Pure functions with the clock passed in.

use std::collections::{HashMap, HashSet};

/// What a message was, as Insights groups them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Text,
    Photo,
    Video,
    Voice,
    Document,
    Sticker,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Text,
        Kind::Photo,
        Kind::Video,
        Kind::Voice,
        Kind::Document,
        Kind::Sticker,
        Kind::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Text => "Text",
            Kind::Photo => "Photos",
            Kind::Video => "Videos",
            Kind::Voice => "Voice & audio",
            Kind::Document => "Documents",
            Kind::Sticker => "Stickers",
            Kind::Other => "Other",
        }
    }

    /// The archive's content kind (`$.kind`); `None` for what is not a
    /// message someone wrote (deletions, timers, calls, notices).
    pub fn from_archive(kind: &str) -> Option<Kind> {
        Some(match kind {
            "text" | "interactive" => Kind::Text,
            "image" => Kind::Photo,
            "video" => Kind::Video,
            "audio" => Kind::Voice,
            "document" => Kind::Document,
            "sticker" | "sticker_pack" => Kind::Sticker,
            "location" | "livelocation" | "contact" | "poll" => Kind::Other,
            _ => return None,
        })
    }
}

/// One message, as Insights sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// Index into [`Source::chats`].
    pub chat: u32,
    pub from_me: bool,
    /// Unix seconds.
    pub at: i64,
    /// Local calendar day, as days since 1970-01-01.
    pub day: i32,
    /// Local hour, 0–23.
    pub hour: u8,
    /// Local weekday, 0 = Monday.
    pub weekday: u8,
    pub kind: Kind,
    /// Only with content analysis on.
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatMeta {
    pub id: String,
    pub title: String,
    pub group: bool,
}

/// Records from one place: this account's archive or a chat export.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub name: String,
    pub chats: Vec<ChatMeta>,
    /// Oldest first.
    pub records: Vec<Record>,
    /// Whether the records carry text.
    pub with_text: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Period {
    Week,
    #[default]
    Month,
    Quarter,
    Year,
    All,
}

impl Period {
    pub const ALL: [Period; 5] = [
        Period::Week,
        Period::Month,
        Period::Quarter,
        Period::Year,
        Period::All,
    ];

    pub fn days(self) -> Option<i32> {
        match self {
            Period::Week => Some(7),
            Period::Month => Some(30),
            Period::Quarter => Some(90),
            Period::Year => Some(365),
            Period::All => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Period::Week => "7 days",
            Period::Month => "30 days",
            Period::Quarter => "90 days",
            Period::Year => "Year",
            Period::All => "All time",
        }
    }
}

/// A bar of the trend chart.
#[derive(Clone, Debug, PartialEq)]
pub struct Bucket {
    /// First day of the bucket.
    pub day: i32,
    /// Days the bucket spans: 1, 7 or 30.
    pub span: i32,
    pub sent: u32,
    pub received: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChatStats {
    pub chat: u32,
    pub title: String,
    pub id: String,
    pub group: bool,
    pub total: u32,
    pub sent: u32,
    pub received: u32,
    /// Of all messages in the period.
    pub share: f32,
    /// Median seconds the person took to answer.
    pub my_reply: Option<i64>,
    /// Median seconds the other side took (private chats).
    pub their_reply: Option<i64>,
    pub last_at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quiet {
    pub title: String,
    pub id: String,
    /// Messages in the 90 days before the last 30.
    pub before: u32,
    pub last_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Words {
    pub top_words: Vec<(String, u32)>,
    pub top_emoji: Vec<(String, u32)>,
    pub my_average: f32,
    pub their_average: f32,
    pub texts: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub total: u32,
    pub sent: u32,
    pub received: u32,
    /// The same span just before, for the change shown beside the total.
    pub previous_total: Option<u32>,
    pub active_chats: u32,
    pub active_days: u32,
    /// Days the period covers (from the first record for All time).
    pub days: u32,
    pub series: Vec<Bucket>,
    /// Messages by weekday (0 = Monday) and hour.
    pub heatmap: [[u32; 24]; 7],
    pub busiest_hour: Option<(u8, u32)>,
    pub busiest_weekday: Option<(u8, u32)>,
    pub busiest_day: Option<(i32, u32)>,
    /// Consecutive days on which the person sent something.
    pub longest_streak: u32,
    pub current_streak: u32,
    pub kinds: Vec<(Kind, u32)>,
    pub private_messages: u32,
    pub group_messages: u32,
    pub top: Vec<ChatStats>,
    pub quiet: Vec<Quiet>,
    /// Private conversations (after 6 hours of quiet) the person started.
    pub started_by_me: u32,
    pub started_by_them: u32,
    pub my_reply: Option<i64>,
    pub their_reply: Option<i64>,
    pub words: Option<Words>,
}

impl Stats {
    pub fn daily_average(&self) -> f32 {
        self.total as f32 / self.days.max(1) as f32
    }
}

/// A reply counts within this gap.
pub const REPLY_WINDOW: i64 = 12 * 3600;
/// Quiet this long starts a new conversation.
pub const CONVERSATION_GAP: i64 = 6 * 3600;
const TOP_CHATS: usize = 12;

fn median(values: &mut [i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2
    } else {
        values[middle]
    })
}

/// The longest run of consecutive days in `days`, and the run ending on
/// `today` or yesterday.
fn streaks(days: &HashSet<i32>, today: i32) -> (u32, u32) {
    let mut sorted: Vec<i32> = days.iter().copied().collect();
    sorted.sort_unstable();
    let (mut longest, mut run, mut previous) = (0u32, 0u32, None::<i32>);
    for day in &sorted {
        run = if previous == Some(day - 1) {
            run + 1
        } else {
            1
        };
        longest = longest.max(run);
        previous = Some(*day);
    }
    let mut current = 0;
    let mut day = if days.contains(&today) {
        today
    } else {
        today - 1
    };
    while days.contains(&day) {
        current += 1;
        day -= 1;
    }
    (longest, current)
}

/// Every number for `period`, with `today` the local day and `now` the time.
pub fn compute(source: &Source, period: Period, today: i32) -> Stats {
    let start = period.days().map(|days| today - days + 1);
    let in_period = |record: &&Record| start.is_none_or(|start| record.day >= start);
    let records: Vec<&Record> = source.records.iter().filter(in_period).collect();
    let mut stats = Stats {
        previous_total: period.days().map(|days| {
            let from = today - 2 * days + 1;
            let to = today - days;
            source
                .records
                .iter()
                .filter(|record| record.day >= from && record.day <= to)
                .count() as u32
        }),
        ..Stats::default()
    };
    let first_day = records.first().map_or(today, |record| record.day);
    stats.days = match period.days() {
        Some(days) => days as u32,
        None => (today - first_day + 1).max(1) as u32,
    };
    let span = match stats.days {
        0..=120 => 1,
        121..=730 => 7,
        _ => 30,
    };
    let series_start = start.unwrap_or(first_day);
    let buckets = ((today - series_start) / span + 1).max(1);
    stats.series = (0..buckets)
        .map(|index| Bucket {
            day: series_start + index * span,
            span,
            sent: 0,
            received: 0,
        })
        .collect();

    let mut per_day: HashMap<i32, u32> = HashMap::new();
    let mut sent_days = HashSet::new();
    let mut kinds: HashMap<Kind, u32> = HashMap::new();
    let mut per_chat: HashMap<u32, ChatStats> = HashMap::new();
    // Per chat: the last message's direction and time, and the reply gaps.
    let mut last: HashMap<u32, (bool, i64)> = HashMap::new();
    let mut my_gaps: HashMap<u32, Vec<i64>> = HashMap::new();
    let mut their_gaps: HashMap<u32, Vec<i64>> = HashMap::new();
    for record in &records {
        stats.total += 1;
        let meta = &source.chats[record.chat as usize];
        if record.from_me {
            stats.sent += 1;
            sent_days.insert(record.day);
        } else {
            stats.received += 1;
        }
        if meta.group {
            stats.group_messages += 1;
        } else {
            stats.private_messages += 1;
        }
        *per_day.entry(record.day).or_default() += 1;
        stats.heatmap[record.weekday as usize % 7][record.hour as usize % 24] += 1;
        *kinds.entry(record.kind).or_default() += 1;
        let index = ((record.day - series_start) / span) as usize;
        if let Some(bucket) = stats.series.get_mut(index) {
            if record.from_me {
                bucket.sent += 1;
            } else {
                bucket.received += 1;
            }
        }
        let chat = per_chat.entry(record.chat).or_insert_with(|| ChatStats {
            chat: record.chat,
            title: meta.title.clone(),
            id: meta.id.clone(),
            group: meta.group,
            total: 0,
            sent: 0,
            received: 0,
            share: 0.0,
            my_reply: None,
            their_reply: None,
            last_at: 0,
        });
        chat.total += 1;
        if record.from_me {
            chat.sent += 1;
        } else {
            chat.received += 1;
        }
        chat.last_at = chat.last_at.max(record.at);
        let previous = last.insert(record.chat, (record.from_me, record.at));
        if !meta.group {
            match previous {
                Some((_, at)) if record.at - at < CONVERSATION_GAP => {}
                _ if record.from_me => stats.started_by_me += 1,
                _ => stats.started_by_them += 1,
            }
        }
        if let Some((was_me, at)) = previous
            && was_me != record.from_me
            && record.at - at <= REPLY_WINDOW
        {
            let gap = (record.at - at).max(0);
            if record.from_me {
                my_gaps.entry(record.chat).or_default().push(gap);
            } else if !meta.group {
                their_gaps.entry(record.chat).or_default().push(gap);
            }
        }
    }
    stats.active_chats = per_chat.len() as u32;
    stats.active_days = per_day.len() as u32;
    stats.busiest_day = per_day
        .iter()
        .map(|(day, count)| (*day, *count))
        .max_by_key(|(day, count)| (*count, *day));
    let hours: Vec<u32> = (0..24)
        .map(|hour| stats.heatmap.iter().map(|row| row[hour]).sum())
        .collect();
    stats.busiest_hour = hours
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .max_by_key(|(hour, count)| (**count, std::cmp::Reverse(*hour)))
        .map(|(hour, count)| (hour as u8, *count));
    stats.busiest_weekday = (0..7)
        .map(|weekday| (weekday as u8, stats.heatmap[weekday].iter().sum::<u32>()))
        .filter(|(_, count)| *count > 0)
        .max_by_key(|(weekday, count)| (*count, std::cmp::Reverse(*weekday)));
    (stats.longest_streak, stats.current_streak) = streaks(&sent_days, today);
    stats.kinds = Kind::ALL
        .iter()
        .filter_map(|kind| kinds.get(kind).map(|count| (*kind, *count)))
        .collect();
    let mut all_mine: Vec<i64> = my_gaps.values().flatten().copied().collect();
    let mut all_theirs: Vec<i64> = their_gaps.values().flatten().copied().collect();
    stats.my_reply = median(&mut all_mine);
    stats.their_reply = median(&mut all_theirs);
    let total = stats.total.max(1) as f32;
    let mut top: Vec<ChatStats> = per_chat
        .into_values()
        .map(|mut chat| {
            chat.share = chat.total as f32 / total;
            chat.my_reply = my_gaps.get_mut(&chat.chat).and_then(|gaps| median(gaps));
            chat.their_reply = their_gaps.get_mut(&chat.chat).and_then(|gaps| median(gaps));
            chat
        })
        .collect();
    top.sort_by(|a, b| b.total.cmp(&a.total).then(a.title.cmp(&b.title)));
    top.truncate(TOP_CHATS);
    stats.top = top;
    stats.quiet = quiet(source, today);
    if source.with_text {
        stats.words = Some(words(records.iter().copied()));
    }
    stats
}

/// Private chats with at least 20 messages in the 90 days before the last
/// 30, and none in the last 30 days: people the person has drifted from.
pub fn quiet(source: &Source, today: i32) -> Vec<Quiet> {
    let recent_start = today - 29;
    let before_start = recent_start - 90;
    let mut before: HashMap<u32, u32> = HashMap::new();
    let mut recent: HashSet<u32> = HashSet::new();
    let mut last_at: HashMap<u32, i64> = HashMap::new();
    for record in &source.records {
        if source.chats[record.chat as usize].group {
            continue;
        }
        if record.day >= recent_start {
            recent.insert(record.chat);
        } else if record.day >= before_start {
            *before.entry(record.chat).or_default() += 1;
        }
        let at = last_at.entry(record.chat).or_default();
        *at = (*at).max(record.at);
    }
    let mut quiet: Vec<Quiet> = before
        .into_iter()
        .filter(|(chat, count)| *count >= 20 && !recent.contains(chat))
        .map(|(chat, count)| {
            let meta = &source.chats[chat as usize];
            Quiet {
                title: meta.title.clone(),
                id: meta.id.clone(),
                before: count,
                last_at: last_at.get(&chat).copied().unwrap_or_default(),
            }
        })
        .collect();
    quiet.sort_by(|a, b| b.before.cmp(&a.before).then(a.title.cmp(&b.title)));
    quiet.truncate(8);
    quiet
}

const COMMON: &[&str] = &[
    "the", "and", "for", "you", "are", "but", "not", "with", "this", "that", "have", "was", "what",
    "when", "your", "can", "all", "just", "its", "it's", "i'm", "i'll", "don't", "out", "get",
    "got", "how", "will", "from", "they", "she", "him", "her", "his", "our", "who", "one", "too",
    "now", "then", "there", "here", "about", "would", "could", "should", "been", "were", "had",
    "has", "did", "does", "yes", "yeah", "okay", "also", "some", "any", "more", "very", "like",
    "know", "think", "see", "come", "going", "want", "need", "well", "really", "much", "only",
    "into", "over", "them", "than", "their", "these", "those", "which", "where", "why", "let",
    "make", "said", "say", "way", "back", "still", "even", "because", "after", "before", "today",
    "tomorrow", "omitted", "media", "message", "deleted", "edited", "http", "https", "www", "com",
];

fn is_emoji(c: char) -> bool {
    matches!(c as u32,
        0x1F300..=0x1F5FF
        | 0x1F600..=0x1F64F
        | 0x1F680..=0x1F6FF
        | 0x1F900..=0x1F9FF
        | 0x1FA70..=0x1FAFF
        | 0x2600..=0x26FF
        | 0x2700..=0x27BF
        | 0x1F1E6..=0x1F1FF)
}

/// Most used words (three letters or more, common words left out), most
/// used emoji and average lengths, from text records only.
pub fn words<'a>(records: impl Iterator<Item = &'a Record>) -> Words {
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut emoji: HashMap<char, u32> = HashMap::new();
    let (mut mine, mut mine_chars, mut theirs, mut their_chars) = (0u32, 0usize, 0u32, 0usize);
    for record in records {
        let Some(text) = record
            .text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        else {
            continue;
        };
        let length = text.chars().count();
        if record.from_me {
            mine += 1;
            mine_chars += length;
        } else {
            theirs += 1;
            their_chars += length;
        }
        for c in text.chars().filter(|c| is_emoji(*c)) {
            *emoji.entry(c).or_default() += 1;
        }
        if text.contains("://") && !text.contains(' ') {
            continue;
        }
        for word in text
            .split(|c: char| !(c.is_alphanumeric() || c == '\''))
            .map(|word| word.trim_matches('\'').to_lowercase())
        {
            if word.chars().count() >= 3
                && word.chars().any(char::is_alphabetic)
                && !COMMON.contains(&word.as_str())
            {
                *counts.entry(word).or_default() += 1;
            }
        }
    }
    let mut top_words: Vec<(String, u32)> = counts.into_iter().collect();
    top_words.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top_words.truncate(30);
    let mut top_emoji: Vec<(String, u32)> = emoji
        .into_iter()
        .map(|(c, count)| (c.to_string(), count))
        .collect();
    top_emoji.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    top_emoji.truncate(12);
    Words {
        top_words,
        top_emoji,
        my_average: mine_chars as f32 / mine.max(1) as f32,
        their_average: their_chars as f32 / theirs.max(1) as f32,
        texts: mine + theirs,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn record(chat: u32, from_me: bool, day: i32, hour: u8, minute: i64) -> Record {
        Record {
            chat,
            from_me,
            at: i64::from(day) * 86_400 + i64::from(hour) * 3600 + minute * 60,
            day,
            hour,
            weekday: ((day + 3).rem_euclid(7)) as u8,
            kind: Kind::Text,
            text: None,
        }
    }

    pub(crate) fn source(records: Vec<Record>) -> Source {
        Source {
            name: "test".into(),
            chats: vec![
                ChatMeta {
                    id: "a".into(),
                    title: "Ada".into(),
                    group: false,
                },
                ChatMeta {
                    id: "g".into(),
                    title: "Family".into(),
                    group: true,
                },
                ChatMeta {
                    id: "b".into(),
                    title: "Bob".into(),
                    group: false,
                },
            ],
            records,
            with_text: false,
        }
    }

    const TODAY: i32 = 20_000;

    #[test]
    fn totals_split_sent_and_received() {
        let mut photo = record(0, false, TODAY, 10, 5);
        photo.kind = Kind::Photo;
        let stats = compute(
            &source(vec![
                record(0, false, TODAY, 10, 0),
                record(0, true, TODAY, 10, 2),
                photo,
                record(1, false, TODAY - 1, 9, 0),
            ]),
            Period::Week,
            TODAY,
        );
        assert_eq!((stats.total, stats.sent, stats.received), (4, 1, 3));
        assert_eq!(stats.active_chats, 2);
        assert_eq!((stats.private_messages, stats.group_messages), (3, 1));
        assert_eq!(stats.kinds, [(Kind::Text, 3), (Kind::Photo, 1)]);
        assert_eq!(stats.top[0].title, "Ada");
        assert!((stats.top[0].share - 0.75).abs() < 1e-6);
        assert_eq!(stats.series.len(), 7);
        assert_eq!(stats.series.last().unwrap().received, 2);
        assert!(stats.words.is_none(), "no text without content analysis");
    }

    #[test]
    fn heatmap_and_busiest_slots() {
        let stats = compute(
            &source(vec![
                record(0, false, TODAY, 21, 0),
                record(0, true, TODAY, 21, 1),
                record(2, false, TODAY - 2, 21, 0),
                record(2, false, TODAY - 2, 8, 0),
                record(2, false, TODAY, 9, 0),
            ]),
            Period::Month,
            TODAY,
        );
        assert_eq!(stats.busiest_hour, Some((21, 3)));
        let weekday = ((TODAY + 3).rem_euclid(7)) as usize;
        assert_eq!(stats.heatmap[weekday][21], 2);
        assert_eq!(stats.busiest_day, Some((TODAY, 3)));
        assert_eq!(
            stats.busiest_weekday.map(|(day, _)| day as usize),
            Some(weekday)
        );
    }

    #[test]
    fn streaks_count_consecutive_days() {
        let days: HashSet<i32> = [1, 2, 3, 7, 8, 10, 11].into_iter().collect();
        assert_eq!(streaks(&days, 11), (3, 2));
        assert_eq!(streaks(&days, 12), (3, 2), "yesterday still counts");
        assert_eq!(streaks(&days, 13), (3, 0));
    }

    #[test]
    fn reply_times_use_the_median() {
        // Ada writes; the person answers after 2, 10 and 4 minutes; Ada
        // answers after 1 minute. A gap over 12 hours is not a reply.
        let stats = compute(
            &source(vec![
                record(0, false, TODAY - 3, 9, 0),
                record(0, true, TODAY - 3, 9, 2),
                record(0, false, TODAY - 3, 9, 3),
                record(0, true, TODAY - 3, 9, 13),
                record(0, false, TODAY - 2, 9, 0),
                record(0, true, TODAY - 2, 9, 4),
                record(0, false, TODAY - 1, 9, 0),
            ]),
            Period::Week,
            TODAY,
        );
        assert_eq!(stats.my_reply, Some(4 * 60));
        assert_eq!(stats.their_reply, Some(60));
        assert_eq!(stats.top[0].my_reply, Some(4 * 60));
        // Three conversations, each started by Ada.
        assert_eq!((stats.started_by_me, stats.started_by_them), (0, 3));
    }

    #[test]
    fn quiet_contacts_were_frequent_before() {
        let mut records: Vec<Record> = (0..25)
            .map(|i| record(0, i % 2 == 0, TODAY - 60 - i, 12, 0))
            .collect();
        // Bob was as frequent but wrote last week.
        records.extend((0..25).map(|i| record(2, false, TODAY - 60 - i, 12, 0)));
        records.push(record(2, false, TODAY - 5, 12, 0));
        // A busy group is never "quiet".
        records.extend((0..25).map(|i| record(1, false, TODAY - 60 - i, 12, 0)));
        records.sort_by_key(|record| record.at);
        let quiet = quiet(&source(records), TODAY);
        assert_eq!(quiet.len(), 1);
        assert_eq!(quiet[0].title, "Ada");
        assert_eq!(quiet[0].before, 25);
    }

    #[test]
    fn the_period_bounds_every_metric() {
        let records = vec![
            record(0, true, TODAY - 40, 12, 0),
            record(0, true, TODAY - 10, 12, 0),
            record(2, false, TODAY, 12, 0),
        ];
        let week = compute(&source(records.clone()), Period::Week, TODAY);
        assert_eq!(week.total, 1);
        assert_eq!(week.active_chats, 1);
        assert_eq!(week.previous_total, Some(1));
        let month = compute(&source(records.clone()), Period::Month, TODAY);
        assert_eq!(month.total, 2);
        let all = compute(&source(records), Period::All, TODAY);
        assert_eq!(all.total, 3);
        assert_eq!(all.days, 41);
        assert_eq!(all.previous_total, None);
    }

    #[test]
    fn words_skip_common_words_and_count_emoji() {
        let mut a = record(0, true, TODAY, 9, 0);
        a.text = Some("Pizza tonight? 🍕🍕 the pizza place".into());
        let mut b = record(0, false, TODAY, 9, 1);
        b.text = Some("yes pizza 😂 ok".into());
        let words = words([a, b].iter());
        assert_eq!(words.top_words[0], ("pizza".to_owned(), 3));
        assert!(
            !words
                .top_words
                .iter()
                .any(|(word, _)| word == "the" || word == "ok")
        );
        assert_eq!(words.top_emoji[0], ("🍕".to_owned(), 2));
        assert_eq!(words.texts, 2);
        assert!(words.my_average > words.their_average);
    }
}
