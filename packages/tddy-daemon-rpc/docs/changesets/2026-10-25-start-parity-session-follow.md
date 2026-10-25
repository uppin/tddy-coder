# 2026-10-25 — Following a session, read on the host

**Type:** Feature

`#start-parity` 6/9 ([#755](https://github.com/uppin/tddy-coder/pull/755)). `exec_tool/project_home_session_follow.rs`: the
`DaemonProjectHome` port methods' reads — the session's whole `agent-activity.jsonl` and
`pending-question.json`, and the transcript frames `GetAcpReplayPage` pages (as
`TranscriptItem`s, by position) and one tool call's bodies (`GetAcpToolCallDetail`), through
`tddy_service::acp_replay`. A call naming another host is forwarded whole by the existing target
route.

Tests: `tests/session_follow_acceptance.rs` (two hosts).
