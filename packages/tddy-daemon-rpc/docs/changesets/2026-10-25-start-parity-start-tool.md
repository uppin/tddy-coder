# 2026-10-25 — Project-home StartSession over the host's start path

**Type:** Feature

`#start-parity` 2/9 ([#751](https://github.com/uppin/tddy-coder/pull/751)).
`DaemonProjectHome::start_session` builds the request with `StartSessionArgs::into_request`, carrying
the home session and this daemon as the orchestrator; `tag_orchestrator` removed (the creating daemon
records it). A branch conflict is reported naming the branch and its owner. The catalogue a
project-home session is offered carries `start_choices`. `tests/support/mod.rs` holds the two-host
in-process world (with its own repo git identity). Tests: `tests/start_session_tool_acceptance.rs`.
