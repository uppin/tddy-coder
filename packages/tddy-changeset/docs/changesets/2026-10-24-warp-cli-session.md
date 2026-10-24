# 2026-10-24 — SessionMetadata.harness_resume_token

**Type:** Feature

`#warp-agent` 1/2, PR [#717](https://github.com/uppin/tddy-coder/pull/717). Cross-package entry:
[2026-10-24-warp-cli-session.md](../../../../docs/dev/changesets/2026-10-24-warp-cli-session.md).

`SessionMetadata.harness_resume_token: Option<String>` (serde default, omitted when `None`): the handle a
CLI harness printed on exit for resuming its conversation (Warp's `warp --resume <TOKEN>`). See
[architecture § Harness resume token](../architecture.md#harness-resume-token-session_metadata). Tests:
`a_session_without_a_resume_token_writes_no_resume_token_field`,
`a_recorded_resume_token_round_trips_through_session_yaml`.
