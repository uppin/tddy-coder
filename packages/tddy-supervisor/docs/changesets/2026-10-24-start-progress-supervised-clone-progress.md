# 2026-10-24 — Streaming `CloneRepository`

**Type:** Feature · `#start-progress` 6/8 ([#739](https://github.com/uppin/tddy-coder/pull/739))

New `src/clone.rs`; `SpawnPlan.output` is now `ChildOutput {Inherit, Discard, CaptureStderr}` (was
`inherit_output: bool`); `Supervisor::spawn_session_capturing_stderr` and `wait_for_session_exit`;
`SupervisorClient::clone_repository` returns an event receiver that keeps the connection alive (`Arc`
guard). `futures-util` added (developer-approved): the generated server's stream type. See
[architecture.md § Streamed clones](../architecture.md#streamed-clones).
