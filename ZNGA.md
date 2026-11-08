# Znga — ZapFast's AI companion (fork feature)

Znga is an opt-in AI layer for this fork of ZapFast. Click the ✨ on the left
rail (under Calls) to open it. The contract is [`specs/znga.md`](specs/znga.md).

## What it does

| Tab | What you get |
|---|---|
| **Today** | **Needs you**: alerts for messages that need you now (questions, urgency, time-sensitive plans, someone writing repeatedly, mentions in groups). These are scored on your machine without a model. **Briefings**: "what happened / what needs you", on demand (**Brief me**) or automatically after you have been away. |
| **Drafts** | **Twin** replies written in your voice, check-ins with close ones who have gone quiet, **Update your people** ("tell Mom and Alex I'm 20 min late" → one draft each), and composer tools: Suggest reply, Friendlier / Shorter / More formal / Fix spelling, Translate. |
| **Ask** | Questions about your recent chats: "Do I owe anyone a reply?" |
| **Setup** | Engine, privacy, proactive settings, and which chats are **Twin** (your close ones) or **Excluded**. |

Proactive notifications are titled "Znga". You can also have briefings copied
to your own "Message yourself" chat so they reach your phone. That option is
off by default and capped at 6 a day.

## Engines

| Engine | Credentials | Leaves your machine? |
|---|---|---|
| **Claude Code on this computer** (preselected when `claude` is installed) | Your existing Claude Code login, or `CLAUDE_CODE_OAUTH_TOKEN` in its environment. ZapFast runs `claude -p` with tools, MCP servers, settings and history all off, and never reads or stores the token. | Yes, to Anthropic |
| On this computer (Ollama) | None. Run `ollama pull llama3.2`. | No |
| Claude API | `ANTHROPIC_API_KEY` or a key saved in the system keychain | Yes |
| Gemini | `GEMINI_API_KEY` / `GOOGLE_API_KEY` or keychain | Yes |
| OpenAI-compatible | `OPENAI_API_KEY` or keychain. Point the address at LM Studio or llama.cpp to stay local. | Only if remote |

Cloud engines ask for your consent before reading anything. Locked chats and
excluded chats are never read. Message text is kept in memory only. The style
profile on disk (`znga-style.json`) holds numbers, never text.

## The one rule

Znga **never sends a message to anyone but you without your click**. WhatsApp
bans accounts for automated messaging, and your friends should know that a
message from you really came from you. The twin makes replying a one-click
job; it does not reply for you.

## Building

The feature is on by default (`znga` in `Cargo.toml`). To build without it:
`cargo build --no-default-features`.

## Merging from upstream

All the code is in `src/znga/`. Upstream files carry only small hooks, each
marked `znga hook`:

```
git grep -n "znga hook"
```

| File | Hook |
|---|---|
| `Cargo.toml` | `default = ["znga"]`, `znga = []` |
| `src/lib.rs` | `pub mod znga;` |
| `src/app.rs` | the `znga` field and its init; `pub(crate)` on `waker`, `notifications`, `notification_opens`, `deferred_account_actions`; calls in `Event::Incoming`, `background_frame`, `frame_ui` |
| `src/demo.rs` | `--demo-page znga`, `znga-drafts`, `znga-ask`, `znga-setup` |
| `src/updates.rs` | `CONFIG` comes from `src/znga/update.rs` (the fork's releases and key) |
| `.github/workflows/release.yml` | signs with `src/znga/update-public-key.hex`; skips notarization checks without Apple credentials |
| `Cargo.toml` `version` | `<upstream>-znga.<n>` |

If a merge conflicts on one of those lines, keep upstream's change and put the
hook back. The rail button lives in its own layer, so it does not touch
`src/ui/rail.rs`.

## Releasing

Fork versions are pre-releases of the upstream version they build on:
`0.21.1-znga.1`, then `0.21.1-znga.2`, and after merging upstream 0.22.0,
`0.22.0-znga.1`.

1. Set `version` in `Cargo.toml`, run `cargo build` (updates `Cargo.lock`),
   and write `packaging/release-notes/v<version>.md`.
2. Commit on `main`, push, then `git tag v<version> && git push origin v<version>`.
3. The Release workflow builds Linux, Windows, macOS and Flatpak, signs
   `checksums.txt` with the fork's key (the `ZAPFAST_UPDATE_SIGNING_KEY`
   secret of the `release-signing` environment), and creates a **draft
   pre-release**. Publish it on GitHub so installed copies see the update.

Znga builds update only from this fork and trust only the fork's key
(`src/znga/update-public-key.hex`). The private key is kept outside the
checkout. Back it up: installed copies cannot move to a new key. Without
Apple or Windows signing credentials the DMG and MSI are unsigned (on a
Mac, right-click **Open** the first time). Homebrew, winget and AUR
publishing are upstream's and never run for fork tags.

## Tests

```
cargo test --lib znga                                  # unit tests, offline
cargo test --lib live_ -- --ignored --nocapture        # live round-trips via your Claude Code
```
