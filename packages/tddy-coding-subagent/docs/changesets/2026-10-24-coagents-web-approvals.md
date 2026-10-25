# 2026-10-24 — A permission request handed to the user, answered by either side once

**Type:** Feature

`#coagents` 8/8 ([#727](https://github.com/uppin/tddy-coder/pull/727)). New `user_permission_answers` (`PermissionAnswering` for `ParkedPermissions`; `AnsweredBy` and the trait re-exported from `tddy-discovery`) and private `permission_records` (the pending and answered `ToolCallRecord`s). `ParkedPermissions`: `answer_recording(answer, by)` (first answer wins, a later one refused naming who answered), `defer_to_user`, `answered`; `answer` is the main agent's answer. `CodingAgentSession`: a main-agent answer is delivered before the in-flight slot is taken and refused at once when a deferred turn holds it; a deferral holds the turn in `settle` and streams its records; `permission_answerer()`.

Code issues: `oversized-file-session` 575 → 699; new `complexity-session-take-turn` (39 → 71 lines, depth 5). Both deferred to the sweep node. See [architecture](../architecture.md).
