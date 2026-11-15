# Initial discovery — the `#split-alive` sweep

**Date**: 2026-11-15 · **Changeset**: [2026-11-15-split-alive-sweep.md](./2026-11-15-split-alive-sweep.md)

## Backlog collection

The merge-base with `origin/master` is `305f99b86`. The tail is `origin/feature/split-alive/split-pairing`
at `2e53ab061`.

```bash
mb=305f99b86; T=origin/feature/split-alive/split-pairing
git diff --name-status $mb $T -- docs/dev/todo/ 'packages/*/docs/code-issues/'
```

- **TODOs.** `2026-08-29-a-cursor-cli-session-s-orchestrator-link-never-reaches-sessionentry.md` was
  deleted (resolved by #908). `2026-11-15-split-agent-exit-cause-unknown.md` was added.
- **Code-issue records touched:** `tddy-pty` `complexity-runtime-spawn-child-and-pump` (67 → 67)
  and `oversized-file-runtime` (523 → 523) were re-measured with no change.
  `tddy-session-split` `complexity-svc-spawn-split-agent-spawn-split-agent` went 133 → 152 → 157.

## Function-length scan

Each touched non-test `.rs` file was scanned at `$mb` and at `$T`. Production code is counted up to
the first `#[cfg(test)]`. Functions are measured by brace matching, from the `fn` line to its closing
brace. Over 60 lines:

| File | Function | Base → tip |
|---|---|---|
| `tddy-session-split/src/svc_spawn_split_agent.rs` | `spawn_split_agent` | 133 → 157 **worse** |
| `tddy-session-activity/src/session_list_enrichment.rs` | `session_list_status_from_session_dir` | 129 → 133 **worse** |
| `tddy-session-split/src/agent_argv.rs` | `split_claude_extra_args` | 63 → 67 **worse** |
| `tddy-cli-sessions/src/cli_session_manager/pty_spawn.rs` | `spawn_tool_on` | 75 → 75 |
| `tddy-coding-agent-specs/src/spec.rs` | `validate` | 68 → 68 |
| `tddy-daemon-rpc/src/exec_tool/project_home_remote.rs` | `serve_project_home_for_caller_held_elsewhere` | 96 → 96 |
| `tddy-pty/src/runtime.rs` | `run`, `spawn_child_and_pump` | 101 → 101, 67 → 67 |
| `tddy-tool-engine/src/project_home_lifecycle_tools.rs` | `project_home_lifecycle_tool_defs` | 81 → 81 |

`push_identity` in `project_home_tools.rs` also matched (61). It is a trait method declaration with no
body, so the brace-matching scan ran past it into the next items. It is a false positive.

## File-length scan

No touched production file is at or over 500 lines. The largest are `spec.rs` (433 → 461),
`lib.rs` in tool-engine (475 → 476) and `project_home_lifecycle_tools.rs` (479 → 480). The test files
`remote_managed_worktree_cross_host_acceptance.rs` (1210 → 1347) and
`project_home_remote_targets_acceptance.rs` (589 → 725) are long. No budget record covers test files,
and no `[W]` deferred a split of either, so both are out of scope.

## Seams read

- **`spawn_split_agent`** reads as four phases: prepare, then launch arguments, then spawn and
  liveness, then response. The process spawn already goes through `SplitAgentProcess`. Its parameters
  are `self` plus eight, and five of the eight are per-start identity that every helper takes again.
  Hence the `SplitAgentStart` struct.
- **`session_list_status_from_session_dir`** is three early-exit arms, each building a full
  `SessionListStatusDisplay` literal. The literals are what make it long.
- **`split_claude_extra_args`** is variant args, then withdrawals, then slots render. Extracting the
  first and the denied-tools computation puts it under budget.
