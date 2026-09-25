# 2026-09-25 — against the warm index, `check --deep` hangs on a module file the previous `apply` created

**Category:** Future enhancement (warm-index defect; a run never answers, and every later request to
that root queues behind it)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), port-move pilot (T3),
plans `07b-refuse-unready-clone-to-module.jsonl` then `07c-clone-readiness-to-session-agents.jsonl`
in `docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/`

## What happened

1. `07b` (`extract_module`, `to_file: true`) was applied through this worktree's warm
   `tddy-index-daemon`. It created
   `packages/tddy-session-lifecycle/src/connection_service/svc_start_hosted_agent_clone/clone_readiness.rs`
   and answered in 12.4s.
2. `.restructure/` was cleared and `07c` written: one `move_module_to_crate` whose symbol anchor is
   that new file's module.
3. `07c`'s plain `check` answered in 1ms (`no findings`). Its `check --deep`, sent to the same
   daemon, **never answered**. The client printed no progress line after
   `restructure: running against the warm index daemon at …`, and after 20 minutes both
   `tddy-index-daemon` and its `rust-analyzer` were at 0.0% CPU. A second `check --deep` queued
   behind it:

```text
17:10:34.157 [INFO] … check for `…/feature-carve-lifecycle-split-1`: answered (+1ms)     # plain
17:10:35.396 [INFO] … check arrived for `…/feature-carve-lifecycle-split-1`, which is already warm   # --deep: no answer
17:31:57.712 [INFO] … check arrived for `…/feature-carve-lifecycle-split-1`, which is already warm   # queued
```

The **same plan, cold** (`TDDY_INDEX_SOCKET` unset), answered in the ordinary time:

```text
op 0 survey: …/svc_start_hosted_agent_clone/clone_readiness.rs -> tddy-session-agents (tddy_session_agents), 1 item(s) reached from outside, 1 caller(s)
   reached from outside: refuse_unready_clone
   …/svc_start_hosted_agent_clone.rs: clone_readiness::refuse_unready_clone -> tddy_session_agents::clone_readiness::refuse_unready_clone
no findings
```

The daemon was stopped and restarted to go on.

## Likely cause (not confirmed)

The warm server was never told the file exists. The backend closes every document it opened when an
operation ends, so the server reads the tree from disk again. That covers a file it already knew and
that changed. A file **created** by an assist's `CreateFile` edit is new to its VFS, and without a
`workspace/didChangeWatchedFiles` notification (or an open) the server may keep a module tree in which
`mod clone_readiness;` names no file. Then the anchor never resolves, and a wait that is "until the
server is ready or until you stop it" waits indefinitely. A cold run loads the tree fresh, file
included.

## What would fix it

- After applying an edit that creates or deletes a file, send `workspace/didChangeWatchedFiles`
  (`Created` / `Deleted`) for each such path, whether the run is cold or warm. For the warm daemon
  this is the difference between the next request working and hanging.
- Bound the anchor wait on a loaded index the way the inactive-code wait is bounded: once the crate
  index is ready, an anchor whose file is not in the server's module tree is a refusal naming the
  file, not a wait.
