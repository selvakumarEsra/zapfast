//! The bridge between Znga and the app. Upstream files call these from
//! lines marked `znga hook`; everything else about Znga stays in this
//! folder.

use crate::app::App;
use crate::model::{Action, Content, Message};

use super::config::chat_key;
use super::drafts::{Draft, Kind};
use super::privacy::{ChatFacts, visible};
use super::proactive::{self, Context, Seen};
use super::prompt::{self, Line};
use super::{Effect, Purpose};

/// How often our own sent messages are re-read for the twin's style.
const STYLE_SCAN: std::time::Duration = std::time::Duration::from_secs(60);
const CHECKIN_SCAN: std::time::Duration = std::time::Duration::from_secs(10 * 60);

fn away(app: &App) -> bool {
    app.window_hidden || !app.window_focused
}

fn user_part(id: &str) -> &str {
    id.split(['@', ':']).next().unwrap_or(id)
}

/// Called for every live incoming message (`Event::Incoming`), on the
/// account it arrived on (the app makes that account active meanwhile).
pub fn observe_incoming(app: &mut App, chat_id: &str, message: &Message) {
    if !app.znga.config.enabled || message.from_me {
        return;
    }
    let account = app.account().id.as_str().to_owned();
    let now = crate::util::now();
    let Some(chat) = app.chat(chat_id) else {
        return;
    };
    if !visible(&app.znga.config, ChatFacts::of(&account, chat)) {
        return;
    }
    let muted = chat.muted(now);
    let group = chat.is_group();
    let chat_title = app.chat_title(chat);
    let sender_name = app.display_name_or(&message.sender, message.sender_name.as_deref());
    let text = app.message_text(message);
    let mentions_me = {
        let me: Vec<&str> = [app.account().me.as_deref(), app.account().me_lid.as_deref()]
            .into_iter()
            .flatten()
            .map(user_part)
            .collect();
        message
            .mentions
            .iter()
            .any(|mention| me.contains(&user_part(&mention.id)))
    };
    let seen = Seen {
        account: account.clone(),
        chat: chat_id.to_owned(),
        chat_title: chat_title.clone(),
        group,
        sender: message.sender.clone(),
        sender_name: sender_name.clone(),
        text: text.clone(),
        at: message.timestamp,
        score: Default::default(),
    };
    let alert = app
        .znga
        .activity
        .observe(seen, Context { mentions_me, muted });
    if let Some(alert) = alert
        && app.znga.config.alerts
    {
        let reason = alert.reason.clone();
        app.znga.push_alert(alert);
        let reading = !away(app) && app.open_chat.as_deref() == Some(chat_id);
        if !reading {
            notify(
                app,
                format!("Znga · {chat_title} needs you"),
                format!("{sender_name}: {} ({reason})", clip(&text, 140)),
                &account,
                chat_id,
                Some(message.id.clone()),
            );
        }
    }
    if app.znga.config.is_twin(&account, chat_id) && !muted {
        request_draft(app, &account, chat_id, Kind::AutoReply);
    }
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        text.chars().take(max).collect::<String>() + "…"
    }
}

/// A Znga notification, unless notifications are off. Behind the app lock
/// it says nothing about the content.
fn notify(
    app: &mut App,
    title: String,
    body: String,
    account: &str,
    chat: &str,
    message: Option<String>,
) {
    if !app.settings.notifications {
        return;
    }
    let (title, body) = if app.app_lock.is_locked() {
        ("Znga".to_owned(), "Open ZapFast to see it.".to_owned())
    } else {
        (title, body)
    };
    let account = app
        .accounts
        .iter()
        .find(|known| known.id.as_str() == account)
        .unwrap_or_else(|| app.account())
        .id
        .clone();
    let waker = app.waker.clone();
    let sound = app.settings.mention_sound.clone();
    let opens = std::sync::Arc::clone(&app.notification_opens);
    app.notifications.show(
        title,
        body,
        None,
        sound,
        crate::notify::NotificationTarget {
            account,
            chat: chat.to_owned(),
            message,
        },
        opens,
        move || waker.wake(),
        false,
    );
}

/// The chat's recent lines: the loaded conversation when there is one,
/// else what Znga has seen.
fn lines_of(app: &App, account: &str, chat: &str) -> Vec<Line> {
    let index = app
        .accounts
        .iter()
        .position(|known| known.id.as_str() == account);
    if let Some(index) = index
        && let Some(conversation) = app.accounts[index].conversations.get(chat)
        && !conversation.messages.is_empty()
    {
        let skip = conversation.messages.len().saturating_sub(30);
        return conversation.messages[skip..]
            .iter()
            .filter(|message| !matches!(message.content, Content::Call { .. }))
            .map(|message| Line {
                speaker: app.display_name_or(&message.sender, message.sender_name.as_deref()),
                text: app.message_text(message),
                from_me: message.from_me,
            })
            .collect();
    }
    app.znga
        .activity
        .recent_in(account, chat, 30)
        .into_iter()
        .map(|seen| Line {
            speaker: seen.sender_name,
            text: seen.text,
            from_me: false,
        })
        .collect()
}

fn your_name(app: &App, account: &str) -> String {
    let configured = app.znga.config.your_name.trim();
    if !configured.is_empty() {
        return configured.to_owned();
    }
    app.accounts
        .iter()
        .find(|known| known.id.as_str() == account)
        .and_then(|known| known.me_name.clone())
        .unwrap_or_default()
}

fn title_of(app: &App, account: &str, chat: &str) -> String {
    app.accounts
        .iter()
        .find(|known| known.id.as_str() == account)
        .and_then(|known| known.chats.iter().find(|c| c.id == chat))
        .map(|found| app.chat_title(found))
        .unwrap_or_else(|| chat.to_owned())
}

/// Asks the twin for a reply or check-in for `chat`. An automatic request
/// is skipped while one for the same chat is still running.
pub fn request_draft(app: &mut App, account: &str, chat: &str, kind: Kind) {
    let same = |purpose: &Purpose| {
        matches!(purpose, Purpose::Draft { account: a, chat: c, kind: k, .. }
            if a == account && c == chat && *k == kind)
    };
    if app.znga.waiting_for(same) {
        return;
    }
    let lines = lines_of(app, account, chat);
    let name = your_name(app, account);
    let chat_title = title_of(app, account, chat);
    let style = app.znga.style_for(account).describe();
    let examples = app.znga.examples.pick(&chat_key(account, chat), 12);
    let voice = app.znga.config.voice_notes.clone();
    let prompt = match kind {
        Kind::CheckIn => prompt::checkin(
            &name,
            &style,
            &voice,
            &examples,
            &chat_title,
            app.znga.config.checkin_days,
            &lines,
        ),
        _ => prompt::twin_reply(&name, &style, &voice, &examples, &chat_title, &lines),
    };
    let purpose = Purpose::Draft {
        kind,
        account: account.to_owned(),
        chat: chat.to_owned(),
        chat_title,
    };
    if let Err(error) = app.znga.submit(prompt, purpose)
        && !matches!(kind, Kind::AutoReply | Kind::CheckIn)
    {
        app.znga.set_status(error, true);
    }
}

/// "Brief me": a briefing from everything since the last one, or, with
/// nothing new, from the most recent activity.
pub fn request_briefing(app: &mut App, proactive: bool) {
    let mut items = app.znga.activity.since_last_briefing();
    if items.is_empty() && !proactive {
        items = app.znga.activity.recent(60);
    }
    if items.is_empty() {
        app.znga.set_status(
            "Nothing to brief yet: Znga reports on messages that arrive while it is on.",
            false,
        );
        return;
    }
    let account = app.account().id.as_str().to_owned();
    let prompt = prompt::briefing(&your_name(app, &account), &items);
    match app.znga.submit(prompt, Purpose::Briefing { proactive }) {
        Ok(()) => {
            app.znga.activity.mark_briefed(crate::util::now());
            if !proactive {
                app.znga.set_status("Znga is writing your briefing…", false);
            }
        }
        Err(error) => app.znga.set_status(error, true),
    }
}

/// "Update my people": one instruction, a draft per named person.
pub fn request_fan_out(app: &mut App, instruction: &str) {
    let account = app.account().id.as_str().to_owned();
    let config = app.znga.config.clone();
    let mut chats: Vec<(String, String, i64)> = app
        .account()
        .chats
        .iter()
        .filter(|chat| visible(&config, ChatFacts::of(&account, chat)))
        .map(|chat| (chat.id.clone(), app.chat_title(chat), chat.last_activity))
        .collect();
    // Twin chats first, then the most recently active.
    chats.sort_by_key(|(id, _, last)| (!config.is_twin(&account, id), -last));
    chats.truncate(120);
    let names: Vec<String> = chats.iter().map(|(_, title, _)| title.clone()).collect();
    let pairs: Vec<(String, String)> = chats
        .into_iter()
        .map(|(id, title, _)| (id, title))
        .collect();
    let name = your_name(app, &account);
    let style = app.znga.style_for(&account).describe();
    let examples = app.znga.examples.pick("", 12);
    let prompt = prompt::fan_out(
        &name,
        &style,
        &config.voice_notes,
        &examples,
        instruction,
        &names,
    );
    match app.znga.submit(
        prompt,
        Purpose::FanOut {
            account,
            chats: pairs,
        },
    ) {
        Ok(()) => app
            .znga
            .set_status("Znga is drafting your messages…", false),
        Err(error) => app.znga.set_status(error, true),
    }
}

/// "Ask Znga".
pub fn request_ask(app: &mut App, question: String) {
    let question = question.trim().to_owned();
    if question.is_empty() {
        return;
    }
    let account = app.account().id.as_str().to_owned();
    let context = app.znga.activity.recent(120);
    let prompt = prompt::ask(
        &your_name(app, &account),
        &context,
        &app.znga.ask,
        &question,
    );
    app.znga.ask.push(super::provider::Turn::user(question));
    if let Err(error) = app.znga.submit(prompt, Purpose::Ask) {
        app.znga
            .ask
            .push(super::provider::Turn::assistant(format!("⚠ {error}")));
    }
}

/// Rewrite or translate the composer text of the open chat.
pub fn request_rewrite(app: &mut App, prompt: super::provider::Prompt) {
    let Some(chat) = app.open_chat.clone() else {
        app.znga.set_status("Open a chat first.", true);
        return;
    };
    let account = app.account().id.as_str().to_owned();
    let purpose = Purpose::Draft {
        kind: Kind::Rewrite,
        chat_title: title_of(app, &account, &chat),
        account,
        chat,
    };
    match app.znga.submit(prompt, purpose) {
        Ok(()) => app.znga.set_status("Znga is rewriting…", false),
        Err(error) => app.znga.set_status(error, true),
    }
}

/// Sends an approved draft. The only way Znga's text reaches anyone else,
/// and only from the person's click.
pub fn send_draft(app: &mut App, draft: Draft) {
    let Some(account) = app
        .accounts
        .iter()
        .find(|known| known.id.as_str() == draft.account)
        .map(|known| known.id.clone())
    else {
        app.znga
            .set_status("That account is no longer linked.", true);
        return;
    };
    app.deferred_account_actions.push((
        account,
        Action::SendText {
            chat: draft.chat,
            text: draft.text,
            quoting: None,
        },
    ));
}

/// Puts a draft in the composer of its chat, opening the chat when needed.
pub fn use_in_composer(app: &mut App, draft: Draft) {
    if app.account().id.as_str() != draft.account {
        app.toast("Switch to that account to edit this draft.");
        return;
    }
    if app.open_chat.as_deref() == Some(draft.chat.as_str()) {
        app.composer = draft.text;
        app.focus_composer = true;
    } else {
        app.actions.push(Action::OpenChat(draft.chat.clone()));
        app.actions.push(Action::PrefillComposer {
            chat: draft.chat,
            text: draft.text,
        });
    }
}

/// Our own chat for the active account (or the first linked one).
fn own_chat(app: &App) -> Option<(crate::model::AccountId, String)> {
    std::iter::once(app.account())
        .chain(app.accounts.iter())
        .find_map(|account| {
            let chat = proactive::own_chat(account.me.as_deref()?)?;
            Some((account.id.clone(), chat))
        })
}

fn apply(app: &mut App, effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::Notify {
                title,
                body,
                account,
                chat,
                message,
                only_when_away,
            } => {
                if only_when_away && !away(app) {
                    continue;
                }
                let account = if account.is_empty() {
                    app.account().id.as_str().to_owned()
                } else {
                    account
                };
                notify(app, title, body, &account, &chat, message);
            }
            Effect::SelfNote(text) => {
                if let Some((account, chat)) = own_chat(app) {
                    app.deferred_account_actions.push((
                        account,
                        Action::SendText {
                            chat,
                            text,
                            quoting: None,
                        },
                    ));
                }
            }
        }
    }
}

/// Re-reads our own sent text messages from the loaded conversations, for
/// the twin's style.
fn scan_style(app: &mut App) {
    let mut learned: Vec<(String, String, String)> = Vec::new();
    for account in &app.accounts {
        let id = account.id.as_str().to_owned();
        for (chat, conversation) in &account.conversations {
            if app.znga.config.is_excluded(&id, chat) {
                continue;
            }
            if account
                .chats
                .iter()
                .any(|known| known.id == *chat && known.locked)
            {
                continue;
            }
            let skip = conversation.messages.len().saturating_sub(40);
            for message in &conversation.messages[skip..] {
                if message.from_me
                    && let Content::Text { text, .. } = &message.content
                {
                    learned.push((id.clone(), chat.clone(), text.clone()));
                }
            }
        }
    }
    for (account, chat, text) in &learned {
        app.znga.learn(account, chat, text);
    }
    let accounts: Vec<String> = app
        .accounts
        .iter()
        .map(|account| account.id.as_str().to_owned())
        .collect();
    if app.znga.remeasure(&accounts) {
        app.znga.save_styles(&app.files);
    }
}

fn scan_checkins(app: &mut App) {
    if app.znga.config.twin_chats.is_empty() {
        return;
    }
    let chats: Vec<(String, String, i64)> = app
        .accounts
        .iter()
        .flat_map(|account| {
            account.chats.iter().map(move |chat| {
                (
                    account.id.as_str().to_owned(),
                    chat.id.clone(),
                    chat.last_activity,
                )
            })
        })
        .collect();
    let now = crate::util::now();
    let due = proactive::checkins_due(&app.znga.config, &chats, &app.znga.checkins_suggested, now);
    for (account, chat) in due.into_iter().take(2) {
        app.znga
            .checkins_suggested
            .insert(chat_key(&account, &chat), now);
        request_draft(app, &account, &chat, Kind::CheckIn);
    }
}

/// Once a frame, after the app handled its events.
pub fn tick(app: &mut App) {
    let now = crate::util::now();
    for (purpose, answer) in app.znga.runner.poll() {
        let effects = app.znga.handle(purpose, answer, now);
        apply(app, effects);
    }
    app.znga.save_if_dirty(&app.files);
    if !app.znga.config.enabled {
        return;
    }
    let instant = std::time::Instant::now();
    if app
        .znga
        .last_scan
        .is_none_or(|last| instant.duration_since(last) >= STYLE_SCAN)
    {
        app.znga.last_scan = Some(instant);
        scan_style(app);
    }
    if app.znga.blocked().is_some() {
        return;
    }
    let is_briefing = |purpose: &Purpose| matches!(purpose, Purpose::Briefing { .. });
    if app.znga.activity.due(&app.znga.config, now, away(app)) && !app.znga.waiting_for(is_briefing)
    {
        request_briefing(app, true);
    }
    if app
        .znga
        .last_checkin_scan
        .is_none_or(|last| instant.duration_since(last) >= CHECKIN_SCAN)
    {
        app.znga.last_checkin_scan = Some(instant);
        scan_checkins(app);
    }
}
