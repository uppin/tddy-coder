# 2026-10-24 — subagent_resume hands a permission request to the user

**Type:** Feature

`#coagents` 8/8 ([#727](https://github.com/uppin/tddy-coder/pull/727)). `permission_deferral_arg`: `deferPermissionToUser` on `subagent_resume` (refused together with `permission`); `take_a_turn` marks the pending turn `awaitingUser`; `subagent_await`'s description says so. `subagent_cancel` sends no actor — the daemon derives it from the token.

Code issues: `oversized-file-server` 2,970 → 2,984, `complexity-server-take-a-turn` 80 → 84, new `complexity-server-subagent-resume-tool` (59 → 67), deferred to the sweep node. Feature: [managed-codebase subagents](../../../../docs/ft/coder/managed-codebase-subagents.md#a-coding-subagents-permission-request-is-the-callers-to-answer).
