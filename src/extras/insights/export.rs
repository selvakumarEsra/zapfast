//! Chat exports the person chooses (REQ-EXTRAS-005): WhatsApp's "Export
//! chat" `.txt`, or the `.zip` it makes, parsed in memory only.
//!
//! Android: `12/31/24, 21:05 - Name: text` (or `9:05 pm`)
//! iPhone:  `[31/12/2024, 21:05:33] Name: text`

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use super::stats::{ChatMeta, Kind, Record, Source};

/// One parsed line, before the person says which name is theirs.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Date as written: the first two numbers, whichever order they are.
    pub a: u32,
    pub b: u32,
    pub year: i16,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub sender: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Export {
    pub title: String,
    pub lines: Vec<Line>,
}

impl Export {
    /// Senders, most messages first: the person picks theirs from these.
    pub fn senders(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for line in &self.lines {
            *counts.entry(line.sender.as_str()).or_default() += 1;
        }
        let mut senders: Vec<(String, usize)> = counts
            .into_iter()
            .map(|(name, count)| (name.to_owned(), count))
            .collect();
        senders.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        senders
    }

    /// Whether dates are written day first: a first number over 12 says so,
    /// a second one over 12 says month first, and day first otherwise.
    fn day_first(&self) -> bool {
        if self.lines.iter().any(|line| line.a > 12) {
            return true;
        }
        !self.lines.iter().any(|line| line.b > 12)
    }

    /// The records Insights reads, with `me` the person's own name.
    pub fn to_source(&self, me: &str, with_text: bool) -> Source {
        let day_first = self.day_first();
        let group = self.senders().len() > 2;
        let mut records = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            let (day, month) = if day_first {
                (line.a, line.b)
            } else {
                (line.b, line.a)
            };
            let Ok(date) = jiff::civil::Date::new(line.year, month as i8, day as i8) else {
                continue;
            };
            let Ok(time) =
                jiff::civil::Time::new(line.hour as i8, line.minute as i8, line.second as i8, 0)
            else {
                continue;
            };
            let civil = date.to_datetime(time);
            let epoch = jiff::civil::date(1970, 1, 1);
            let day_index = (date - epoch).get_days();
            let at = civil
                .to_zoned(jiff::tz::TimeZone::system())
                .map(|zoned| zoned.timestamp().as_second())
                .unwrap_or_else(|_| i64::from(day_index) * 86_400);
            let (kind, text) = classify(&line.text);
            let Some(kind) = kind else { continue };
            records.push(Record {
                chat: 0,
                from_me: line.sender == me,
                at,
                day: day_index,
                hour: line.hour,
                weekday: date.weekday().to_monday_zero_offset() as u8,
                kind,
                text: if with_text && kind == Kind::Text {
                    Some(text)
                } else {
                    None
                },
            });
        }
        records.sort_by_key(|record| record.at);
        Source {
            name: format!("Imported: {}", self.title),
            chats: vec![ChatMeta {
                id: format!("export:{}", self.title),
                title: self.title.clone(),
                group,
            }],
            records,
            with_text,
        }
    }
}

/// The kind a body stands for, and its text without edit markers. `None`
/// for deleted messages.
fn classify(body: &str) -> (Option<Kind>, String) {
    let body = body.trim_start_matches('\u{200e}').trim();
    let lower = body.to_lowercase();
    if lower == "this message was deleted" || lower == "you deleted this message" {
        return (None, String::new());
    }
    let media = [
        ("image omitted", Kind::Photo),
        ("video omitted", Kind::Video),
        ("gif omitted", Kind::Video),
        ("audio omitted", Kind::Voice),
        ("sticker omitted", Kind::Sticker),
        ("document omitted", Kind::Document),
        ("contact card omitted", Kind::Other),
        ("<media omitted>", Kind::Photo),
    ];
    for (marker, kind) in media {
        if lower == marker {
            return (Some(kind), String::new());
        }
    }
    if lower.starts_with("<attached:") || lower.ends_with("(file attached)") {
        let kind = if lower.contains("photo") || lower.contains("img-") || lower.ends_with(".jpg>")
        {
            Kind::Photo
        } else if lower.contains("video") || lower.contains("vid-") || lower.contains(".mp4") {
            Kind::Video
        } else if lower.contains("audio") || lower.contains("ptt-") || lower.contains(".opus") {
            Kind::Voice
        } else if lower.contains("sticker") || lower.contains(".webp") {
            Kind::Sticker
        } else {
            Kind::Document
        };
        return (Some(kind), String::new());
    }
    if lower.starts_with("location: ") || lower.starts_with("poll:") {
        return (Some(Kind::Other), String::new());
    }
    let text = body
        .trim_end_matches("<This message was edited>")
        .trim_end_matches("\u{200e}")
        .trim()
        .to_owned();
    (Some(Kind::Text), text)
}

fn number(text: &str) -> Option<u32> {
    (!text.is_empty() && text.len() <= 4 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// `31/12/2024` or `12.31.24` or `2024-12-31`: (a, b, year).
fn date(text: &str) -> Option<(u32, u32, i16)> {
    let parts: Vec<&str> = text.split(['/', '.', '-']).collect();
    let [first, second, third] = parts.as_slice() else {
        return None;
    };
    let (a, b, year) = if first.len() == 4 {
        (number(third)?, number(second)?, number(first)?)
    } else {
        (number(first)?, number(second)?, number(third)?)
    };
    let year = if year < 100 { 2000 + year } else { year };
    ((1..=31).contains(&a) && (1..=31).contains(&b) && (1990..=2100).contains(&year)).then_some((
        a,
        b,
        year as i16,
    ))
}

/// `21:05`, `21:05:33`, `9:05 pm`, `9:05\u{202f}PM`.
fn time(text: &str) -> Option<(u8, u8, u8)> {
    let text = text.replace(['\u{202f}', '\u{a0}'], " ");
    let lower = text.trim().to_lowercase();
    let (clock, meridiem) = if let Some(clock) = lower.strip_suffix("pm") {
        (clock.trim().trim_end_matches('.'), Some(true))
    } else if let Some(clock) = lower.strip_suffix("am") {
        (clock.trim().trim_end_matches('.'), Some(false))
    } else if let Some(clock) = lower.strip_suffix("p.m.") {
        (clock.trim(), Some(true))
    } else if let Some(clock) = lower.strip_suffix("a.m.") {
        (clock.trim(), Some(false))
    } else {
        (lower.as_str(), None)
    };
    let parts: Vec<&str> = clock.split(':').collect();
    let (hour, minute, second) = match parts.as_slice() {
        [h, m] => (number(h)?, number(m)?, 0),
        [h, m, s] => (number(h)?, number(m)?, number(s)?),
        _ => return None,
    };
    let hour = match meridiem {
        Some(pm) if (1..=12).contains(&hour) => hour % 12 + if pm { 12 } else { 0 },
        Some(_) => return None,
        None => hour,
    };
    (hour < 24 && minute < 60 && second < 60).then_some((hour as u8, minute as u8, second as u8))
}

/// A line that starts a message: its stamp, sender and first text line.
/// System lines (no `Name: `) are `None` and skipped.
fn start(line: &str) -> Option<Option<Line>> {
    let line = line.trim_start_matches(['\u{200e}', '\u{feff}']);
    let (stamp, rest) = if let Some(inner) = line.strip_prefix('[') {
        let (stamp, rest) = inner.split_once(']')?;
        (stamp, rest.trim_start())
    } else {
        let (stamp, rest) = line.split_once(" - ")?;
        (stamp, rest)
    };
    let (day, clock) = stamp.split_once(", ").or_else(|| stamp.split_once(' '))?;
    let (a, b, year) = date(day.trim())?;
    let (hour, minute, second) = time(clock)?;
    let Some((sender, text)) = rest.split_once(": ") else {
        return Some(None);
    };
    let sender = sender.trim_start_matches('\u{200e}').trim();
    if sender.is_empty() || sender.len() > 80 {
        return Some(None);
    }
    Some(Some(Line {
        a,
        b,
        year,
        hour,
        minute,
        second,
        sender: sender.to_owned(),
        text: text.to_owned(),
    }))
}

/// Parses an export's text. Lines that start no message continue the one
/// before. A file with no message lines is not an export.
pub fn parse(title: &str, text: &str) -> Result<Export, String> {
    let mut lines: Vec<Line> = Vec::new();
    let mut in_message = false;
    for raw in text.lines() {
        match start(raw) {
            Some(Some(line)) => {
                lines.push(line);
                in_message = true;
            }
            Some(None) => in_message = false,
            None if in_message => {
                if let Some(last) = lines.last_mut() {
                    last.text.push('\n');
                    last.text.push_str(raw);
                }
            }
            None => {}
        }
    }
    if lines.is_empty() {
        return Err("This file is not a WhatsApp chat export.".to_owned());
    }
    Ok(Export {
        title: title.to_owned(),
        lines,
    })
}

/// Reads a `.txt` export, or the chat text inside WhatsApp's `.zip`.
pub fn read(path: &Path) -> Result<Export, String> {
    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Chat")
        .trim_start_matches("WhatsApp Chat with ")
        .trim_start_matches("WhatsApp Chat - ")
        .to_owned();
    let is_zip = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
    let text = if is_zip {
        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|_| "This is not a chat export.")?;
        let name = archive
            .file_names()
            .filter_map(|name| name.ok().map(|name| name.into_owned()))
            .find(|name| name.to_lowercase().ends_with(".txt"))
            .ok_or("This zip holds no chat text.")?;
        let mut text = String::new();
        archive
            .by_name(&name)
            .map_err(|error| error.to_string())?
            .take(256 * 1024 * 1024)
            .read_to_string(&mut text)
            .map_err(|_| "The chat text is not readable.")?;
        text
    } else {
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        String::from_utf8(bytes).map_err(|_| "This file is not a WhatsApp chat export.")?
    };
    parse(&title, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_and_iphone_layouts_parse() {
        let android = "12/31/24, 21:05 - Messages and calls are end-to-end encrypted.\n\
                       12/31/24, 21:05 - Ada: Happy new year!\n\
                       12/31/24, 9:07\u{202f}PM - Me: You too\n";
        let export = parse("Ada", android).unwrap();
        assert_eq!(export.lines.len(), 2);
        assert_eq!(export.lines[1].hour, 21);
        assert_eq!(export.lines[1].minute, 7);
        let source = export.to_source("Me", false);
        assert_eq!(source.records.len(), 2);
        let date = jiff::civil::date(2024, 12, 31);
        let day = (date - jiff::civil::date(1970, 1, 1)).get_days();
        assert_eq!(
            source.records[0].day, day,
            "month first: 31 cannot be a month"
        );
        assert!(source.records[1].from_me && !source.records[0].from_me);

        let iphone = "[31/12/2024, 21:05:33] Ada: Happy new year!\n\
                      \u{200e}[01/01/2025, 08:00:01] Me: \u{200e}image omitted\n";
        let export = parse("Ada", iphone).unwrap();
        let source = export.to_source("Me", true);
        assert_eq!(source.records.len(), 2);
        assert_eq!(source.records[0].text.as_deref(), Some("Happy new year!"));
        assert_eq!(source.records[1].kind, Kind::Photo);
        assert_eq!(source.records[1].hour, 8);
        assert!(!source.chats[0].group);
    }

    #[test]
    fn continuation_lines_join_the_message() {
        let text = "[01/02/2025, 10:00:00] Ada: first line\nsecond line\n\
                    [01/02/2025, 10:01:00] Bob: hi <This message was edited>\n";
        let export = parse("Friends", text).unwrap();
        assert_eq!(export.lines[0].text, "first line\nsecond line");
        let source = export.to_source("Bob", true);
        assert_eq!(source.records[1].text.as_deref(), Some("hi"));
    }

    #[test]
    fn media_placeholders_become_kinds() {
        let text = "1/2/25, 10:00 - Ada: <Media omitted>\n\
                    1/2/25, 10:01 - Ada: PTT-20250102-WA0001.opus (file attached)\n\
                    1/2/25, 10:02 - Ada: This message was deleted\n\
                    1/2/25, 10:03 - Ada: sticker omitted\n";
        let source = parse("Ada", text).unwrap().to_source("Me", true);
        let kinds: Vec<Kind> = source.records.iter().map(|record| record.kind).collect();
        assert_eq!(kinds, [Kind::Photo, Kind::Voice, Kind::Sticker]);
        assert!(source.records.iter().all(|record| record.text.is_none()));
    }

    #[test]
    fn a_file_that_is_not_an_export_is_refused() {
        assert!(parse("notes", "shopping list\nmilk\neggs").is_err());
        assert!(parse("empty", "").is_err());
    }

    #[test]
    fn many_senders_make_a_group() {
        let text = "1/2/25, 10:00 - Ada: a\n1/2/25, 10:01 - Bob: b\n1/2/25, 10:02 - Cy: c\n";
        let export = parse("Club", text).unwrap();
        assert_eq!(export.senders().len(), 3);
        assert!(export.to_source("Ada", false).chats[0].group);
    }
}
