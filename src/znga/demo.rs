//! Synthetic Znga states for demo screenshots (`--demo-page znga`,
//! `znga-drafts`, `znga-ask`, `znga-setup`). Sample content only.

use crate::app::App;

use super::drafts::{Draft, Kind};
use super::proactive::Alert;
use super::provider::Turn;
use super::{Briefing, Tab};

/// Applies a Znga demo flag; `false` when `part` is not one.
pub fn apply(app: &mut App, part: &str) -> bool {
    let tab = match part {
        "znga" => Tab::Today,
        "znga-drafts" => Tab::Drafts,
        "znga-ask" => Tab::Ask,
        "znga-setup" => Tab::Setup,
        _ => return false,
    };
    let account = app.account().id.as_str().to_owned();
    let chats: Vec<(String, String)> = app
        .account()
        .chats
        .iter()
        .filter(|chat| !chat.locked)
        .take(4)
        .map(|chat| (chat.id.clone(), app.chat_title(chat)))
        .collect();
    let pick = |i: usize| chats.get(i).or(chats.first()).cloned().unwrap_or_default();
    let now = crate::util::now();
    let znga = &mut app.znga;
    znga.config.enabled = true;
    znga.open = true;
    znga.tab = tab;
    let (chat, title) = pick(0);
    znga.alerts.push(Alert {
        account: account.clone(),
        chat: chat.clone(),
        chat_title: title.clone(),
        sender_name: title.clone(),
        text: "Can you call me before 6? It's about Saturday".into(),
        reason: "asks you something, time-sensitive, writing repeatedly".into(),
        at: now - 240,
    });
    znga.briefings.push(Briefing {
        text: format!(
            "WHAT HAPPENED\n• {title} wants to talk about Saturday's plans before 6pm.\n• {} \
             shared the photos from the trip and asked which to print.\n\nNEEDS YOU\n1. Call \
             {title} before 6pm.\n2. Pick the photos for {}.",
            pick(1).1,
            pick(1).1
        ),
        at: now - 600,
        proactive: true,
    });
    znga.drafts.add(Draft {
        id: 0,
        kind: Kind::AutoReply,
        account: account.clone(),
        chat: chat.clone(),
        chat_title: title.clone(),
        text: "yes! give me 10 min, calling you right after this meeting".into(),
        created: now - 200,
    });
    for i in [1, 2] {
        let (chat, title) = pick(i);
        znga.drafts.add(Draft {
            id: 0,
            kind: Kind::Update,
            account: account.clone(),
            chat,
            chat_title: title,
            text: "running about 20 min late, sorry!! save me a seat 🙏".into(),
            created: now - 60,
        });
    }
    znga.ask.push(Turn::user("Do I owe anyone a reply?"));
    znga.ask.push(Turn::assistant(format!(
        "Two people are waiting on you:\n1. {title} asked you to call before 6pm.\n2. {} \
         asked which photos to print.\nWant me to draft replies?",
        pick(1).1
    )));
    znga.unseen = 0;
    true
}
