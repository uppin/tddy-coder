# 2026-11-11 — A workflow spawn without `--agent` records no usage context

**Category:** Deferred from `#usage-binding 2/5` (#891)
**Source:** `docs/dev/changesets/2026-11-10-usage-binding-daemon-stream.md`; `packages/tddy-spawn/src/spawner.rs`
(`plan_session_child`, the `TODO(usage-binding)` beside the `write_usage_context` call)

A workflow session is spawned as `tddy-coder --daemon`. When the start request names no agent, the
spawn passes no `--agent`, and the child runs its own default (`claude`, `run.rs` `run_daemon`).
`plan_session_child` only records `usage-context.json` when it was given an agent, so such a session
streams only its tddy subagent rows on the Usage tab, although its agent's transcript exists.

**Why deferred:** the fix belongs in the daemon, not in `tddy-spawn`. Either the daemon resolves the
default agent before it spawns (so `req.agent` is never empty by the time `tool_session_spawn.rs`
builds `SpawnOptions`), or the child records the context once it has resolved its agent. Hard-coding
`claude` in `tddy-spawn` would make the spawn crate name an agent, which the coding-agent spec work
removed everywhere else. The web's start form always sends an agent, so the gap is reachable only
from a client that leaves it blank (Telegram's workflow spawn is the known one).

**What would close it:** a spawn without `--agent` records the default agent's spec id, with a test in
`packages/tddy-spawn/tests/usage_context_spawn.rs` replacing
`a_workflow_spawn_without_an_agent_records_no_usage_context`.
