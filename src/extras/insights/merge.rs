//! Chat exports merged into this account's numbers (REQ-EXTRAS-009): an
//! export adds the messages the archive does not have, in memory only.

use std::collections::HashMap;

use super::stats::{ChatMeta, Kind, Record, Source};

/// One export merged into one of this account's chats.
#[derive(Clone, Debug, PartialEq)]
pub struct Merge {
    /// The account's chat it belongs to.
    pub chat: String,
    /// The export's own title, as its file names it.
    pub export: String,
    /// The export's records, as read; their `chat` is set when merged.
    pub records: Vec<Record>,
}

/// The chats an export may belong to, best guess first: a title equal to the
/// export's, then one containing it (or contained in it), then the rest by
/// title. Indices into `chats`; never an export's own chat.
pub fn suggest(title: &str, chats: &[ChatMeta]) -> Vec<usize> {
    let wanted = title.trim().to_lowercase();
    let score = |chat: &ChatMeta| {
        let title = chat.title.trim().to_lowercase();
        if title == wanted {
            0
        } else if !title.is_empty()
            && !wanted.is_empty()
            && (title.contains(&wanted) || wanted.contains(&title))
        {
            1
        } else {
            2
        }
    };
    let mut indices: Vec<usize> = (0..chats.len())
        .filter(|&i| !chats[i].id.starts_with("export:"))
        .collect();
    indices.sort_by(|&a, &b| {
        score(&chats[a]).cmp(&score(&chats[b])).then_with(|| {
            chats[a]
                .title
                .to_lowercase()
                .cmp(&chats[b].title.to_lowercase())
        })
    });
    indices
}

/// A message's identity for matching: an export has minute precision.
fn slot(record: &Record) -> (bool, Kind, i64) {
    (record.from_me, record.kind, record.at.div_euclid(60))
}

/// The export records `chat` lacks: each one matches a record already in
/// `base` with the same direction and kind in the same minute, one to one,
/// and only those left without a match are returned, set to `chat`.
pub fn merge(base: &[Record], chat: u32, export: &[Record]) -> Vec<Record> {
    let mut have: HashMap<(bool, Kind, i64), u32> = HashMap::new();
    for record in base.iter().filter(|record| record.chat == chat) {
        *have.entry(slot(record)).or_default() += 1;
    }
    let mut added = Vec::new();
    for record in export {
        match have.get_mut(&slot(record)) {
            Some(count) if *count > 0 => *count -= 1,
            _ => added.push(Record {
                chat,
                ..record.clone()
            }),
        }
    }
    added
}

/// This account's records with every merge applied, in order, and the
/// records the merges added. A merge whose chat is no longer listed (locked
/// since, say) adds nothing.
pub fn combine(account: &Source, merges: &[Merge]) -> (Source, Vec<Record>) {
    let mut combined = account.clone();
    let mut added = Vec::new();
    for merge_ in merges {
        let Some(chat) = account.chats.iter().position(|meta| meta.id == merge_.chat) else {
            continue;
        };
        let chat = chat as u32;
        let base: Vec<Record> = combined
            .records
            .iter()
            .chain(added.iter())
            .filter(|record| record.chat == chat)
            .cloned()
            .collect();
        let mut records = merge(&base, chat, &merge_.records);
        if !account.with_text {
            for record in &mut records {
                record.text = None;
            }
        }
        added.extend(records);
    }
    combined.records.extend(added.iter().cloned());
    combined.records.sort_by_key(|record| record.at);
    (combined, added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extras::insights::stats::tests::{record, source};

    const DAY: i32 = 20_000;

    fn at(chat: u32, from_me: bool, minute: i64, second: i64) -> Record {
        let mut record = record(chat, from_me, DAY, 9, minute);
        record.at += second;
        record
    }

    #[test]
    fn an_export_adds_only_what_the_archive_lacks() {
        // The archive has two messages from Ada at 9:00 (with seconds) and
        // one of the person's at 9:05.
        let archive = vec![at(0, false, 0, 12), at(0, false, 0, 40), at(0, true, 5, 3)];
        // The export has minute precision, and reaches back to 8:00. It also
        // has a third message at 9:00 the archive missed.
        let mut early = record(0, false, DAY, 8, 0);
        early.chat = 9;
        let export: Vec<Record> = [
            early.clone(),
            at(9, false, 0, 0),
            at(9, false, 0, 0),
            at(9, false, 0, 0),
            at(9, true, 5, 0),
        ]
        .into();
        let added = merge(&archive, 0, &export);
        assert_eq!(added.len(), 2, "the 8:00 one and the third at 9:00");
        assert!(added.iter().all(|record| record.chat == 0));
        assert_eq!(added[0].at, early.at);
        // Another chat's records never match.
        let other: Vec<Record> = archive
            .iter()
            .map(|r| Record {
                chat: 1,
                ..r.clone()
            })
            .collect();
        assert_eq!(merge(&other, 0, &export).len(), 5);
        // A different kind in the same minute is another message.
        let mut photo = at(9, true, 5, 0);
        photo.kind = Kind::Photo;
        assert_eq!(merge(&archive, 0, &[photo]).len(), 1);
    }

    #[test]
    fn merging_the_same_export_twice_adds_nothing() {
        let account = source(vec![at(0, false, 0, 12)]);
        let export = vec![record(0, false, DAY, 8, 0), at(0, false, 0, 0)];
        let once = Merge {
            chat: "a".into(),
            export: "Ada".into(),
            records: export,
        };
        let (combined, added) = combine(&account, std::slice::from_ref(&once));
        assert_eq!((combined.records.len(), added.len()), (2, 1));
        let (combined, added) = combine(&account, &[once.clone(), once]);
        assert_eq!((combined.records.len(), added.len()), (2, 1));
        assert!(combined.records.windows(2).all(|w| w[0].at <= w[1].at));
        // Removing the merge leaves the archive's numbers.
        let (combined, added) = combine(&account, &[]);
        assert_eq!((combined, added.len()), (account, 0));
    }

    #[test]
    fn the_chat_with_the_exports_title_is_suggested_first() {
        let chats = vec![
            ChatMeta {
                id: "1".into(),
                title: "Bob".into(),
                group: false,
            },
            ChatMeta {
                id: "2".into(),
                title: "Ada Lovelace".into(),
                group: false,
            },
            ChatMeta {
                id: "3".into(),
                title: "ada".into(),
                group: false,
            },
            ChatMeta {
                id: "export:Ada".into(),
                title: "Ada".into(),
                group: false,
            },
        ];
        assert_eq!(suggest("Ada", &chats), [2, 1, 0]);
    }
}
