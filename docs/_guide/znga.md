---
title: Znga, the AI companion
description: Alerts, briefings and replies drafted in your voice, from an AI that runs from your computer.
nav_order: 0
---

{% assign v = site.fork_version | default: "0.21.1-beta.3" %}
{% assign repo = site.fork_repository | default: "selvakumarEsra/zapfast" %}
{% assign base = "https://github.com/" | append: repo | append: "/releases/download/v" | append: v %}

This build of ZapFast adds **Znga**, an opt-in AI companion. Click the ✨
on the left rail to set it up.

## Download

[Mac]({{ base }}/zapfast-v{{ v }}-macos-universal.dmg) ·
[Windows]({{ base }}/zapfast-v{{ v }}-x86_64-pc-windows-msvc.msi) ·
[Windows ARM]({{ base }}/zapfast-v{{ v }}-aarch64-pc-windows-msvc.msi) ·
[Linux]({{ base }}/zapfast-v{{ v }}-x86_64-unknown-linux-gnu.tar.gz) ·
[Linux ARM]({{ base }}/zapfast-v{{ v }}-aarch64-unknown-linux-gnu.tar.gz) ·
[Flatpak]({{ base }}/zapfast-v{{ v }}-x86_64.flatpak) ·
[All releases](https://github.com/{{ repo }}/releases)

The Mac and Windows packages are not code-signed. On a Mac, right-click
ZapFast and choose **Open** the first time. Znga builds update only from
[this fork](https://github.com/{{ repo }}/releases).

## What Znga does

- **Alerts.** Messages that need you now (urgent questions, time-sensitive
  plans, someone writing repeatedly, mentions in groups) are scored on your
  computer without a model and raise a "Znga" notification.
- **Briefings.** "What happened / what needs you", on demand or after you
  have been away. They can also be copied to your own "Message yourself"
  chat, at most 6 a day (off by default).
- **Digital twin.** Znga learns how you write and drafts replies in your
  voice for the chats you mark as Twin, suggests check-ins with close ones
  who have gone quiet, and turns "tell Mom and Alex I'm 20 min late" into
  one personal draft each.
- **Composer tools and Ask.** Suggest a reply; rewrite a message friendlier,
  shorter or more formal, or fix its spelling; translate; and ask questions
  about your recent chats.

**Znga never sends a message to anyone but you without your click.**
WhatsApp bans accounts for automated messaging, and your friends should know
that a message from you came from you.

## Engines

| Engine | What it needs | Leaves your computer? |
|---|---|---|
| Claude Code on this computer | Your Claude Code login. ZapFast runs `claude -p` and never reads its token. | Yes, to Anthropic |
| On this computer (Ollama) | `ollama pull llama3.2` | No |
| Claude API | `ANTHROPIC_API_KEY` or a key in the system keychain | Yes |
| Gemini | `GEMINI_API_KEY` or a key in the system keychain | Yes |
| OpenAI-compatible | `OPENAI_API_KEY`, or a local server such as LM Studio | Only if remote |

## Privacy

Znga is off until you turn it on. Cloud engines ask for your consent before
reading anything. Locked and excluded chats are never read, and message text
stays in memory.

## More in this build

- **Favourites** — the heart on the left rail opens your favourite chats and
  groups, the way Archived opens archived ones.
- **Insights** — the bar-chart button opens a dashboard of your WhatsApp
  activity, worked out on your computer: your trends, busiest hours, streaks,
  top chats and reply times, and people you've drifted from. It looks at when
  and with whom you talk; reading what was said (most used words and emoji)
  is optional and off until you turn it on. The History tab shows how far
  back each chat goes, and fills the gaps: it can ask your phone for older
  messages, or merge a chat export from WhatsApp into the matching chat
  without saving it.
- **Keep** — messages deleted for everyone, and the versions that edits
  replace, stay in your local archive under Insights → Deleted & edited, and
  every attachment is saved as it arrives.
