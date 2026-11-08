//! Reads this account's archive for Insights, off the interface thread,
//! through a separate read-only connection (REQ-EXTRAS-003).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use rusqlite::Connection;

use super::stats::{ChatMeta, Kind, Record, Source};
use crate::extras::keep::{self, Kept};

/// What one load brings back.
pub struct Loaded {
    pub source: Result<Source, String>,
    pub kept: Vec<Kept>,
    /// Attachments in the archive that are not on disk yet.
    pub missing: Vec<(String, String)>,
}

/// The chats Insights may read, decided on the interface thread where the
/// titles and the locked folder are known: locked chats and channels are
/// left out before anything is read.
pub struct Request {
    pub archive: PathBuf,
    pub name: String,
    pub chats: Vec<ChatMeta>,
    pub with_text: bool,
}

pub fn spawn(request: Request, waker: crate::backend::Waker) -> mpsc::Receiver<Loaded> {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("extras-insights".into())
        .spawn(move || {
            let loaded = load(&request);
            let _ = sender.send(loaded);
            waker.wake();
        })
        .ok();
    receiver
}

fn load(request: &Request) -> Loaded {
    let reader = match open(&request.archive) {
        Ok(reader) => reader,
        Err(error) => {
            return Loaded {
                source: Err(error),
                kept: Vec::new(),
                missing: Vec::new(),
            };
        }
    };
    let connection = reader.extras_connection();
    let source = load_archive(connection, request).map_err(|error| error.to_string());
    let kept = keep::kept(connection).unwrap_or_default();
    let missing = keep::missing(connection).unwrap_or_default();
    Loaded {
        source,
        kept,
        missing,
    }
}

fn open(path: &Path) -> Result<crate::archive::Archive, String> {
    crate::archive::Archive::export_reader(path)
        .map_err(|error| format!("Could not open the local archive: {error}"))
}

/// Local day, hour and weekday of a Unix time.
pub fn local(at: i64, zone: &jiff::tz::TimeZone) -> (i32, u8, u8) {
    let Ok(timestamp) = jiff::Timestamp::from_second(at) else {
        return (0, 0, 0);
    };
    let zoned = timestamp.to_zoned(zone.clone());
    let date = zoned.date();
    let day = (date - jiff::civil::date(1970, 1, 1)).get_days();
    (
        day,
        zoned.hour() as u8,
        date.weekday().to_monday_zero_offset() as u8,
    )
}

/// Today's local day, as days since 1970-01-01.
pub fn today() -> i32 {
    local(
        jiff::Timestamp::now().as_second(),
        &jiff::tz::TimeZone::system(),
    )
    .0
}

pub fn load_archive(connection: &Connection, request: &Request) -> rusqlite::Result<Source> {
    let index: HashMap<&str, u32> = request
        .chats
        .iter()
        .enumerate()
        .map(|(i, chat)| (chat.id.as_str(), i as u32))
        .collect();
    let zone = jiff::tz::TimeZone::system();
    // Metadata only, unless the person turned content analysis on.
    let sql = if request.with_text {
        "SELECT chat, from_me, timestamp, json_extract(content, '$.kind'),
                COALESCE(json_extract(content, '$.text'), json_extract(content, '$.caption'))
         FROM messages WHERE json_valid(content) ORDER BY timestamp"
    } else {
        "SELECT chat, from_me, timestamp, json_extract(content, '$.kind'), NULL
         FROM messages WHERE json_valid(content) ORDER BY timestamp"
    };
    let mut statement = connection.prepare(sql)?;
    let mut rows = statement.query([])?;
    let mut records = Vec::new();
    while let Some(row) = rows.next()? {
        let chat: String = row.get(0)?;
        let Some(&chat) = index.get(chat.as_str()) else {
            continue;
        };
        let kind: Option<String> = row.get(3)?;
        let Some(kind) = kind.as_deref().and_then(Kind::from_archive) else {
            continue;
        };
        let at: i64 = row.get(2)?;
        if at <= 0 {
            continue;
        }
        let (day, hour, weekday) = local(at, &zone);
        let text: Option<String> = if request.with_text && kind == Kind::Text {
            row.get(4)?
        } else {
            None
        };
        records.push(Record {
            chat,
            from_me: row.get(1)?,
            at,
            day,
            hour,
            weekday,
            kind,
            text,
        });
    }
    Ok(Source {
        name: request.name.clone(),
        chats: request.chats.clone(),
        records,
        with_text: request.with_text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_and_unlisted_chats_are_never_read_and_text_needs_consent() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE messages (chat TEXT, id TEXT, from_me INTEGER, timestamp INTEGER, content TEXT);
                 INSERT INTO messages VALUES ('a', '1', 0, 1700000000, '{\"kind\":\"text\",\"text\":\"hi\"}');
                 INSERT INTO messages VALUES ('a', '2', 1, 1700000060, '{\"kind\":\"image\",\"caption\":\"cat\"}');
                 INSERT INTO messages VALUES ('a', '3', 1, 1700000090, '{\"kind\":\"revoked\"}');
                 INSERT INTO messages VALUES ('locked', '4', 0, 1700000100, '{\"kind\":\"text\",\"text\":\"secret\"}');",
            )
            .unwrap();
        let mut request = Request {
            archive: PathBuf::new(),
            name: "This account".into(),
            chats: vec![ChatMeta {
                id: "a".into(),
                title: "Ada".into(),
                group: false,
            }],
            with_text: false,
        };
        let source = load_archive(&connection, &request).unwrap();
        assert_eq!(source.records.len(), 2, "the deletion stub is no message");
        assert!(source.records.iter().all(|record| record.text.is_none()));
        assert_eq!(source.records[1].kind, Kind::Photo);
        request.with_text = true;
        let source = load_archive(&connection, &request).unwrap();
        assert_eq!(source.records[0].text.as_deref(), Some("hi"));
        assert_eq!(source.records[1].text, None, "captions are not words");
    }
}
