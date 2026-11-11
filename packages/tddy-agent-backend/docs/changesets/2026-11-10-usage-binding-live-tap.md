# 2026-11-10 — tddy-agent-backend: the usage tap

`usage_tap` (`UsageTapSpec::for_conversation`, `UsageTap`): the headless driver offers every stdout line and the ACP client every `session/update` of a tapped session; matching events are appended verbatim to `<session_dir>/usage/<conversation_id>.jsonl`. `HeadlessCommand::usage_tap`, `OpenSession::usage_tap`. The stderr reader moved to `backend/headless_driver/stderr.rs`. `#usage-binding 3/5`, PR [#892](https://github.com/uppin/tddy-coder/pull/892). See [session-token-accounting.md](../../../../docs/ft/coder/session-token-accounting.md).
