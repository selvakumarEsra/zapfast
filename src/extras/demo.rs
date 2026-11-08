//! Synthetic extras states for demo screenshots: `--demo-page favourites`,
//! `insights`, `insights-people`, `insights-rhythm`, `insights-words`,
//! `insights-kept`, `insights-saving`, `insights-history`. Sample content
//! only.

use crate::app::App;
use crate::model::{ChatFilter, Content};

use super::insights::Tab;
use super::insights::stats::{ChatMeta, Kind, Record, Source};
use super::keep::{Kept, Why};

/// Applies an extras demo flag; `false` when `part` is not one.
pub fn apply(app: &mut App, part: &str) -> bool {
    let tab = match part {
        "favourites" => {
            app.chat_filter = ChatFilter::Favorites;
            return true;
        }
        "insights" => Tab::Overview,
        "insights-people" => Tab::People,
        "insights-rhythm" => Tab::Rhythm,
        "insights-words" => Tab::Words,
        "insights-kept" => Tab::Kept,
        "insights-saving" => Tab::Saving,
        "insights-history" => Tab::History,
        _ => return false,
    };
    let words = tab == Tab::Words;
    app.extras.config.content_analysis = words;
    let source = sample(app, words);
    let kept = kept(&source);
    let missing = (0..37)
        .map(|i| ("demo".to_owned(), i.to_string()))
        .collect();
    let history = (tab == Tab::History).then(|| history(&source));
    let insights = &mut app.extras.insights;
    insights.set_demo(source, kept, missing);
    if let Some((full, merge, selected)) = history {
        insights.set_demo_history(full, merge, selected);
    }
    insights.tab = tab;
    insights.open = true;
    true
}

/// Two chats with their full history, an export merged into a third that
/// reaches a year further back, and two chats ticked for fetching.
fn history(source: &Source) -> (Vec<String>, super::insights::merge::Merge, Vec<String>) {
    let id = |i: usize| source.chats[i % source.chats.len()].id.clone();
    let oldest = source.records.first().map_or(0, |record| record.at);
    let zone = jiff::tz::TimeZone::system();
    let mut lcg = Lcg(7);
    let records = (0..420)
        .map(|i| {
            let at = oldest - 400 * 86_400 + i64::from(i) * 80_000 + i64::from(lcg.below(3600));
            let (day, hour, weekday) = super::insights::source::local(at, &zone);
            Record {
                chat: 0,
                from_me: lcg.below(2) == 0,
                at,
                day,
                hour,
                weekday,
                kind: if lcg.below(6) == 0 {
                    Kind::Photo
                } else {
                    Kind::Text
                },
                text: None,
            }
        })
        .collect();
    let merge = super::insights::merge::Merge {
        chat: id(2),
        export: source.chats[2 % source.chats.len()].title.clone(),
        records,
    };
    (vec![id(0), id(4)], merge, vec![id(1), id(3)])
}

/// A small deterministic generator, so screenshots do not change.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
}

const LINES: &[&str] = &[
    "Dinner tonight? 🍝",
    "Running late, traffic is crazy 😅",
    "Did you see the match? ⚽",
    "Pizza on Friday 🍕",
    "Happy birthday!! 🎉🎂",
    "Call me when you're free",
    "The concert was amazing 🎶",
    "Weekend trip to the lake?",
    "Thanks so much ❤️",
    "Coffee tomorrow morning ☕",
    "Sending the photos now 📸",
    "Movie night at mine 🍿",
];

fn sample(app: &App, words: bool) -> Source {
    let mut chats: Vec<ChatMeta> = app
        .account()
        .chats
        .iter()
        .filter(|chat| !chat.locked && chat.kind != crate::model::ChatKind::Broadcast)
        .take(9)
        .map(|chat| ChatMeta {
            id: chat.id.clone(),
            title: app.chat_title(chat),
            group: chat.is_group(),
        })
        .collect();
    if chats.is_empty() {
        chats.push(ChatMeta {
            id: "demo@s.whatsapp.net".into(),
            title: "Ada Lovelace".into(),
            group: false,
        });
    }
    let today = super::insights::source::today();
    let zone = jiff::tz::TimeZone::system();
    let mut random = Lcg(42);
    let mut records = Vec::new();
    // Earlier chats talk more; the last private chat goes quiet a month ago.
    let quiet = chats.iter().rposition(|chat| !chat.group);
    for day in (today - 150)..=today {
        let weekday = (day + 3).rem_euclid(7);
        let weekend = weekday >= 5;
        for index in 0..chats.len() {
            if Some(index) == quiet && day > today - 35 {
                continue;
            }
            let weight = 9 / (index as u32 + 1) + u32::from(weekend) * 2;
            let conversations = random.below(weight / 3 + 2);
            for _ in 0..conversations {
                // Evenings are busiest, then lunch and the morning.
                let hour = match random.below(10) {
                    0..=3 => 19 + random.below(4),
                    4..=5 => 12 + random.below(2),
                    6..=7 => 8 + random.below(3),
                    _ => random.below(24),
                };
                // Local wall-clock time, as a person's evenings are.
                let date = jiff::civil::date(1970, 1, 1) + jiff::Span::new().days(i64::from(day));
                let base = date
                    .at(hour as i8, 0, 0, 0)
                    .to_zoned(zone.clone())
                    .map_or(i64::from(day) * 86_400, |zoned| {
                        zoned.timestamp().as_second()
                    });
                let mut at = base + i64::from(random.below(3000));
                let mut from_me = random.below(2) == 0;
                for _ in 0..(2 + random.below(6)) {
                    let (local_day, local_hour, weekday) =
                        super::insights::source::local(at, &zone);
                    let kind = match random.below(20) {
                        0..=1 => Kind::Photo,
                        2 => Kind::Voice,
                        3 => Kind::Sticker,
                        4 => Kind::Video,
                        5 if random.below(3) == 0 => Kind::Document,
                        _ => Kind::Text,
                    };
                    records.push(Record {
                        chat: index as u32,
                        from_me,
                        at,
                        day: local_day,
                        hour: local_hour,
                        weekday,
                        kind,
                        text: (words && kind == Kind::Text)
                            .then(|| LINES[random.below(LINES.len() as u32) as usize].to_owned()),
                    });
                    at += i64::from(30 + random.below(if from_me { 400 } else { 900 }));
                    from_me = !from_me || random.below(4) == 0;
                }
            }
        }
    }
    let now = jiff::Timestamp::now().as_second();
    records.retain(|record| record.at <= now);
    records.sort_by_key(|record| record.at);
    Source {
        name: "This account".into(),
        chats,
        records,
        with_text: words,
        full: Default::default(),
    }
}

fn kept(source: &Source) -> Vec<Kept> {
    let now = jiff::Timestamp::now().as_second();
    let chat = |i: usize| source.chats[i % source.chats.len()].clone();
    let text = |text: &str| Content::Text {
        text: text.into(),
        preview: None,
    };
    vec![
        Kept {
            chat: chat(0).id,
            id: "k1".into(),
            why: Why::Deleted,
            version: 0,
            sender: chat(0).id,
            sender_name: Some(chat(0).title),
            from_me: false,
            timestamp: now - 3600,
            content: text("Don't tell anyone yet, but we're getting a puppy! 🐶"),
            kept_at: now - 3500,
        },
        Kept {
            chat: chat(1).id,
            id: "k2".into(),
            why: Why::Edited,
            version: 1,
            sender: chat(1).id,
            sender_name: Some(chat(1).title),
            from_me: false,
            timestamp: now - 7200,
            content: text("Meet at 6 at the station"),
            kept_at: now - 7000,
        },
        Kept {
            chat: chat(2).id,
            id: "k3".into(),
            why: Why::Deleted,
            version: 0,
            sender: chat(2).id,
            sender_name: Some(chat(2).title),
            from_me: false,
            timestamp: now - 86_400,
            content: text("Oops, wrong chat 🙈"),
            kept_at: now - 86_000,
        },
    ]
}
