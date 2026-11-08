//! Znga's own configuration file, `znga.json` beside ZapFast's settings.
//!
//! Kept apart from [`crate::settings::Settings`] on purpose: upstream
//! ZapFast owns that struct, and a fork-only field there would conflict on
//! every merge. API keys are never stored here (see [`super::keys`]).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Which model answers Znga's requests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// An Ollama-compatible server on this machine: nothing leaves it.
    #[default]
    Local,
    /// Anthropic's Messages API with the person's own API key.
    Claude,
    /// Google's Generative Language API with the person's own API key.
    Gemini,
    /// Any OpenAI-compatible chat-completions server (OpenAI, LM Studio,
    /// llama.cpp, vLLM…).
    OpenAi,
    /// The person's own installed Claude Code CLI, run headless
    /// (`claude -p`). It signs in by itself (its saved login or
    /// `CLAUDE_CODE_OAUTH_TOKEN`); ZapFast never reads or stores the token.
    /// For personal use on this machine.
    #[serde(rename = "claude-code")]
    ClaudeCode,
}

impl Engine {
    pub const ALL: [Engine; 5] = [
        Engine::Local,
        Engine::ClaudeCode,
        Engine::Claude,
        Engine::Gemini,
        Engine::OpenAi,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Engine::Local => "On this computer (Ollama)",
            Engine::Claude => "Claude (Anthropic API key)",
            Engine::Gemini => "Gemini (Google API key)",
            Engine::OpenAi => "OpenAI-compatible (API key)",
            Engine::ClaudeCode => "Claude Code on this computer (your login)",
        }
    }

    /// Whether message text leaves this machine with this engine. An
    /// OpenAI-compatible endpoint on localhost stays local.
    pub fn is_cloud(self, endpoint: &str) -> bool {
        match self {
            Engine::Local => false,
            Engine::OpenAi => !is_loopback(endpoint),
            Engine::Claude | Engine::Gemini | Engine::ClaudeCode => true,
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Engine::Local => "llama3.2",
            Engine::Claude => "claude-opus-5-5",
            Engine::Gemini => "gemini-2.5-flash",
            Engine::OpenAi => "gpt-5-mini",
            Engine::ClaudeCode => "claude-opus-5-5",
        }
    }

    pub fn default_endpoint(self) -> &'static str {
        match self {
            Engine::Local => "http://127.0.0.1:11434",
            Engine::Claude => "https://api.anthropic.com",
            Engine::Gemini => "https://generativelanguage.googleapis.com",
            Engine::OpenAi => "https://api.openai.com/v1",
            // The CLI to run; found on PATH or in the usual install places.
            Engine::ClaudeCode => "claude",
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Engine::Local | Engine::ClaudeCode)
    }

    /// The keyring account and the environment variables checked for a key.
    pub fn key_sources(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Engine::Local => ("local", &[]),
            Engine::Claude => ("claude", &["ANTHROPIC_API_KEY"]),
            Engine::Gemini => ("gemini", &["GEMINI_API_KEY", "GOOGLE_API_KEY"]),
            Engine::OpenAi => ("openai", &["OPENAI_API_KEY"]),
            Engine::ClaudeCode => ("claude-code", &[]),
        }
    }
}

fn is_loopback(endpoint: &str) -> bool {
    let rest = endpoint
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let host = rest.split(['/', ':']).next().unwrap_or_default();
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// A chat across accounts: `ChatId` is unique only inside one account.
pub fn chat_key(account: &str, chat: &str) -> String {
    format!("{account}/{chat}")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Master switch. Off until the person turns Znga on.
    pub enabled: bool,
    pub engine: Engine,
    /// Model name; empty uses the engine's default.
    pub model: String,
    /// Base URL; empty uses the engine's default.
    pub endpoint: String,
    /// The person accepted that a cloud engine receives message text.
    pub cloud_consent: bool,
    /// Score incoming messages and alert on the ones that need the person.
    pub alerts: bool,
    /// Brief the person proactively after time away.
    pub briefings: bool,
    /// Messages since the last briefing before a proactive one is due.
    pub briefing_min_messages: usize,
    /// Minutes between proactive briefings.
    pub briefing_interval_minutes: u64,
    /// Also send proactive briefings to our own "Message yourself" chat.
    pub self_notes: bool,
    /// Chats (by [`chat_key`]) where the twin prepares replies on arrival
    /// and suggests check-ins: the person's close ones.
    pub twin_chats: BTreeSet<String>,
    /// Chats Znga must never read.
    pub excluded_chats: BTreeSet<String>,
    /// Include group chats in alerts and briefings.
    pub include_groups: bool,
    /// Days of silence in a twin chat before a check-in is suggested.
    pub checkin_days: u32,
    /// What the twin calls the person, used in prompts. Empty uses the
    /// account's profile name.
    pub your_name: String,
    /// Extra words about how the person writes ("I use 'haha' a lot").
    pub voice_notes: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: false,
            engine: Engine::Local,
            model: String::new(),
            endpoint: String::new(),
            cloud_consent: false,
            alerts: true,
            briefings: true,
            briefing_min_messages: 5,
            briefing_interval_minutes: 30,
            self_notes: false,
            twin_chats: BTreeSet::new(),
            excluded_chats: BTreeSet::new(),
            include_groups: true,
            checkin_days: 7,
            your_name: String::new(),
            voice_notes: String::new(),
        }
    }
}

impl Config {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("znga.json")
    }

    /// Reads `path`, or the defaults when it is missing or unreadable.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fastframe_file::write(path, text.into_bytes())
    }

    /// Hands the file to `files`, which writes it off the interface thread.
    pub fn save_to(&self, files: &fastframe_file::Writer, path: &Path) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        files.write(path, text, fastframe_file::Access::Default);
        Ok(())
    }

    pub fn model(&self) -> String {
        let model = self.model.trim();
        if model.is_empty() {
            self.engine.default_model().to_owned()
        } else {
            model.to_owned()
        }
    }

    pub fn endpoint(&self) -> String {
        let endpoint = self.endpoint.trim().trim_end_matches('/');
        if endpoint.is_empty() {
            self.engine.default_endpoint().to_owned()
        } else {
            endpoint.to_owned()
        }
    }

    pub fn is_cloud(&self) -> bool {
        self.engine.is_cloud(&self.endpoint())
    }

    /// Whether Znga may send requests now: on, and consented when cloud.
    pub fn may_call(&self) -> bool {
        self.enabled && (!self.is_cloud() || self.cloud_consent)
    }

    pub fn is_twin(&self, account: &str, chat: &str) -> bool {
        self.twin_chats.contains(&chat_key(account, chat))
    }

    pub fn is_excluded(&self, account: &str, chat: &str) -> bool {
        self.excluded_chats.contains(&chat_key(account, chat))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_config_is_off_and_local() {
        let config = Config::default();
        assert!(!config.enabled);
        assert!(!config.may_call());
        assert_eq!(config.engine, Engine::Local);
        assert!(!config.self_notes);
    }

    #[test]
    fn a_missing_or_broken_file_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = Config::path(dir.path());
        assert_eq!(Config::load(&path), Config::default());
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Config::load(&path), Config::default());
    }

    #[test]
    fn saved_config_round_trips_and_has_no_key_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = Config::path(dir.path());
        let mut config = Config {
            enabled: true,
            engine: Engine::Claude,
            ..Config::default()
        };
        config
            .twin_chats
            .insert(chat_key("1", "mom@s.whatsapp.net"));
        config.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.to_lowercase().contains("api_key"));
        assert_eq!(Config::load(&path), config);
        assert!(config.is_twin("1", "mom@s.whatsapp.net"));
        assert!(!config.is_twin("2", "mom@s.whatsapp.net"));
    }

    #[test]
    fn cloud_engines_need_consent_and_loopback_stays_local() {
        let mut config = Config {
            enabled: true,
            engine: Engine::Gemini,
            ..Config::default()
        };
        assert!(config.is_cloud());
        assert!(!config.may_call());
        config.cloud_consent = true;
        assert!(config.may_call());
        let local_openai = Config {
            enabled: true,
            engine: Engine::OpenAi,
            endpoint: "http://localhost:1234/v1".into(),
            ..Config::default()
        };
        assert!(!local_openai.is_cloud());
        assert!(local_openai.may_call());
    }

    #[test]
    fn empty_model_and_endpoint_use_engine_defaults() {
        let config = Config {
            engine: Engine::Claude,
            ..Config::default()
        };
        assert_eq!(config.model(), "claude-opus-5-5");
        assert_eq!(config.endpoint(), "https://api.anthropic.com");
        let custom = Config {
            endpoint: "http://box:11434/".into(),
            ..Config::default()
        };
        assert_eq!(custom.endpoint(), "http://box:11434");
    }
}
