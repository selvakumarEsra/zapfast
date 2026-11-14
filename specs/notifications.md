---
id: NOTIFY-DOC
title: Message notifications
owner: fork
priority: medium
---

<!-- id: NOTIFY-DOC -->
# Message notifications

How a desktop notification for an incoming message is titled and worded.

<!-- id: REQ-NOTIFY-001 -->
## A group notification MUST name the sender in its title

For a group message the title is `<group name> · <sender name>` and the body
is the message summary alone. Before, the title ended with the account
owner's name when several accounts were linked, and the sender was only a
prefix in the body. Direct-chat notifications are unchanged: the title is
the chat name, and it ends with `· <account>` when several accounts are
linked.

implementations:
  - src/notify.rs:lines

verifies:
  - src/notify.rs:a_group_names_the_sender_and_a_chat_does_not

## Acceptance
<!-- id: REQ-NOTIFY-001.A1 -->
- A group message from "Mira" in "Rust Berlin" is titled "Rust Berlin · Mira" with body "Save me a seat", whether or not several accounts are linked.
<!-- id: REQ-NOTIFY-001.A2 -->
- A direct-chat message with several accounts linked is still titled "<chat> · <account>".
<!-- id: REQ-NOTIFY-001.A3 -->
- Locked / content-hidden notifications are unaffected (they name neither chat nor sender).
