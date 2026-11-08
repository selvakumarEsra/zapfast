---
id: ZNGA-DOC
title: Znga — the on-device AI companion for ZapFast
owner: selvakumarEsra
priority: high
version: 1
---

<!-- id: ZNGA-DOC -->
# Znga — the on-device AI companion for ZapFast

Znga is an opt-in AI layer inside ZapFast. It reads the chats the person
already sees, tells them what happened and what needs them, and writes in
their voice so keeping up with friends and family takes one click. It is
built as a fork-only, self-contained module (`src/znga/`, Cargo feature
`znga`) so upstream ZapFast merges stay conflict-light.

**Product stance (decided, PM review).** Znga is a *proactive assistant
and a drafting twin*, not an autonomous bot:

- It acts **for the person, never instead of them** toward other people.
  Everything addressed to a contact is a draft the person approves with a
  click. WhatsApp bans accounts for automated sending, and friends deserve
  to know a message really came from the person. This keeps every account
  safe while still making the twin feel instant.
- It is **local-first**: the default engine is a model on this machine
  (Ollama-compatible endpoint). Cloud engines (Claude, Gemini, OpenAI) are
  bring-your-own-key and clearly disclosed.
- It is **off until switched on**, and it never sees locked or excluded
  chats.

**Non-goals.** No autonomous sending to other people, no auto-replies,
no bulk or scheduled messages, no MCP/bot/API surface for other programs
(all of which WhatsApp bans accounts for), no hosted Znga backend, no
storage of message text outside ZapFast's encrypted archive, and no
handling of Claude Code login tokens by ZapFast itself.

This fork deliberately relaxes upstream's "features never send message
content to a third party" boundary **only** when the person picks a cloud
engine, with an explicit disclosure; with the local engine nothing leaves
the machine.

<!-- id: REQ-ZNGA-001 -->
## Znga MUST be opt-in, disclosed, and isolated from upstream state

Znga is disabled by default. Its configuration lives in its own file
(`znga.json` in ZapFast's config folder), never inside ZapFast's
`Settings` or `AccountSettings`. API keys live in the OS keyring (or in
the provider's standard environment variable), never in a file.

implementations:
  - src/znga/config.rs:Config
  - src/znga/config.rs:Config.load
  - src/znga/keys.rs:resolve_key

verifies:
  - src/znga/config.rs:a_fresh_config_is_off_and_local
  - src/znga/config.rs:a_missing_or_broken_file_loads_defaults
  - src/znga/config.rs:saved_config_round_trips_and_has_no_key_field
  - src/znga/config.rs:cloud_engines_need_consent_and_loopback_stays_local

## Acceptance
<!-- id: REQ-ZNGA-001.A1 -->
- A fresh install has Znga disabled; no model request, notification, or
  draft is produced until the person turns it on.
<!-- id: REQ-ZNGA-001.A2 -->
- A missing or unreadable `znga.json` loads defaults without touching
  ZapFast's own settings file.
<!-- id: REQ-ZNGA-001.A3 -->
- Saved configuration never contains an API key.
<!-- id: REQ-ZNGA-001.A4 -->
- Choosing a cloud engine shows, before first use, that recent message
  text from non-excluded chats will be sent to that provider.

<!-- id: REQ-ZNGA-002 -->
## Znga MUST support a local engine and bring-your-own-key cloud engines

Engines: **Local** (Ollama-compatible `/api/chat`, default
`http://127.0.0.1:11434`), **Claude Code on this computer** (the person's
installed `claude` CLI run headless with no tools, MCP servers, settings
or session history; it signs in by itself, so ZapFast never reads or
stores the token; preselected on first run when the CLI is found),
**Claude** (Anthropic Messages API, API key),
**Gemini** (Google Generative Language API, API key), and **OpenAI-
compatible** (chat completions; also covers LM Studio / llama.cpp
servers via a custom base URL). Claude Code subscription tokens and
ChatGPT/Codex logins are not API credentials and are rejected with an
explanation.

implementations:
  - src/znga/config.rs:Engine
  - src/znga/provider.rs:complete
  - src/znga/provider.rs:build_request
  - src/znga/provider.rs:parse_response
  - src/znga/runner.rs:Runner

verifies:
  - src/znga/provider.rs:claude_requests_follow_the_messages_api
  - src/znga/provider.rs:claude_replies_join_text_blocks_and_refusals_are_errors
  - src/znga/provider.rs:gemini_requests_and_replies
  - src/znga/provider.rs:openai_and_local_requests_and_replies
  - src/znga/provider.rs:claude_code_runs_without_tools_settings_or_history
  - src/znga/provider.rs:claude_code_results_and_errors
  - src/znga/keys.rs:a_claude_code_token_is_not_a_claude_api_key

## Acceptance
<!-- id: REQ-ZNGA-002.A1 -->
- Each engine builds a request with its documented endpoint, auth header,
  and JSON shape, and extracts the reply text from its documented response.
<!-- id: REQ-ZNGA-002.A2 -->
- A Claude reply whose `stop_reason` is `refusal`, or any HTTP error,
  surfaces as a readable error in the Znga panel, not as a draft.
<!-- id: REQ-ZNGA-002.A3 -->
- Model calls run off the interface thread; the window never blocks on
  the network.
<!-- id: REQ-ZNGA-002.A4 -->
- A value that looks like a Claude Code OAuth token (`sk-ant-oat…`) is
  refused as a Claude key with a message pointing to API keys.
<!-- id: REQ-ZNGA-002.A5 -->
- "Test connection" reports success or the provider's error.
<!-- id: REQ-ZNGA-002.A6 -->
- The Claude Code engine is found on `PATH` or in the usual install
  folders (a Dock-launched app has no shell `PATH`), runs in an empty
  working folder, and counts as a cloud engine for consent.

<!-- id: REQ-ZNGA-003 -->
## Znga MUST never see locked or excluded chats

implementations:
  - src/znga/privacy.rs:visible

verifies:
  - src/znga/privacy.rs:locked_excluded_and_broadcast_chats_are_never_seen
  - src/znga/privacy.rs:groups_follow_the_switch

## Acceptance
<!-- id: REQ-ZNGA-003.A1 -->
- Messages from a chat hidden by chat lock are never recorded or sent.
<!-- id: REQ-ZNGA-003.A2 -->
- A chat the person excluded in Znga setup is never recorded or sent.
<!-- id: REQ-ZNGA-003.A3 -->
- While the app lock is on, Znga produces no notification content.
<!-- id: REQ-ZNGA-003.A4 -->
- Znga logs never contain message text, names, or phone numbers.

<!-- id: REQ-ZNGA-004 -->
## Znga SHOULD flag messages that need the person right away

Every incoming message from a visible chat is scored locally (no model
needed): direct questions, urgency words, being mentioned, someone
writing repeatedly in a short time, and time-sensitive plans. A message
over the threshold becomes an **alert** in the Znga inbox and, while the
person is away from the window, a desktop notification titled "Znga".

implementations:
  - src/znga/triage.rs:score
  - src/znga/proactive.rs:Activity.observe

verifies:
  - src/znga/triage.rs:urgent_questions_alert_and_small_talk_does_not
  - src/znga/triage.rs:bursts_raise_the_score
  - src/znga/triage.rs:muted_chats_and_our_own_messages_never_alert
  - src/znga/proactive.rs:an_urgent_message_alerts_once_per_ten_minutes_per_chat

## Acceptance
<!-- id: REQ-ZNGA-004.A1 -->
- "URGENT can you call me?" in a one-to-one chat scores as an alert;
  "ok 👍" does not.
<!-- id: REQ-ZNGA-004.A2 -->
- Three or more messages from the same sender within five minutes raise
  the score.
<!-- id: REQ-ZNGA-004.A3 -->
- Muted chats and our own messages never raise alerts.
<!-- id: REQ-ZNGA-004.A4 -->
- At most one Znga alert notification per chat per ten minutes.

<!-- id: REQ-ZNGA-005 -->
## Znga SHOULD brief the person on what happened and what to do

A **briefing** summarises activity since the last one: who wrote, what
was decided, and a short "to do" list. Briefings are generated on demand
("Brief me") and proactively when the person has been away and enough
has happened (default: ≥ 5 messages and ≥ 30 minutes since the last
briefing). A briefing made while away also raises a "Znga" notification.

implementations:
  - src/znga/proactive.rs:Activity.due
  - src/znga/prompt.rs:briefing

verifies:
  - src/znga/proactive.rs:briefings_wait_for_absence_volume_and_interval
  - src/znga/prompt.rs:briefings_group_by_chat_and_mark_text_as_data
  - src/znga/prompt.rs:briefings_are_capped

## Acceptance
<!-- id: REQ-ZNGA-005.A1 -->
- No proactive briefing is generated while the person is reading the
  window, or before the thresholds are met.
<!-- id: REQ-ZNGA-005.A2 -->
- The briefing prompt includes only activity since the previous briefing,
  grouped by chat, capped in size.
<!-- id: REQ-ZNGA-005.A3 -->
- "Brief me" works any time there is activity, regardless of thresholds.

<!-- id: REQ-ZNGA-006 -->
## Znga MAY copy briefings to the person's own "Message yourself" chat

Off by default. When on, a proactive briefing is also sent to the
person's own chat so it reaches their phone. It never targets anyone
else, and at most six such notes are sent per day.

implementations:
  - src/znga/proactive.rs:SelfNotes

verifies:
  - src/znga/proactive.rs:self_notes_are_off_by_default_and_capped_per_day
  - src/znga/proactive.rs:our_own_chat_drops_the_device_suffix
  - src/znga/mod.rs:proactive_briefings_notify_and_copy_to_self_only_when_enabled

## Acceptance
<!-- id: REQ-ZNGA-006.A1 -->
- With the switch off, nothing is ever sent by Znga.
<!-- id: REQ-ZNGA-006.A2 -->
- The seventh note in a day is not sent.
<!-- id: REQ-ZNGA-006.A3 -->
- The destination is always our own number's chat.

<!-- id: REQ-ZNGA-007 -->
## The digital twin MUST write in the person's voice and MUST wait for approval

Znga learns a **style profile** from the person's own sent messages
(length, casing, emoji, punctuation, sign-offs, plus in-memory examples)
and drafts replies that sound like them. Chats marked **Twin** get a draft
prepared automatically when a message arrives. Drafts wait in the
**Drafts** queue with Send, Edit (to composer) and Discard.

implementations:
  - src/znga/style.rs:StyleProfile
  - src/znga/prompt.rs:twin_reply
  - src/znga/drafts.rs:Drafts

verifies:
  - src/znga/style.rs:a_lowercase_no_stop_emoji_writer_is_described_that_way
  - src/znga/style.rs:the_profile_serializes_without_message_text
  - src/znga/drafts.rs:a_newer_auto_reply_replaces_the_pending_one
  - src/znga/mod.rs:answers_become_drafts_and_never_send

## Acceptance
<!-- id: REQ-ZNGA-007.A1 -->
- The style profile reflects measured habits (e.g. a person who writes
  lowercase without full stops is described that way).
<!-- id: REQ-ZNGA-007.A2 -->
- Nothing in the Drafts queue is sent until the person clicks Send.
<!-- id: REQ-ZNGA-007.A3 -->
- A newer incoming message in the same chat replaces that chat's pending
  auto-draft rather than stacking.
<!-- id: REQ-ZNGA-007.A4 -->
- Example messages used for style are kept in memory only; the persisted
  profile holds statistics, never message text.

<!-- id: REQ-ZNGA-008 -->
## The twin SHOULD help the person update and stay close to their people

**Update:** the person writes one instruction ("tell Mom and Alex I'll be
20 minutes late") and Znga produces one draft per named person in their
voice, matched to chats locally. **Check-ins:** for Twin chats with no
activity for N days (default 7), Znga suggests a warm check-in draft,
once per chat per period.

implementations:
  - src/znga/prompt.rs:fan_out
  - src/znga/drafts.rs:match_chat
  - src/znga/proactive.rs:checkins_due

verifies:
  - src/znga/drafts.rs:names_match_chats_without_guessing
  - src/znga/drafts.rs:send_all_takes_only_the_remaining_updates
  - src/znga/proactive.rs:check_ins_come_once_per_period_for_quiet_twin_chats
  - src/znga/mod.rs:fan_out_drafts_only_matched_people

## Acceptance
<!-- id: REQ-ZNGA-008.A1 -->
- Names in the model's answer are matched to the person's chats by
  name, case- and accent-insensitively; unmatched names are reported, not
  guessed.
<!-- id: REQ-ZNGA-008.A2 -->
- "Send all" sends only the drafts still in the queue, each once.
<!-- id: REQ-ZNGA-008.A3 -->
- A check-in is not suggested twice for the same chat within its period.

<!-- id: REQ-ZNGA-009 -->
## Znga SHOULD help in the composer and answer questions about chats

For the open chat: **Suggest reply**, **Rewrite** the composer text
(friendlier / shorter / more formal), and **Translate** it. **Ask Znga**
answers free-form questions using the recent activity it has seen.

implementations:
  - src/znga/prompt.rs:rewrite
  - src/znga/prompt.rs:ask
  - src/znga/ui.rs:show

verifies:
  - src/znga/prompt.rs:ask_keeps_the_thread_and_ends_with_the_question

## Acceptance
<!-- id: REQ-ZNGA-009.A1 -->
- A rewrite result replaces the composer text only when the person clicks
  "Use".
<!-- id: REQ-ZNGA-009.A2 -->
- With no open chat, the composer tools are disabled and say why.

<!-- id: REQ-ZNGA-010 -->
## Znga MUST stay modular so upstream merges stay easy

All Znga code lives under `src/znga/` behind the `znga` Cargo feature
(on by default in this fork). Upstream files get only small hook lines
marked `// znga hook`. Znga's rail button is drawn in its own layer,
positioned from where the rail drew Calls, so upstream's rail code and
its widget ids stay untouched.

implementations:
  - src/znga/mod.rs:Znga
  - src/znga/hooks.rs:observe_incoming
  - src/znga/hooks.rs:tick
  - src/znga/ui.rs:launcher

## Acceptance
<!-- id: REQ-ZNGA-010.A1 -->
- `cargo build --no-default-features` builds ZapFast without Znga.
<!-- id: REQ-ZNGA-010.A2 -->
- Every change outside `src/znga/`, `specs/` and docs carries a
  `znga hook` marker.

<!-- id: REQ-ZNGA-011 -->
## Znga builds MUST update only from the fork

A build with Znga checks this fork's releases for updates, never
upstream's, so an update cannot replace Znga with upstream ZapFast. Fork
releases are signed with the fork's own update key, and a build trusts
only that key. Fork versions are pre-releases of the upstream version
they are based on (`0.21.1-znga.1`), so a fork build moves to any newer
published fork release.

implementations:
  - src/znga/update.rs:CONFIG

verifies:
  - src/znga/update.rs:updates_come_from_the_fork_with_its_own_key

## Acceptance
<!-- id: REQ-ZNGA-011.A1 -->
- The update repository is `selvakumarEsra/zapfast`.
<!-- id: REQ-ZNGA-011.A2 -->
- The trusted publisher key is the fork's, and upstream's key is not trusted.
<!-- id: REQ-ZNGA-011.A3 -->
- A release whose signature was made with any other key is refused.
