//! Keep (REQ-EXTRAS-006/007/008): everything stays in local files.
//!
//! - A message deleted for everyone keeps its last content in the archive's
//!   `extras_kept` table, written by the worker's own connection just before
//!   the "deleted" stub replaces it (a hook in `Archive::set_content`).
//! - An edit keeps the version it replaces (a hook in
//!   `Archive::set_edited_content`).
//! - Attachments download as they arrive, and a backfill fetches the ones
//!   already archived but not on disk.
//!
//! Messages the person deletes here themselves are their choice and are not
//! kept. Nothing about keeping reaches WhatsApp.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};

use crate::app::App;
use crate::model::{Action, ChatId, Content, Message};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS extras_kept (
    chat TEXT NOT NULL,
    id TEXT NOT NULL,
    kind TEXT NOT NULL,
    version INTEGER NOT NULL,
    sender TEXT NOT NULL,
    sender_name TEXT,
    from_me INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    content TEXT NOT NULL,
    raw BLOB,
    kept_at INTEGER NOT NULL,
    PRIMARY KEY (chat, id, kind, version)
);";

/// Why a copy was kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Deleted for everyone; the copy is the last content.
    Deleted,
    /// Replaced by an edit; the copy is the version before it.
    Edited,
}

impl Why {
    fn key(self) -> &'static str {
        match self {
            Self::Deleted => "deleted",
            Self::Edited => "edited",
        }
    }
}

/// One kept copy, for the Deleted & edited tab.
#[derive(Clone, Debug)]
pub struct Kept {
    pub chat: ChatId,
    pub id: String,
    pub why: Why,
    pub version: i64,
    pub sender: String,
    pub sender_name: Option<String>,
    pub from_me: bool,
    pub timestamp: i64,
    pub content: Content,
    pub kept_at: i64,
}

fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// Called by the archive before it writes `next` over a message. `edit` says
/// the write is an edit (`set_edited_content`); otherwise only a revoke is
/// kept, since `set_content` also records downloads and poll updates.
/// Never fails the archive's own write: problems are logged.
pub fn preserve(connection: &Connection, chat: &str, id: &str, next: &Content, edit: bool) {
    let why = match (next, edit) {
        (Content::Revoked, _) => Why::Deleted,
        (_, true) => Why::Edited,
        _ => return,
    };
    if let Err(error) = preserve_inner(connection, chat, id, next, why) {
        log::warn!(target: "zapfast::extras", "could not keep a copy: {error}");
    }
}

fn preserve_inner(
    connection: &Connection,
    chat: &str,
    id: &str,
    next: &Content,
    why: Why,
) -> rusqlite::Result<()> {
    let row = connection
        .query_row(
            "SELECT sender, sender_name, from_me, timestamp, content, raw
             FROM messages WHERE chat = ?1 AND id = ?2",
            params![chat, id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((sender, sender_name, from_me, timestamp, content, raw)) = row else {
        return Ok(());
    };
    // A replayed revoke finds the stub already there: nothing left to keep.
    if matches!(
        serde_json::from_str::<Content>(&content),
        Ok(Content::Revoked)
    ) {
        return Ok(());
    }
    if why == Why::Edited && serde_json::to_string(next).ok().as_deref() == Some(content.as_str()) {
        return Ok(());
    }
    connection.execute_batch(SCHEMA)?;
    let version: i64 = match why {
        Why::Deleted => 0,
        Why::Edited => connection.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM extras_kept
             WHERE chat = ?1 AND id = ?2 AND kind = 'edited'",
            params![chat, id],
            |row| row.get(0),
        )?,
    };
    connection.execute(
        "INSERT OR IGNORE INTO extras_kept
         (chat, id, kind, version, sender, sender_name, from_me, timestamp, content, raw, kept_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            chat,
            id,
            why.key(),
            version,
            sender,
            sender_name,
            from_me,
            timestamp,
            content,
            raw,
            now()
        ],
    )?;
    Ok(())
}

/// Every kept copy, newest first. An archive that never kept one has no
/// table yet, which reads as none.
pub fn kept(connection: &Connection) -> rusqlite::Result<Vec<Kept>> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'extras_kept')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(Vec::new());
    }
    let mut statement = connection.prepare(
        "SELECT chat, id, kind, version, sender, sender_name, from_me, timestamp, content, kept_at
         FROM extras_kept ORDER BY kept_at DESC, timestamp DESC, version DESC",
    )?;
    let rows = statement.query_map([], |row| {
        let kind: String = row.get(2)?;
        let content: String = row.get(8)?;
        Ok(Kept {
            chat: row.get(0)?,
            id: row.get(1)?,
            why: if kind == "edited" {
                Why::Edited
            } else {
                Why::Deleted
            },
            version: row.get(3)?,
            sender: row.get(4)?,
            sender_name: row.get(5)?,
            from_me: row.get(6)?,
            timestamp: row.get(7)?,
            content: serde_json::from_str(&content).unwrap_or(Content::Unsupported {
                what: "message".into(),
            }),
            kept_at: row.get(9)?,
        })
    })?;
    rows.collect()
}

/// Copies the person asked to forget, per archive file, until that
/// account's worker removes them on its next tick. The window reads the
/// archive read-only; only the worker writes it.
type Forgets = HashMap<PathBuf, Vec<(ChatId, String, Why, i64)>>;
static FORGET: Mutex<Option<Forgets>> = Mutex::new(None);

pub fn request_forget(archive: &Path, kept: &Kept) {
    if let Ok(mut queue) = FORGET.lock() {
        queue
            .get_or_insert_with(HashMap::new)
            .entry(archive.to_owned())
            .or_default()
            .push((kept.chat.clone(), kept.id.clone(), kept.why, kept.version));
    }
}

/// Called from the worker's tick with its own connection.
pub fn drain_forgets(connection: &Connection, archive: &Path) {
    let pending = FORGET
        .lock()
        .ok()
        .and_then(|mut queue| queue.as_mut().and_then(|queue| queue.remove(archive)))
        .unwrap_or_default();
    for (chat, id, why, version) in pending {
        if let Err(error) = forget(connection, &chat, &id, why, version) {
            log::warn!(target: "zapfast::extras", "could not forget a kept copy: {error}");
        }
    }
}

fn forget(
    connection: &Connection,
    chat: &str,
    id: &str,
    why: Why,
    version: i64,
) -> rusqlite::Result<()> {
    connection.execute_batch(SCHEMA)?;
    connection.execute(
        "DELETE FROM extras_kept WHERE chat = ?1 AND id = ?2 AND kind = ?3 AND version = ?4",
        params![chat, id, why.key(), version],
    )?;
    Ok(())
}

/// Whether `content` has an attachment of its own that is not on disk yet.
fn needs_download(content: &Content) -> bool {
    match content {
        Content::Image { media, .. }
        | Content::Video { media, .. }
        | Content::Audio { media, .. }
        | Content::Document { media, .. }
        | Content::Sticker { media, .. } => media.path.is_none(),
        _ => false,
    }
}

fn download(chat: &str, message: &str) -> Action {
    Action::Download {
        manual: true,
        card: None,
        chat: chat.to_owned(),
        message: message.to_owned(),
    }
}

/// An incoming message: with saving on, its attachment downloads at once,
/// whatever its size, instead of when it scrolls into view.
pub fn on_incoming(app: &mut App, chat: &str, message: &Message) {
    if app.extras.config.save_all_media && needs_download(&message.content) {
        app.actions.push(download(chat, &message.id));
    }
}

/// Attachments in the archive that are not on disk, oldest last: the
/// backfill fetches the newest first.
pub fn missing(connection: &Connection) -> rusqlite::Result<Vec<(ChatId, String)>> {
    let mut statement = connection.prepare(
        "SELECT chat, id FROM messages
         WHERE json_valid(content)
           AND json_extract(content, '$.kind') IN ('image', 'video', 'audio', 'document', 'sticker')
           AND json_extract(content, '$.media.path') IS NULL
         ORDER BY timestamp DESC",
    )?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

/// "Save everything now": the archive's missing attachments, asked for a
/// few at a time so the link and the phone are not flooded.
#[derive(Default)]
pub struct Backfill {
    queue: VecDeque<(ChatId, String)>,
    pub total: usize,
    pub asked: usize,
    last: Option<Instant>,
}

impl Backfill {
    /// Downloads asked for per second at most.
    pub const PER_SECOND: usize = 3;

    pub fn start(&mut self, missing: Vec<(ChatId, String)>) {
        self.total = missing.len();
        self.asked = 0;
        self.queue = missing.into();
        self.last = None;
    }

    pub fn stop(&mut self) {
        self.queue.clear();
    }

    pub fn running(&self) -> bool {
        !self.queue.is_empty()
    }

    /// The downloads due at `now`.
    pub fn due(&mut self, now: Instant) -> Vec<Action> {
        if self.queue.is_empty()
            || self
                .last
                .is_some_and(|last| now.duration_since(last) < Duration::from_secs(1))
        {
            return Vec::new();
        }
        self.last = Some(now);
        let take = Self::PER_SECOND.min(self.queue.len());
        self.asked += take;
        self.queue
            .drain(..take)
            .map(|(chat, id)| download(&chat, &id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE messages (chat TEXT, id TEXT, sender TEXT, sender_name TEXT,
                 from_me INTEGER, timestamp INTEGER, content TEXT, raw BLOB,
                 PRIMARY KEY (chat, id));",
            )
            .unwrap();
        connection
    }

    fn put(connection: &Connection, id: &str, content: &Content) {
        connection
            .execute(
                "INSERT OR REPLACE INTO messages VALUES ('c', ?1, 's', 'Sam', 0, 100, ?2, x'01')",
                params![id, serde_json::to_string(content).unwrap()],
            )
            .unwrap();
    }

    fn text(text: &str) -> Content {
        Content::Text {
            text: text.into(),
            preview: None,
        }
    }

    #[test]
    fn a_revoked_message_is_kept() {
        let db = archive();
        put(&db, "m", &text("call me at 5"));
        preserve(&db, "c", "m", &Content::Revoked, false);
        put(&db, "m", &Content::Revoked);
        let kept = kept(&db).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].why, Why::Deleted);
        assert_eq!(kept[0].content, text("call me at 5"));
        assert_eq!(kept[0].sender_name.as_deref(), Some("Sam"));
    }

    #[test]
    fn a_message_is_kept_once() {
        let db = archive();
        put(&db, "m", &text("hello"));
        preserve(&db, "c", "m", &Content::Revoked, false);
        preserve(&db, "c", "m", &Content::Revoked, false);
        put(&db, "m", &Content::Revoked);
        // A replay finds the stub: the first copy stays.
        preserve(&db, "c", "m", &Content::Revoked, false);
        let kept = kept(&db).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].content, text("hello"));
        // A download or poll update through set_content keeps nothing.
        put(&db, "n", &text("x"));
        preserve(&db, "c", "n", &text("x"), false);
        assert_eq!(super::kept(&db).unwrap().len(), 1);
    }

    #[test]
    fn each_edit_keeps_the_version_before_it() {
        let db = archive();
        put(&db, "m", &text("v1"));
        preserve(&db, "c", "m", &text("v2"), true);
        put(&db, "m", &text("v2"));
        preserve(&db, "c", "m", &text("v3"), true);
        put(&db, "m", &text("v3"));
        // An edit that changes nothing keeps nothing.
        preserve(&db, "c", "m", &text("v3"), true);
        let mut kept = kept(&db).unwrap();
        kept.sort_by_key(|kept| kept.version);
        let versions: Vec<_> = kept.iter().map(|kept| kept.content.clone()).collect();
        assert_eq!(versions, [text("v1"), text("v2")]);
        assert!(kept.iter().all(|kept| kept.why == Why::Edited));
    }

    /// Through the real archive: its hooks keep the copies.
    #[test]
    fn the_archive_keeps_revokes_and_edits_through_its_hooks() {
        let archive = crate::archive::Archive::in_memory().unwrap();
        let chat = "1@s.whatsapp.net";
        archive.ensure_chat(chat, "Ada").unwrap();
        let mut message = crate::archive::tests::message(chat, "m", 100, false);
        message.content = text("see you at 7");
        archive.insert_message(&message, Some(b"raw")).unwrap();
        archive
            .set_edited_content(chat, "m", &text("see you at 8"), &[])
            .unwrap();
        archive
            .set_content(chat, "m", &Content::Revoked, false)
            .unwrap();
        let mut kept = kept(archive.extras_connection()).unwrap();
        kept.sort_by_key(|kept| kept.why == Why::Deleted);
        assert_eq!(kept.len(), 2);
        assert_eq!(
            (kept[0].why, &kept[0].content),
            (Why::Edited, &text("see you at 7"))
        );
        assert_eq!(
            (kept[1].why, &kept[1].content),
            (Why::Deleted, &text("see you at 8"))
        );
        // The archive itself shows the stub, as before.
        assert_eq!(
            archive.message(chat, "m").unwrap().unwrap().content,
            Content::Revoked
        );
    }

    #[test]
    fn forgetting_removes_the_copy() {
        let db = archive();
        put(&db, "m", &text("secret"));
        preserve(&db, "c", "m", &Content::Revoked, false);
        let copy = kept(&db).unwrap().remove(0);
        let path = Path::new("/archive-under-test.db");
        request_forget(path, &copy);
        drain_forgets(&db, Path::new("/another-account.db"));
        assert_eq!(kept(&db).unwrap().len(), 1, "another account's tick");
        drain_forgets(&db, path);
        assert!(kept(&db).unwrap().is_empty());
    }

    #[test]
    fn incoming_attachments_download_when_saving_is_on() {
        let media = crate::model::Media {
            mime: "image/jpeg".into(),
            size: 900_000_000,
            width: None,
            height: None,
            album: None,
            path: None,
            state: Default::default(),
        };
        let photo = Content::Image {
            caption: None,
            media,
            motion: None,
        };
        assert!(needs_download(&photo));
        assert!(!needs_download(&text("hi")));
        assert!(matches!(
            download("c", "m"),
            Action::Download {
                manual: true,
                card: None,
                ..
            }
        ));
    }

    #[test]
    fn backfill_paces_downloads() {
        let mut backfill = Backfill::default();
        backfill.start((0..7).map(|i| ("c".to_owned(), i.to_string())).collect());
        let start = Instant::now();
        assert_eq!(backfill.due(start).len(), 3);
        assert!(backfill.due(start + Duration::from_millis(500)).is_empty());
        assert_eq!(backfill.due(start + Duration::from_secs(1)).len(), 3);
        backfill.stop();
        assert!(!backfill.running());
        assert!(backfill.due(start + Duration::from_secs(5)).is_empty());
        assert_eq!(backfill.asked, 6);
    }
}
