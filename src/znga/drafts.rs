//! The twin's drafts, waiting for the person's approval. Nothing here is
//! ever sent without a click on Send.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Prepared automatically when a message arrived in a twin chat.
    AutoReply,
    /// Asked for with "Suggest reply".
    Suggestion,
    /// A check-in with someone close who has been quiet.
    CheckIn,
    /// One of several drafts from an "update my people" instruction.
    Update,
    /// The composer text, rewritten or translated.
    Rewrite,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::AutoReply => "Twin reply",
            Kind::Suggestion => "Suggested reply",
            Kind::CheckIn => "Check-in",
            Kind::Update => "Update",
            Kind::Rewrite => "Rewrite",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub id: u64,
    pub kind: Kind,
    pub account: String,
    pub chat: String,
    pub chat_title: String,
    pub text: String,
    /// Unix seconds.
    pub created: i64,
}

#[derive(Default)]
pub struct Drafts {
    items: Vec<Draft>,
    next: u64,
}

impl Drafts {
    /// Adds a draft. An automatic reply replaces that chat's earlier
    /// automatic reply instead of stacking.
    pub fn add(&mut self, mut draft: Draft) -> u64 {
        self.next += 1;
        draft.id = self.next;
        if matches!(draft.kind, Kind::AutoReply | Kind::Rewrite) {
            self.items.retain(|old| {
                !(old.kind == draft.kind && old.account == draft.account && old.chat == draft.chat)
            });
        }
        let id = draft.id;
        self.items.push(draft);
        id
    }

    pub fn items(&self) -> &[Draft] {
        &self.items
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Draft> {
        self.items.iter_mut().find(|draft| draft.id == id)
    }

    /// Removes and returns the draft, so it can be sent exactly once.
    pub fn take(&mut self, id: u64) -> Option<Draft> {
        let index = self.items.iter().position(|draft| draft.id == id)?;
        Some(self.items.remove(index))
    }

    /// Removes and returns every draft of `kind`, oldest first.
    pub fn take_kind(&mut self, kind: Kind) -> Vec<Draft> {
        let (taken, kept) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|draft| draft.kind == kind);
        self.items = kept;
        taken
    }

    pub fn count(&self, kind: Kind) -> usize {
        self.items.iter().filter(|draft| draft.kind == kind).count()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear_chat(&mut self, account: &str, chat: &str) {
        self.items
            .retain(|draft| !(draft.account == account && draft.chat == chat));
    }
}

/// Case-, accent- and spacing-insensitive form of a name.
fn fold(name: &str) -> String {
    crate::util::search_key(name)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The chat whose title matches `name`: an exact match first, then a
/// unique title starting with it, then a unique title containing it as a
/// whole word. `None` when there is no match or it is ambiguous, so a
/// draft is never addressed by a guess.
pub fn match_chat<'a>(name: &str, chats: &'a [(String, String)]) -> Option<&'a str> {
    let wanted = fold(name);
    if wanted.is_empty() {
        return None;
    }
    let folded: Vec<(&str, String)> = chats
        .iter()
        .map(|(id, title)| (id.as_str(), fold(title)))
        .collect();
    let unique = |hits: Vec<&'a str>| (hits.len() == 1).then(|| hits[0]);
    let exact: Vec<&str> = folded
        .iter()
        .filter(|(_, title)| *title == wanted)
        .map(|(id, _)| *id)
        .collect();
    if !exact.is_empty() {
        return unique(exact);
    }
    let prefix: Vec<&str> = folded
        .iter()
        .filter(|(_, title)| title.starts_with(&format!("{wanted} ")))
        .map(|(id, _)| *id)
        .collect();
    if !prefix.is_empty() {
        return unique(prefix);
    }
    unique(
        folded
            .iter()
            .filter(|(_, title)| title.split(' ').any(|word| word == wanted))
            .map(|(id, _)| *id)
            .collect(),
    )
}

/// One addressed message from a fan-out answer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Addressed {
    pub to: String,
    pub text: String,
}

/// The JSON array in a model's fan-out answer, tolerating prose or code
/// fences around it.
pub fn parse_fan_out(answer: &str) -> Result<Vec<Addressed>, String> {
    let start = answer
        .find('[')
        .ok_or("Znga did not name anyone to message.")?;
    let end = answer.rfind(']').ok_or("Znga's answer was cut short.")?;
    if end < start {
        return Err("Znga's answer could not be read.".into());
    }
    let items: Vec<Addressed> = serde_json::from_str(&answer[start..=end])
        .map_err(|_| "Znga's answer could not be read.".to_owned())?;
    Ok(items
        .into_iter()
        .filter(|item| !item.to.trim().is_empty() && !item.text.trim().is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(kind: Kind, chat: &str, text: &str) -> Draft {
        Draft {
            id: 0,
            kind,
            account: "1".into(),
            chat: chat.into(),
            chat_title: chat.into(),
            text: text.into(),
            created: 0,
        }
    }

    #[test]
    fn a_newer_auto_reply_replaces_the_pending_one() {
        let mut drafts = Drafts::default();
        drafts.add(draft(Kind::AutoReply, "mom", "first"));
        drafts.add(draft(Kind::Suggestion, "mom", "kept"));
        drafts.add(draft(Kind::AutoReply, "mom", "second"));
        drafts.add(draft(Kind::AutoReply, "bob", "bob's"));
        let texts: Vec<&str> = drafts.items().iter().map(|d| d.text.as_str()).collect();
        assert_eq!(texts, vec!["kept", "second", "bob's"]);
    }

    #[test]
    fn a_draft_is_taken_once() {
        let mut drafts = Drafts::default();
        let id = drafts.add(draft(Kind::Update, "mom", "late"));
        assert!(drafts.take(id).is_some());
        assert!(drafts.take(id).is_none());
    }

    #[test]
    fn send_all_takes_only_the_remaining_updates() {
        let mut drafts = Drafts::default();
        let a = drafts.add(draft(Kind::Update, "mom", "late"));
        drafts.add(draft(Kind::Update, "alex", "late"));
        drafts.add(draft(Kind::CheckIn, "gran", "hi"));
        drafts.take(a);
        let sent = drafts.take_kind(Kind::Update);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].chat, "alex");
        assert_eq!(drafts.len(), 1);
        assert!(drafts.take_kind(Kind::Update).is_empty());
    }

    #[test]
    fn names_match_chats_without_guessing() {
        let chats = vec![
            ("mom".to_owned(), "Mom ❤️".to_owned()),
            ("alex".to_owned(), "Alex Müller".to_owned()),
            ("alexa".to_owned(), "Alexa Jones".to_owned()),
            ("sam1".to_owned(), "Sam Lee".to_owned()),
            ("sam2".to_owned(), "Sam Park".to_owned()),
        ];
        assert_eq!(match_chat("mom", &chats), Some("mom"));
        assert_eq!(match_chat("Alex", &chats), Some("alex"));
        assert_eq!(match_chat("alex muller", &chats), Some("alex"));
        assert_eq!(match_chat("Sam", &chats), None, "ambiguous");
        assert_eq!(match_chat("Jones", &chats), Some("alexa"));
        assert_eq!(match_chat("Zed", &chats), None);
    }

    #[test]
    fn fan_out_answers_are_read_through_fences() {
        let answer = "Sure!\n```json\n[{\"to\":\"Mom\",\"text\":\"running 20 min late\"},{\"to\":\"\",\"text\":\"x\"}]\n```";
        let items = parse_fan_out(answer).unwrap();
        assert_eq!(
            items,
            vec![Addressed {
                to: "Mom".into(),
                text: "running 20 min late".into()
            }]
        );
        assert!(parse_fan_out("nobody").is_err());
    }
}
