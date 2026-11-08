//! API keys for Znga's cloud engines: the provider's standard environment
//! variable first, then Znga's own OS-keyring entry. Never a file.

use super::config::Engine;

const SERVICE: &str = "rocks.zapfast.ZapFast.znga";

/// Why a pasted value cannot be used as a key for `engine`.
pub fn reject(engine: Engine, key: &str) -> Option<&'static str> {
    let key = key.trim();
    if key.is_empty() {
        return Some("Paste an API key first.");
    }
    if engine == Engine::Claude && key.starts_with("sk-ant-oat") {
        return Some(
            "That is a Claude Code login token, not an API key. Choose the \
             \"Claude Code on this computer\" engine to use your Claude Code \
             login, or create an API key at console.anthropic.com.",
        );
    }
    if engine == Engine::Claude && !key.starts_with("sk-ant-") {
        return Some("Claude API keys start with \"sk-ant-\".");
    }
    None
}

/// The key to use for `engine`, if any is set.
pub fn resolve_key(engine: Engine) -> Option<String> {
    resolve_with(engine, |name| std::env::var(name).ok(), || stored(engine))
}

fn resolve_with(
    engine: Engine,
    env: impl Fn(&str) -> Option<String>,
    keyring: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if !engine.needs_key() {
        return None;
    }
    let (_, vars) = engine.key_sources();
    vars.iter()
        .filter_map(|name| env(name))
        .map(|value| value.trim().to_owned())
        .find(|value| !value.is_empty())
        .or_else(keyring)
        .filter(|key| reject(engine, key).is_none())
}

/// Where the key for `engine` would come from, for the setup page.
pub fn source(engine: Engine) -> Option<String> {
    let (_, vars) = engine.key_sources();
    if let Some(name) = vars
        .iter()
        .find(|name| std::env::var(name).is_ok_and(|v| !v.trim().is_empty()))
    {
        return Some(format!("from ${name}"));
    }
    stored(engine).map(|_| "saved in the system keychain".to_owned())
}

/// The OS keyring, as `archive::encryption` opens it (kept here so this
/// fork-only module does not widen upstream's API).
#[cfg(not(test))]
fn platform_store() -> Option<std::sync::Arc<dyn keyring_core::api::CredentialStoreApi>> {
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new();
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new();
    Some(store.ok()? as std::sync::Arc<dyn keyring_core::api::CredentialStoreApi>)
}

#[cfg(not(test))]
fn entry(engine: Engine) -> Option<keyring_core::Entry> {
    platform_store()?
        .build(SERVICE, engine.key_sources().0, None)
        .ok()
}

#[cfg(not(test))]
fn stored(engine: Engine) -> Option<String> {
    entry(engine)?.get_password().ok()
}

#[cfg(test)]
fn stored(_engine: Engine) -> Option<String> {
    None
}

/// Saves `key` in the OS keyring. Tests never touch the real keyring.
pub fn store(engine: Engine, key: &str) -> Result<(), String> {
    if let Some(problem) = reject(engine, key) {
        return Err(problem.to_owned());
    }
    #[cfg(not(test))]
    {
        let entry = entry(engine).ok_or("The system keychain is not available.")?;
        entry
            .set_password(key.trim())
            .map_err(|_| "The system keychain refused the key.".to_owned())
    }
    #[cfg(test)]
    {
        let _ = SERVICE;
        Ok(())
    }
}

pub fn forget(engine: Engine) {
    #[cfg(not(test))]
    if let Some(entry) = entry(engine) {
        let _ = entry.delete_credential();
    }
    #[cfg(test)]
    let _ = engine;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claude_code_token_is_not_a_claude_api_key() {
        assert!(reject(Engine::Claude, "sk-ant-oat01-abc").is_some());
        assert!(reject(Engine::Claude, "sk-ant-api03-abc").is_none());
        assert!(reject(Engine::Claude, "AIza123").is_some());
        assert!(reject(Engine::Gemini, "AIza123").is_none());
        assert!(reject(Engine::Gemini, "  ").is_some());
    }

    #[test]
    fn the_environment_wins_over_the_keyring_and_empty_values_are_skipped() {
        let env = |name: &str| match name {
            "GEMINI_API_KEY" => Some(" ".into()),
            "GOOGLE_API_KEY" => Some("AIza-env".into()),
            _ => None,
        };
        assert_eq!(
            resolve_with(Engine::Gemini, env, || Some("AIza-ring".into())),
            Some("AIza-env".into())
        );
        assert_eq!(
            resolve_with(Engine::Gemini, |_| None, || Some("AIza-ring".into())),
            Some("AIza-ring".into())
        );
    }

    #[test]
    fn local_engines_need_no_key_and_bad_keys_resolve_to_none() {
        assert_eq!(
            resolve_with(Engine::Local, |_| Some("x".into()), || None),
            None
        );
        assert_eq!(
            resolve_with(Engine::ClaudeCode, |_| Some("x".into()), || None),
            None
        );
        assert_eq!(
            resolve_with(Engine::Claude, |_| Some("sk-ant-oat01-x".into()), || None),
            None
        );
    }
}
