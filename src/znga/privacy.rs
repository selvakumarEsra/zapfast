//! Which chats Znga may read at all.

use crate::model::{Chat, ChatKind};

use super::config::Config;

/// Facts about a chat that decide whether Znga may see it.
#[derive(Clone, Copy, Debug)]
pub struct ChatFacts<'a> {
    pub account: &'a str,
    pub chat: &'a str,
    pub kind: ChatKind,
    pub locked: bool,
}

impl<'a> ChatFacts<'a> {
    pub fn of(account: &'a str, chat: &'a Chat) -> Self {
        Self {
            account,
            chat: &chat.id,
            kind: chat.kind,
            locked: chat.locked,
        }
    }
}

/// Whether Znga may record or send anything from this chat.
pub fn visible(config: &Config, facts: ChatFacts<'_>) -> bool {
    !facts.locked
        && facts.kind != ChatKind::Broadcast
        && (config.include_groups || facts.kind != ChatKind::Group)
        && !config.is_excluded(facts.account, facts.chat)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(kind: ChatKind, locked: bool) -> ChatFacts<'static> {
        ChatFacts {
            account: "1",
            chat: "a@s.whatsapp.net",
            kind,
            locked,
        }
    }

    #[test]
    fn locked_excluded_and_broadcast_chats_are_never_seen() {
        let mut config = Config::default();
        assert!(visible(&config, facts(ChatKind::Direct, false)));
        assert!(!visible(&config, facts(ChatKind::Direct, true)));
        assert!(!visible(&config, facts(ChatKind::Broadcast, false)));
        config
            .excluded_chats
            .insert(super::super::config::chat_key("1", "a@s.whatsapp.net"));
        assert!(!visible(&config, facts(ChatKind::Direct, false)));
    }

    #[test]
    fn groups_follow_the_switch() {
        let mut config = Config::default();
        assert!(visible(&config, facts(ChatKind::Group, false)));
        config.include_groups = false;
        assert!(!visible(&config, facts(ChatKind::Group, false)));
    }
}
