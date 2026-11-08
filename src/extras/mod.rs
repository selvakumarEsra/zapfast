//! Fork extras (specs/extras.md): Favourites on the rail, the Insights
//! dashboard, and Keep, which keeps deleted and edited messages and every
//! attachment in local files.
//!
//! Everything lives here behind the `extras` feature. Upstream files carry
//! only small hooks marked `extras hook` (`git grep -n "extras hook"`).

pub mod config;
#[cfg(any(test, feature = "demo"))]
pub mod demo;
pub mod favourites;
pub mod insights;
pub mod keep;
pub mod ui;

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::app::App;
use crate::model::Message;

pub use config::Config;

pub struct Extras {
    pub config: Config,
    config_path: PathBuf,
    /// The settings changed and are saved on the next tick.
    pub dirty: bool,
    pub insights: insights::Insights,
    pub backfill: keep::Backfill,
}

impl Extras {
    pub fn load(config_dir: &Path) -> Self {
        let config_path = Config::path(config_dir);
        Self {
            config: Config::load(&config_path),
            config_path,
            dirty: false,
            insights: insights::Insights::default(),
            backfill: keep::Backfill::default(),
        }
    }
}

/// A message arrived (hook in `Event::Incoming`).
pub fn on_incoming(app: &mut App, chat: &str, message: &Message) {
    keep::on_incoming(app, chat, message);
}

/// Once per frame, after the app's events (hook in `background_frame`).
pub fn tick(app: &mut App) {
    if std::mem::take(&mut app.extras.dirty) {
        app.extras
            .config
            .save_to(&app.files, &app.extras.config_path);
    }
    if app.extras.backfill.running() {
        let due = app.extras.backfill.due(Instant::now());
        app.actions.extend(due);
        // Keep the frames coming while the queue drains.
        app.waker.wake();
    }
}
