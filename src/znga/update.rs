//! Self-update for Znga builds: releases come from this fork, signed with
//! the fork's own key, so an update never swaps Znga for upstream ZapFast.
//! Replaces `updates::CONFIG` when the `znga` feature is on.

use fastframe_update::{MacConfig, Prereleases, UpdateConfig};

/// The fork's GitHub repository, which publishes Znga releases.
pub const REPOSITORY: &str = "selvakumarEsra/zapfast";

/// Upstream's names and history, with the fork's repository and key.
/// Fork versions are pre-releases (`0.21.1-beta.1`), so they follow every
/// published fork release above them.
pub const CONFIG: UpdateConfig = UpdateConfig {
    legacy_names: &["fastsapp"],
    macos: MacConfig {
        bundle_ids: &["me.paolino.fastsapp"],
        executable_names: &[],
        legacy_bundle_names: &["FastsApp.app"],
    },
    publisher_key: Some(include_str!("update-public-key.hex")),
    additional_publisher_keys: &[],
    prereleases: Prereleases::WhenRunningPrerelease,
    ..UpdateConfig::new(REPOSITORY, "ZapFast", "zapfast", env!("CARGO_PKG_VERSION"))
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_come_from_the_fork_with_its_own_key() {
        CONFIG.validate().unwrap();
        assert_eq!(CONFIG.repository, REPOSITORY);
        assert_eq!(
            CONFIG.publisher_key.map(str::trim),
            Some(include_str!("update-public-key.hex").trim())
        );
        assert_ne!(
            CONFIG.publisher_key,
            Some(include_str!("../../assets/update-public-key.hex")),
            "never upstream's key"
        );
    }
}
