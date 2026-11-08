//! Insights (REQ-EXTRAS-003/004/005): a dashboard of the person's WhatsApp
//! activity, computed on this computer.

pub mod export;
pub mod source;
pub mod stats;
pub mod ui;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::extras::keep::Kept;

use stats::{Period, Source, Stats};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Overview,
    People,
    Rhythm,
    Words,
    Kept,
    Saving,
}

/// Which records the dashboard shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    #[default]
    Account,
    Imported(usize),
}

/// A chat export waiting for the person to say which name is theirs.
pub struct PendingExport {
    pub export: export::Export,
    pub senders: Vec<(String, usize)>,
}

#[derive(Default)]
pub struct Insights {
    pub open: bool,
    pub tab: Tab,
    pub period: Period,
    pub choice: Choice,
    pub account: Option<Source>,
    pub error: Option<String>,
    pub imports: Vec<Source>,
    pub pending: Option<PendingExport>,
    pub notice: Option<String>,
    pub kept: Vec<Kept>,
    pub missing: Vec<(String, String)>,
    loading: Option<mpsc::Receiver<source::Loaded>>,
    picking: Option<mpsc::Receiver<Option<Result<export::Export, String>>>>,
    loaded_at: Option<Instant>,
    /// Bumped whenever the records change, so the numbers are recomputed.
    generation: u64,
    cache: Option<((Choice, Period, i32, u64), Stats)>,
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

    /// Demo screenshots: synthetic records, as if the archive was read.
    #[cfg(any(test, feature = "demo"))]
    pub fn set_demo(&mut self, source: Source, kept: Vec<Kept>, missing: Vec<(String, String)>) {
        self.account = Some(source);
        self.kept = kept;
        self.missing = missing;
        self.loaded_at = Some(Instant::now());
        self.generation += 1;
    }

    /// Drops text derived from messages, when content analysis is turned off.
    pub fn forget_text(&mut self) {
        for source in self.account.iter_mut().chain(self.imports.iter_mut()) {
            source.with_text = false;
            for record in &mut source.records {
                record.text = None;
            }
        }
        self.generation += 1;
    }
}

/// Starts reading this account's archive. Locked chats and channels are
/// left out here, before anything is read.
pub fn refresh(app: &mut App) {
    if app.extras.insights.loading.is_some() {
        return;
    }
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
    let request = source::Request {
        archive: app.dirs.archive_db(),
        name: "This account".into(),
        chats,
        with_text: app.extras.config.content_analysis,
    };
    app.extras.insights.loading = Some(source::spawn(request, app.waker.clone()));
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

/// The person said which name is theirs.
pub fn import_as(app: &mut App, me: &str) {
    let with_text = app.extras.config.content_analysis;
    let insights = &mut app.extras.insights;
    if let Some(pending) = insights.pending.take() {
        insights
            .imports
            .push(pending.export.to_source(me, with_text));
        insights.choice = Choice::Imported(insights.imports.len() - 1);
        insights.generation += 1;
        insights.tab = Tab::Overview;
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
                insights.account = Some(source);
                insights.error = None;
            }
            Err(error) => insights.error = Some(error),
        }
        insights.kept = loaded.kept;
        insights.missing = loaded.missing;
        insights.generation += 1;
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
                insights.pending = Some(PendingExport { export, senders });
            }
        }
    }
}

/// Opens or closes the dashboard; opening reads the archive again when the
/// last read is over two minutes old.
pub fn toggle(app: &mut App) {
    let insights = &mut app.extras.insights;
    insights.open = !insights.open;
    if insights.open
        && insights
            .loaded_at
            .is_none_or(|at| at.elapsed() > Duration::from_secs(120))
    {
        refresh(app);
    }
}
