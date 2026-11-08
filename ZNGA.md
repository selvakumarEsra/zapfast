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

## Extras: Favourites, Insights, Keep

Three more fork features live in `src/extras/` (contract:
[`specs/extras.md`](specs/extras.md)), behind the `extras` feature:

- **Favourites** (heart on the rail): opens the chats marked as favourites,
  here or on the phone, the way Archived opens archived chats. Chats leads
  back to every chat.
- **Insights** (bar-chart button on the rail): a dashboard computed on this
  computer: totals and their change, a daily trend, an hour × weekday
  heatmap, streaks, top chats with reply times, who starts conversations,
  the mix of message kinds, private vs groups, and people who have gone
  quiet. These use only time, chat, direction and kind. **Words** (most used
  words and emoji, message length) reads message text only after you turn
  it on, in memory only. **Import chat export…** analyses a WhatsApp
  `.txt`/`.zip` export without saving it, either on its own or merged into
  the matching chat, where it adds only the messages the archive lacks
  (in memory, for the session). **History** shows how far back each chat
  goes (full history, with export, or partial) and can ask your own phone
  for older messages in ticked chats: one request every 15 s, at most 10
  (about 500 messages) per chat per run. Locked chats and channels are
  never included.
- **Keep**: a message deleted for everyone, and each version an edit
  replaces, is kept in the local archive (`extras_kept` table, encrypted
  with the archive) and listed under Insights → Deleted & edited. Every
  attachment downloads as it arrives (Insights → Saving, on by default), and
  **Save everything now** fetches archived attachments not yet on disk.
  Messages you delete yourself are not kept, and nothing about keeping
  reaches WhatsApp.

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
`cargo build --no-default-features` (or `--features znga` for Znga alone).

## Merging from upstream

All the code is in `src/znga/` and `src/extras/`. Upstream files carry only
small hooks, each marked `znga hook` or `extras hook`:

```
git grep -n -e "znga hook" -e "extras hook"
```

| File | Hook |
|---|---|
| `Cargo.toml` | `default = ["znga"]`, `znga = []` |
| `src/lib.rs` | `pub mod znga;` |
| `src/app.rs` | the `znga` field and its init; `pub(crate)` on `waker`, `notifications`, `notification_opens`, `deferred_account_actions`; calls in `Event::Incoming`, `background_frame`, `frame_ui` |
| `src/demo.rs` | `--demo-page znga`, `znga-drafts`, `znga-ask`, `znga-setup` |
| `src/ui/rail.rs` | extras: Chats is dark while Favourites is lit, and leads back to every chat |
| `src/archive.rs` | extras: `keep::preserve` before a revoke or edit overwrites a message; `extras_connection` |
| `src/backend/worker.rs` | extras: the tick removes kept copies the person forgot |
| `src/updates.rs` | `CONFIG` comes from `src/znga/update.rs` (the fork's releases and key) |
| `.github/workflows/release.yml` | signs with `src/znga/update-public-key.hex`; skips notarization checks without Apple credentials |
| `Cargo.toml` `version` | `<upstream>-beta.<n>` |

If a merge conflicts on one of those lines, keep upstream's change and put the
hook back. The rail button lives in its own layer, so it does not touch
`src/ui/rail.rs`.

## Releasing

Fork versions are pre-releases of the upstream version they build on:
`0.21.1-beta.1`, then `0.21.1-beta.2`, and after merging upstream 0.22.0,
`0.22.0-beta.1`. The packaging tool accepts only `-alpha.N`, `-beta.N` and
`-rc.N` pre-releases, so the fork uses `-beta.N`.

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

## Docs site

https://selvakumaresra.github.io/zapfast/ is built by
`.github/workflows/znga-docs.yml` from upstream's `docs/` plus
`docs/_guide/znga.md`, with `docs/_config.znga.yml` layered over
`docs/_config.yml`. Upstream's Docs workflow is disabled on the fork (a
repository setting, not a file change): it deploys zapfast.rocks with
upstream's analytics and IndexNow.

## Tests

```
cargo test --lib znga                                  # unit tests, offline
cargo test --lib live_ -- --ignored --nocapture        # live round-trips via your Claude Code
```
