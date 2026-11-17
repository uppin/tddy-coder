# 2026-11-16 — A started CLI-harness session records Started at spawn

**Type:** Fix · PR [#933](https://github.com/uppin/tddy-coder/pull/933)

Both CLI-harness starts — the host start and the jailed start — write
`activity_status: Started` into the first `.session.yaml`, generically across agents. A
Cursor launch never fires `sessionStart` (every launch resumes its chat), so without this
the session read no status until its first prompt's `beforeSubmitPrompt`; a Claude start's
`SessionStart` hook re-records the same a moment later. Daemon-level resume is unchanged.

Measurement noted at wrap: `start_cli_harness_session` 368 → 378 (worse by 10), recorded in
its complexity record; restructuring stays with the `#agent-tools` sweep as deferred.
