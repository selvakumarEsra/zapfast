//! Insights (REQ-EXTRAS-003/004/005/009/010/011): a dashboard of the
//! person's WhatsApp activity, computed on this computer.

pub mod export;
pub mod fetch;
pub mod merge;
pub mod source;
pub mod stats;
pub mod ui;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::extras::keep::Kept;

use stats::{Coverage, Period, Record, Source, Stats};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Overview,
    People,
    Rhythm,
    Words,
    Kept,
    Saving,
    History,
}

/// Which records the dashboard shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    #[default]
    Account,
    Imported(usize),
}

/// A chat export waiting for the person to say which name is theirs, then
/// which chat it belongs to.
pub struct PendingExport {
    pub export: export::Export,
    pub senders: Vec<(String, usize)>,
    /// The name the person picked as theirs.
    pub me: Option<String>,
    /// The chats it may belong to, best guess first, as (id, title).
    pub chats: Vec<(String, String)>,
}

#[derive(Default)]
pub struct Insights {
    pub open: bool,
    pub tab: Tab,
    pub period: Period,
    pub choice: Choice,
    /// The archive's records with the merged exports added.
    pub account: Option<Source>,
    /// The archive's records alone.
    raw: Option<Source>,
    pub merges: Vec<merge::Merge>,
    /// The records the merges added to `raw`.
    added: Vec<Record>,
    pub fetch: fetch::Fetch,
    /// Chats chosen for fetching older history.
    pub selected: std::collections::HashSet<String>,
    /// Requests sent since the archive was last read.
    fetched_since_read: u32,
    /// The archive is to be read again once the current read is done.
    reread: bool,
    pub error: Option<String>,
    pub imports: Vec<Source>,
    pub pending: Option<PendingExport>,
    pub notice: Option<String>,
    pub kept: Vec<Kept>,
    pub missing: Vec<(String, String)>,
    loading: Option<mpsc::Receiver<source::Loaded>>,
    picking: Option<mpsc::Receiver<Option<Result<export::Export, String>>>>,
    loaded_at: Option<Instant>,
    /// The archive last read, so another account's numbers are never reused.
    loaded_from: Option<std::path::PathBuf>,
    /// Bumped whenever the records change, so the numbers are recomputed.
    generation: u64,
    cache: Option<((Choice, Period, i32, u64), Stats)>,
    coverage: Option<(u64, Vec<Coverage>)>,
}

impl Insights {
    pub fn loading(&self) -> bool {
        self.loading.is_some()
    }

    pub fn picking(&self) -> bool {
        self.picking.is_some()
    }

    /// The source the dashboard shows now.
    pub fn current(&self) -> Option<&Source> {
        match self.choice {
            Choice::Account => self.account.as_ref(),
            Choice::Imported(index) => self.imports.get(index),
        }
    }

    /// The numbers for the chosen source and period, computed once per change.
    pub fn stats(&mut self) -> Option<&Stats> {
        let today = source::today();
        let key = (self.choice, self.period, today, self.generation);
        if self.cache.as_ref().is_none_or(|(cached, _)| *cached != key) {
            let stats = stats::compute(self.current()?, self.period, today);
            self.cache = Some((key, stats));
        }
        self.cache.as_ref().map(|(_, stats)| stats)
    }

    /// Each chat's coverage, for this account with its merges.
    pub fn coverage(&mut self) -> &[Coverage] {
        if self
            .coverage
            .as_ref()
            .is_none_or(|(at, _)| *at != self.generation)
        {
            let list = self
                .raw
                .as_ref()
                .map(|raw| stats::coverage(raw, &self.added))
                .unwrap_or_default();
            self.coverage = Some((self.generation, list));
        }
        self.coverage.as_ref().map_or(&[], |(_, list)| list)
    }

    /// Applies the merges to the archive's records again.
    fn recombine(&mut self) {
        match &self.raw {
            Some(raw) => {
                let (combined, added) = merge::combine(raw, &self.merges);
                self.account = Some(combined);
                self.added = added;
            }
            None => {
                self.account = None;
                self.added.clear();
            }
        }
        self.generation += 1;
    }

    /// Demo screenshots: synthetic records, as if the archive was read.
    #[cfg(any(test, feature = "demo"))]
    pub fn set_demo(&mut self, source: Source, kept: Vec<Kept>, missing: Vec<(String, String)>) {
        self.raw = Some(source);
        self.recombine();
        self.kept = kept;
        self.missing = missing;
        self.loaded_at = Some(Instant::now());
    }

    /// Demo screenshots of the History tab.
    #[cfg(any(test, feature = "demo"))]
    pub fn set_demo_history(
        &mut self,
        full: Vec<String>,
        merge: merge::Merge,
        selected: Vec<String>,
    ) {
        if let Some(raw) = &mut self.raw {
            raw.full.extend(full);
        }
        self.merges.push(merge);
        self.selected.extend(selected);
        self.recombine();
    }

    /// Drops text derived from messages, when content analysis is turned off.
    pub fn forget_text(&mut self) {
        for source in self.raw.iter_mut().chain(self.imports.iter_mut()) {
            source.with_text = false;
            for record in &mut source.records {
                record.text = None;
            }
        }
        for merge in &mut self.merges {
            for record in &mut merge.records {
                record.text = None;
            }
        }
        self.recombine();
    }
}

/// The archive of the account shown in the window. The app-wide directory
/// holds no archive: each account keeps its own.
pub fn archive_path(app: &App) -> std::path::PathBuf {
    app.account().dirs.archive_db()
}

/// Starts reading this account's archive. Locked chats and channels are
/// left out here, before anything is read.
pub fn refresh(app: &mut App) {
    if app.extras.insights.loading.is_some() {
        return;
    }
    let request = request(app);
    app.extras.insights.loaded_from = Some(request.archive.clone());
    app.extras.insights.loading = Some(source::spawn(request, app.waker.clone()));
}

fn request(app: &App) -> source::Request {
    let chats = app
        .chats
        .iter()
        .filter(|chat| !chat.locked && chat.kind != crate::model::ChatKind::Broadcast)
        .map(|chat| stats::ChatMeta {
            id: chat.id.clone(),
            title: app.chat_title(chat),
            group: chat.is_group(),
        })
        .collect();
    source::Request {
        archive: archive_path(app),
        name: "This account".into(),
        chats,
        with_text: app.extras.config.content_analysis,
    }
}

/// Asks for a chat export with the system file picker, off this thread.
pub fn pick_export(app: &mut App) {
    if app.extras.insights.picking.is_some() {
        return;
    }
    let (sender, receiver) = mpsc::channel();
    let waker = app.waker.clone();
    std::thread::Builder::new()
        .name("extras-export-picker".into())
        .spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose a WhatsApp chat export")
                .add_filter("Chat export", &["txt", "zip"])
                .pick_file()
                .map(|path| export::read(&path));
            let _ = sender.send(picked);
            waker.wake();
        })
        .ok();
    app.extras.insights.picking = Some(receiver);
}

/// The person said which name is theirs; next they say which chat it is.
pub fn import_as(app: &mut App, me: &str) {
    let insights = &mut app.extras.insights;
    let Some(pending) = insights.pending.as_mut() else {
        return;
    };
    let chats = insights
        .raw
        .as_ref()
        .map(|raw| {
            merge::suggest(&pending.export.title, &raw.chats)
                .into_iter()
                .map(|i| (raw.chats[i].id.clone(), raw.chats[i].title.clone()))
                .collect()
        })
        .unwrap_or_default();
    pending.me = Some(me.to_owned());
    pending.chats = chats;
}

/// Merges the pending export into `chat` of this account, or with `None`
/// keeps it as a source of its own (REQ-EXTRAS-009).
pub fn merge_into(app: &mut App, chat: Option<String>) {
    let with_text = app.extras.config.content_analysis;
    let insights = &mut app.extras.insights;
    let Some(pending) = insights.pending.take() else {
        return;
    };
    let Some(me) = pending.me else {
        insights.pending = Some(pending);
        return;
    };
    let source = pending.export.to_source(&me, with_text);
    match chat {
        Some(chat) => {
            insights.merges.push(merge::Merge {
                chat,
                export: pending.export.title.clone(),
                records: source.records,
            });
            insights.recombine();
            insights.choice = Choice::Account;
            insights.tab = Tab::History;
        }
        None => {
            insights.imports.push(source);
            insights.choice = Choice::Imported(insights.imports.len() - 1);
            insights.generation += 1;
            insights.tab = Tab::Overview;
        }
    }
}

/// Takes a merged export out of this account's numbers.
pub fn unmerge(app: &mut App, index: usize) {
    let insights = &mut app.extras.insights;
    if index < insights.merges.len() {
        insights.merges.remove(index);
        insights.recombine();
    }
}

/// Starts asking the phone for the chosen chats' older history.
pub fn start_fetch(app: &mut App) {
    let insights = &mut app.extras.insights;
    // Only chats this account lists: never a locked chat or a channel.
    let listed: Vec<String> = insights
        .raw
        .as_ref()
        .map(|raw| {
            raw.chats
                .iter()
                .filter(|chat| insights.selected.contains(&chat.id) && !raw.full.contains(&chat.id))
                .map(|chat| chat.id.clone())
                .collect()
        })
        .unwrap_or_default();
    insights.fetch.start(listed);
    insights.fetched_since_read = 0;
}

/// Reads the archive again this often during a run, in requests.
const READ_EVERY: u32 = 4;

/// Sends the run's next request when it is due. Called every frame.
fn tick_fetch(app: &mut App) {
    if app.extras.insights.fetch.running()
        && app.extras.insights.loaded_from.as_ref() != Some(&archive_path(app))
    {
        // The window shows another account now: its phone is not asked
        // about this one's chats.
        app.extras.insights.fetch.stop();
    }
    let connected = matches!(app.account().link, crate::backend::LinkStatus::Connected);
    let insights = &mut app.extras.insights;
    let full = insights.raw.as_ref().map(|raw| &raw.full);
    let next = insights.fetch.next(Instant::now(), connected, |chat| {
        full.is_some_and(|full| full.contains(chat))
    });
    let ended = insights.fetch.take_ended();
    if let Some(chat) = next {
        // The person's own phone, as scrolling to the top of the chat asks it.
        app.account()
            .backend
            .send(crate::backend::Command::FetchOlder {
                chat,
                explicit: false,
            });
        app.extras.insights.fetched_since_read += 1;
    }
    let insights = &mut app.extras.insights;
    if ended || insights.fetched_since_read >= READ_EVERY {
        insights.fetched_since_read = 0;
        insights.reread = true;
    }
    if insights.reread && !insights.loading() {
        insights.reread = false;
        refresh(app);
    }
}

/// Collects finished loads and picks. Called every frame.
pub fn poll(app: &mut App) {
    let insights = &mut app.extras.insights;
    if let Some(receiver) = &insights.loading
        && let Ok(loaded) = receiver.try_recv()
    {
        insights.loading = None;
        insights.loaded_at = Some(Instant::now());
        match loaded.source {
            Ok(source) => {
                insights.raw = Some(source);
                insights.error = None;
            }
            Err(error) => insights.error = Some(error),
        }
        insights.kept = loaded.kept;
        insights.missing = loaded.missing;
        insights.recombine();
    }
    if let Some(receiver) = &insights.picking
        && let Ok(picked) = receiver.try_recv()
    {
        insights.picking = None;
        match picked {
            None => {}
            Some(Err(error)) => insights.notice = Some(error),
            Some(Ok(export)) => {
                let senders = export.senders();
                insights.notice = None;
                insights.pending = Some(PendingExport {
                    export,
                    senders,
                    me: None,
                    chats: Vec::new(),
                });
            }
        }
    }
    tick_fetch(app);
}

/// Opens or closes the dashboard; opening reads the archive again when the
/// last read is over two minutes old.
pub fn toggle(app: &mut App) {
    let archive = archive_path(app);
    let insights = &mut app.extras.insights;
    insights.open = !insights.open;
    let stale = insights
        .loaded_at
        .is_none_or(|at| at.elapsed() > Duration::from_secs(120));
    let other_account = insights.loaded_from.as_ref() != Some(&archive);
    if insights.open && (stale || other_account) {
        if other_account {
            // Never show one account's numbers in another's window.
            insights.raw = None;
            insights.merges.clear();
            insights.selected.clear();
            insights.fetch.stop();
            insights.fetch.take_ended();
            insights.kept.clear();
            insights.missing.clear();
            insights.recombine();
        }
        refresh(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insights_reads_the_active_accounts_archive() {
        let root = tempfile::tempdir().unwrap();
        let dirs = crate::paths::AppDirs::under(root.path());
        let app_wide = dirs.archive_db();
        let app = App::headless(dirs, crate::settings::Settings::default()).0;
        let path = request(&app).archive;
        assert_eq!(path, app.account().dirs.archive_db());
        assert_ne!(path, app_wide, "each account keeps its own archive");
    }
}
