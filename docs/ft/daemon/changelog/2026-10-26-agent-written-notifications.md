# 2026-10-26 — Agents notify their operator in their own words

An agent in a daemon-launched claude-cli or cursor-cli session can tell its operator what finished
or what is blocked with `tddy-tools notify "<title>" --body "<markdown>" --level
info|success|warning|error [--sound]`. The daemon publishes it as an attention notification: the
drawer dot turns yellow, and Telegram shows the title in bold, the session label, the markdown body
rendered (bold, italic, code, code blocks and links; everything else escaped), and a link to the
session — split into several messages when it is longer than Telegram allows, and sent silently
without `--sound`. A title over 200 characters or a body over 2,000 is refused with a reason rather
than truncated, and the command exits non-zero whenever the notification was not sent. Two identical
notifications are both delivered. claude-cli agents are told about the command in their system
prompt. A session with a standing role (the project-home main) and a sandboxed session cannot notify
yet ([#766](https://github.com/uppin/tddy-coder/pull/766)). See
[Session notifications § Agent-written notifications](../session-notifications.md#agent-written-notifications).
