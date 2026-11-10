# 2026-11-03 — Cursor CLI from cursor.yaml

**Type:** Feature

`#agent-tools` 7/12 ([#796](https://github.com/uppin/tddy-coder/pull/796)).

- `relays_to_the_host`: the in-jail tool-IPC socket also relays `tddy_service::session_activity::IN_JAIL_RELAYABLE_HOOK_REPORTS` (`ReportSessionStatus`, `ReportAgentActivity`) to the facilitating daemon; `AwaitQuestionAnswer` stays unrelayed.
