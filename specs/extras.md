---
id: EXTRAS-DOC
title: Fork extras — Favourites, Insights, Keep
owner: fork
priority: high
---

<!-- id: EXTRAS-DOC -->
# Fork extras — Favourites, Insights, Keep

Three fork features that sit beside Znga and are built the same way:

- **Favourites** on the left rail, opening the chats marked as favourites
  the way Archived opens archived chats.
- **Insights**, a dashboard of the person's WhatsApp activity, computed on
  this computer from the local archive or from a chat export they choose.
  Reading what people said is optional.
- **Keep**, which makes sure everything stays in local files: messages the
  sender deleted for everyone, earlier versions of edited messages, and
  every attachment.

Non-goals: nothing here sends data off the computer, changes what the phone
or the other person sees, or tells anyone that a message was kept.
Messages the person deletes here themselves ("Delete for me", "Clear chat")
are their choice and are not kept.

<!-- id: REQ-EXTRAS-001 -->
## Extras MUST stay modular so upstream merges stay easy

All code lives under `src/extras/` behind the `extras` Cargo feature (on by
default in this fork). Upstream files get only small hook lines marked
`// extras hook`. Rail buttons are drawn in their own layer, so upstream's
rail widgets and their ids stay untouched. Settings live in their own file
(`extras.json`), not in upstream's settings.

implementations:
  - src/extras/mod.rs:Extras

## Acceptance
<!-- id: REQ-EXTRAS-001.A1 -->
- `cargo build --no-default-features` builds ZapFast without the extras.
<!-- id: REQ-EXTRAS-001.A2 -->
- Every change outside `src/extras/`, `specs/` and docs carries an
  `extras hook` marker.
<!-- id: REQ-EXTRAS-001.A3 -->
- Upstream's rail and demo tests pass unchanged.

<!-- id: REQ-EXTRAS-002 -->
## The rail MUST offer Favourites, which opens the favourite chats like Archived

A heart button on the rail opens the chat list showing only the chats marked
as favourites (here or on the phone), in the phone's order. Like Archived,
it is a destination rather than a toggle: it is lit while the list shows the
favourites, the Chats destination is not lit at the same time, and Chats
leads back to every chat. Opening Favourites brings a hidden chat list back,
leaves Starred, Archived and the locked folder, and clears a label filter.
The button carries the number of unread favourite chats.

implementations:
  - src/extras/favourites.rs:lit
  - src/extras/favourites.rs:actions
  - src/extras/favourites.rs:leave
  - src/extras/favourites.rs:button

verifies:
  - src/extras/favourites.rs:favourites_opens_like_archived_and_chats_leaves_it
  - src/extras/favourites.rs:only_one_destination_is_lit

## Acceptance
<!-- id: REQ-EXTRAS-002.A1 -->
- From Calls, Starred, Archived, the locked folder or a hidden list, one
  click shows the favourite chats in the list column.
<!-- id: REQ-EXTRAS-002.A2 -->
- While favourites show, the heart is lit and Chats is not; clicking Chats
  shows every chat again.
<!-- id: REQ-EXTRAS-002.A3 -->
- With no favourites, the list explains there are none yet.
<!-- id: REQ-EXTRAS-002.A4 -->
- The badge counts favourite chats with unread messages.

<!-- id: REQ-EXTRAS-003 -->
## Insights MUST show activity trends computed on this computer

A dashboard, opened from its own rail button, shows the person's messaging
activity for a chosen period (7, 30 or 90 days, a year, or everything):
totals sent and received, active chats, a daily trend, an hour × weekday
heatmap, busiest hour and day, the longest daily streak, the top chats with
their share, typical reply times (theirs and the person's), the mix of
message kinds (text, photo, video, voice, document, sticker, other), the
split between private chats and groups, and frequent contacts who have gone
quiet. These come from message metadata alone (time, chat, direction, kind):
no message text is read. Locked chats and channels are never included. The
archive is read through a separate read-only connection, off the UI thread.

implementations:
  - src/extras/insights/stats.rs:compute
  - src/extras/insights/source.rs:load_archive
  - src/extras/insights/ui.rs:show

verifies:
  - src/extras/insights/stats.rs:totals_split_sent_and_received
  - src/extras/insights/stats.rs:heatmap_and_busiest_slots
  - src/extras/insights/stats.rs:streaks_count_consecutive_days
  - src/extras/insights/stats.rs:reply_times_use_the_median
  - src/extras/insights/stats.rs:quiet_contacts_were_frequent_before
  - src/extras/insights/stats.rs:the_period_bounds_every_metric
  - src/extras/insights/source.rs:locked_and_unlisted_chats_are_never_read_and_text_needs_consent
  - src/extras/insights/mod.rs:insights_reads_the_active_accounts_archive

## Acceptance
<!-- id: REQ-EXTRAS-003.A1 -->
- Every number is computed from (time, chat, direction, kind) records; the
  default dashboard reads no message text.
<!-- id: REQ-EXTRAS-003.A2 -->
- Locked chats and channels contribute nothing.
<!-- id: REQ-EXTRAS-003.A3 -->
- A reply time is the gap from the other side's last message to the next
  message in the other direction within 12 hours; the dashboard shows the
  median.
<!-- id: REQ-EXTRAS-003.A4 -->
- A quiet contact is a private chat with at least 20 messages in the 90 days
  before the last 30, and none in the last 30 days.
<!-- id: REQ-EXTRAS-003.A5 -->
- Loading never blocks the window, and a failure to open the archive is
  shown in the dashboard, not as a crash.
<!-- id: REQ-EXTRAS-003.A6 -->
- The dashboard reads the archive of the account shown in the window, and
  switching accounts reads that account's archive the next time it opens.

<!-- id: REQ-EXTRAS-004 -->
## Content analysis MUST be optional, private and off by default

When the person turns it on, Insights also reads message text, in memory
only, to show the most used words (common words left out), the most used
emoji, and the average message length, overall and per chat. Nothing it
derives is written to disk or sent anywhere. Turning it off drops what was
derived.

implementations:
  - src/extras/insights/stats.rs:words
  - src/extras/config.rs:Config

verifies:
  - src/extras/insights/stats.rs:words_skip_common_words_and_count_emoji
  - src/extras/config.rs:content_analysis_is_off_by_default

## Acceptance
<!-- id: REQ-EXTRAS-004.A1 -->
- With the option off, no text is loaded from the archive.
<!-- id: REQ-EXTRAS-004.A2 -->
- Words of fewer than three letters and common words are not counted.

<!-- id: REQ-EXTRAS-005 -->
## Insights MAY analyse a chat export the person chooses

The person can pick a WhatsApp chat export (`.txt`, or the `.zip` WhatsApp
makes) for a chat ZapFast has no history of. It is parsed in memory and
shown as its own source with the same dashboard; nothing is written to the
archive. Both the Android (`12/31/24, 21:05 - Name: text`) and iPhone
(`[31/12/2024, 21:05:33] Name: text`) layouts are read, with multi-line
messages and system lines handled. The person says which name is theirs.

implementations:
  - src/extras/insights/export.rs:parse

verifies:
  - src/extras/insights/export.rs:android_and_iphone_layouts_parse
  - src/extras/insights/export.rs:continuation_lines_join_the_message
  - src/extras/insights/export.rs:media_placeholders_become_kinds
  - src/extras/insights/export.rs:a_file_that_is_not_an_export_is_refused

## Acceptance
<!-- id: REQ-EXTRAS-005.A1 -->
- Both layouts, 12- and 24-hour times, and day-first or month-first dates
  parse into records with the right time and sender.
<!-- id: REQ-EXTRAS-005.A2 -->
- "<Media omitted>" and attachment lines count as media, not text.
<!-- id: REQ-EXTRAS-005.A3 -->
- A file that is not a chat export is refused with a message, and nothing
  is shown for it.

<!-- id: REQ-EXTRAS-006 -->
## Keep MUST keep messages deleted for everyone

Before a message is replaced by "This message was deleted" (by its sender,
from here, or on a linked device), its last content, sender, time and raw
data are copied to a local table in the archive (encrypted whenever the
archive is). Its attachment file stays on disk. Insights lists kept
messages by chat, newest first, with the original text or attachment, and
each can be forgotten by the person.

implementations:
  - src/extras/keep.rs:preserve
  - src/extras/keep.rs:kept

verifies:
  - src/extras/keep.rs:a_revoked_message_is_kept
  - src/extras/keep.rs:the_archive_keeps_revokes_and_edits_through_its_hooks
  - src/extras/keep.rs:a_message_is_kept_once
  - src/extras/keep.rs:forgetting_removes_the_copy

## Acceptance
<!-- id: REQ-EXTRAS-006.A1 -->
- After "deleted for everyone", the original content is still readable in
  Insights → Deleted.
<!-- id: REQ-EXTRAS-006.A2 -->
- A replayed revoke does not add a second copy, nor overwrite the first
  with the "deleted" stub.
<!-- id: REQ-EXTRAS-006.A3 -->
- Nothing about keeping is sent to WhatsApp or shown to the other person.

<!-- id: REQ-EXTRAS-007 -->
## Keep MUST keep earlier versions of edited messages

Before an edit replaces a message's content, the previous content is kept
with the time it was replaced, so every version stays readable.

implementations:
  - src/extras/keep.rs:preserve

verifies:
  - src/extras/keep.rs:each_edit_keeps_the_version_before_it
  - src/extras/keep.rs:the_archive_keeps_revokes_and_edits_through_its_hooks

## Acceptance
<!-- id: REQ-EXTRAS-007.A1 -->
- Two edits keep two earlier versions, oldest first.
<!-- id: REQ-EXTRAS-007.A2 -->
- An edit that changes nothing keeps no new version.

<!-- id: REQ-EXTRAS-008 -->
## Keep MUST save every attachment to local files

With "Save all attachments" on (the fork's default), every attachment that
arrives is downloaded as it arrives, whatever its size, not only when it
scrolls into view. "Save everything now" downloads the attachments already
in the archive that are not on disk yet, a few at a time, with progress
shown. The person can turn saving off.

implementations:
  - src/extras/keep.rs:on_incoming
  - src/extras/keep.rs:Backfill

verifies:
  - src/extras/keep.rs:incoming_attachments_download_when_saving_is_on
  - src/extras/keep.rs:backfill_paces_downloads

## Acceptance
<!-- id: REQ-EXTRAS-008.A1 -->
- An incoming photo, video, voice note, document or sticker without a file
  asks for its download at once.
<!-- id: REQ-EXTRAS-008.A2 -->
- The backfill asks for at most 3 downloads per second and stops when the
  person stops it.

<!-- id: REQ-EXTRAS-009 -->
## Insights SHOULD merge a chat export into this account's numbers

The archive only holds what the phone has sent this computer, so a chat's
early history is often missing. The person can merge a chat export into
the matching chat of this account instead of viewing it on its own: the
account's dashboard then counts the export's messages that the archive
does not already have. The merge lives in memory for the session; nothing
is written to the archive, and the person can remove it.

implementations:
  - src/extras/insights/merge.rs:merge
  - src/extras/insights/merge.rs:suggest
  - src/extras/insights/mod.rs:merge_into

verifies:
  - src/extras/insights/merge.rs:an_export_adds_only_what_the_archive_lacks
  - src/extras/insights/merge.rs:merging_the_same_export_twice_adds_nothing
  - src/extras/insights/merge.rs:the_chat_with_the_exports_title_is_suggested_first

## Acceptance
<!-- id: REQ-EXTRAS-009.A1 -->
- After choosing their name, the person picks the chat the export belongs
  to, with the chat whose title matches the export's offered first, or
  keeps the export separate as before. Locked chats and channels are not
  offered.
<!-- id: REQ-EXTRAS-009.A2 -->
- Within the chosen chat, an export message matches an archived message of
  the same direction and kind in the same minute, one to one; only export
  messages left without a match are added.
<!-- id: REQ-EXTRAS-009.A3 -->
- Merging the same export again adds nothing.
<!-- id: REQ-EXTRAS-009.A4 -->
- Merged messages are never written to the archive, carry text only when
  content analysis is on, and leave the numbers when the merge is removed.

<!-- id: REQ-EXTRAS-010 -->
## Insights SHOULD fetch older history from the phone for chosen chats

The person can choose chats and ask for their older history. ZapFast asks
the person's own phone for messages before the oldest one archived, the way
scrolling to the top of a chat does, and the answers are written to the
archive like any other history, so they stay on this computer and "Save
everything now" can download their attachments. Each request wakes
the phone (an iPhone shows "Syncing with WhatsApp"), so requests are spaced
out and capped.

implementations:
  - src/extras/insights/fetch.rs:Fetch

verifies:
  - src/extras/insights/fetch.rs:requests_are_spaced_and_capped
  - src/extras/insights/fetch.rs:a_chat_at_its_start_is_skipped
  - src/extras/insights/fetch.rs:nothing_is_asked_while_offline_or_stopped

## Acceptance
<!-- id: REQ-EXTRAS-010.A1 -->
- Only the person's own phone is asked; nothing is sent to any contact or
  group, and nothing is asked while the phone is not connected.
<!-- id: REQ-EXTRAS-010.A2 -->
- At most one request every 15 seconds for the whole run, and at most 10
  requests (about 500 messages) per chat per run.
<!-- id: REQ-EXTRAS-010.A3 -->
- A chat whose phone said there is nothing older is not asked again, and
  the run ends when every chosen chat is done or the person stops it.
<!-- id: REQ-EXTRAS-010.A4 -->
- Locked chats and channels cannot be chosen.
<!-- id: REQ-EXTRAS-010.A5 -->
- The dashboard reads the archive again while the run goes and when it
  ends, so new history shows up in the numbers.

<!-- id: REQ-EXTRAS-011 -->
## Insights MUST show how far back each chat's data goes

The numbers are only as complete as the history on this computer, so the
dashboard says what it covers instead of letting a partial view pass as
the whole story.

implementations:
  - src/extras/insights/stats.rs:coverage

verifies:
  - src/extras/insights/stats.rs:coverage_tells_full_partial_and_exported_apart

## Acceptance
<!-- id: REQ-EXTRAS-011.A1 -->
- For each chat it lists the date of its oldest message, its message count,
  and whether it is "Full history" (the phone said there is nothing older),
  "With export" (a merged export reaches further back than the archive), or
  "Partial".
<!-- id: REQ-EXTRAS-011.A2 -->
- The Overview says from which date the numbers run and how many chats have
  their full history, and links to the list.
<!-- id: REQ-EXTRAS-011.A3 -->
- Coverage comes from message times and chat state only; no message text is
  read for it.
