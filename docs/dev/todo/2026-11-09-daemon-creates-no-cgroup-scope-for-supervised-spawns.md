# 2026-11-09 — The daemon creates no cgroup scope for a supervised spawn

**Category:** Gap (daemon / supervisor cgroups)
**Source:** `#agent-tools` 32/32 (`supervisor-agent-agnostic`, #865), found at its `/validate-changes`.
**Consent:** Deferred by the user, 2026-11-09 (#865 wrap): #865 ships supervisor-side scope support for relayed and streamed spawns; the daemon creating and destroying session scopes is a follow-up node.

## What is there

`tddy-supervisor` places a child in a named cgroup scope before it execs, for every spawn kind:
`SpawnSession`, `SpawnSandbox`, `SpawnRelayed` and `SpawnStreamed` each take a `scope`, and the
supervisor's own tests pin it (`supervisor_relay.rs::a_relayed_spawn_runs_in_a_cgroup_scope`,
`supervisor_streamed_spawn.rs::a_streamed_spawn_runs_in_a_cgroup_scope`). The scope must already
exist (`CreateScope`); naming one that does not is a `NotFound` refusal.

The daemon never calls `CreateScope`, so it names no scope anywhere:

- `tddy-spawn/src/supervisor_spawn.rs::spawn_session_request` — `scope: None`;
- `tddy-spawn/src/supervisor_spawn.rs::plan_supervised_clone` — `scope: None`;
- `tddy-session-lifecycle/src/coding_agent_launcher.rs` — `start_relayed(…, None)`, marked
  `TODO(daemon-creates-no-cgroup-scope-for-supervised-spawns)`. The green phase of #865 first passed the session id here,
  which would have refused every relayed start, since no scope of that name exists.

So on a supervised host, sessions, relayed coding agents and clones all run unscoped: no memory, cpu
or pids ceiling applies to them.

## What would close it

The daemon creates one scope per session (`CreateScope { name: <session id>, limits }`, the limits
from the daemon's YAML clamped to the supervisor's ceilings), passes its name on the session's own
spawn, every relayed agent of that session and its clone, and destroys it when the session ends
(`DestroyScope`). A daemon-side test with a fake supervisor pins that each request names the scope it
created, and that a session start whose `CreateScope` fails is refused rather than run unscoped.
