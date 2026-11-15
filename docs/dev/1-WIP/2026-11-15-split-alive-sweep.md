# Changeset: refactor — the `#split-alive` sweep pays down the stack's code issues

**Date**: 2026-11-15
**Status**: 🚧 In Progress — [G] applied; [W] pending
**Type**: Refactor
**Complexity**: 3/5 — two crates (`tddy-session-split`, `tddy-session-activity`), three functions that the stack grew, nine `extract_method` operations, and one hand-written parameter struct with its two call sites
**Stack**: `#split-alive` 5/5, the sweep node. Base: `feature/split-alive/split-pairing` (#908)

## Initial Discovery

[2026-11-15-split-alive-sweep-initial-discovery.md](./2026-11-15-split-alive-sweep-initial-discovery.md)
holds the backlog collection, the measurement commands and the seams that were read.

## Related Feature Documentation

None. Every operation preserves behaviour, so there is no PRD.

## Summary

This node closes every code issue that `#split-alive` (#905 → #908) added or made worse. Three
functions grew in the stack. `spawn_split_agent` already had an open record, and the stack grew it
133 → 157 lines. `session_list_status_from_session_dir` (129 → 133) and `split_claude_extra_args`
(63 → 67) were already over the 60-line budget and had no record. All three are brought within
budget: 60 lines, nesting 4 and 5 parameters. No caller outside `tddy-session-split` changes.

## Background

`/pr-wrap` did not decompose `spawn_split_agent` in #905 or #908, because #906–#908 also modify
`svc_spawn_split_agent.rs`, `split_session.rs` and `agent_argv.rs`. Every node is now green, so
nothing downstream shares those files and the split is safe.

## Dependencies

- **Inherited:** #905 (n1), #906 (n2), #907 (n3) and #908 (n4). This node implements no surface of
  theirs. It only extracts and regroups what they left.
- **Engine:** `tddy-tools restructure` at this base.

## Prerequisites — the deferred-work cross-check (Step 2b)

| Record / finding | Verdict |
|---|---|
| `packages/tddy-session-split/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md` (157 lines, 9 params) | **fix here** (B1) |
| `session_list_status_from_session_dir` 129 → 133, no record | **fix here** (B2) |
| `split_claude_extra_args` 63 → 67, no record | **fix here** (B3) |
| `packages/tddy-pty/docs/code-issues/complexity-runtime-spawn-child-and-pump.md` (67 → 67) | out of scope: touched by #905 but not worse |
| `packages/tddy-pty/docs/code-issues/oversized-file-runtime.md` (523 → 523) | out of scope: touched by #905 but not worse |
| `spawn_tool_on` 75, `validate` 68, `serve_project_home_for_caller_held_elsewhere` 96, `runtime::run` 101, `project_home_lifecycle_tool_defs` 81 | out of scope: in touched files, unchanged by the stack |
| `docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md` item 3 (`spawn_split_agent`) | **closed here** with B1. The item is removed from the entry |
| the same entry's item 1 (fix 4, why the agent exits) | the TODO gate: it is an investigation, and no restructure closes it |

No touched production file is at or over 500 lines; the largest is `spec.rs` at 461. No record in
the touched packages carries `Claimed by`.

## Scope

- [x] **Baseline**: CI on #908's head `2e53ab061` (read before the first plan): `Rust tests` 12773 run, 12773 passed, 0 failed; `Rust e2e tests` 653/653; web shards 788 + 381 + 606 all passed; no failing test by name. Rust shards 1-3 were still pending when read, so the figures are the aggregate report's
- [x] **Plan**: anchors emitted with `restructure anchors` from the warm index and `check --deep` clean. **Deferred to `[G]`**: this commit carries intents only
- [x] **Hand edit H1**: the `SplitAgentStart` parameter struct and its two call sites, in a commit of its own
- [x] **Apply**: each group applied and committed on its own, with `verify --against` after each
- [x] **Baseline after**: the scoped suites match the recorded set by name (`./test -p tddy-session-split -p tddy-session-activity`: 28 targets, 291 passed, 0 failed, 0 ignored; clippy `-D warnings` and `cargo fmt --check` clean)
- [x] **Code quality**: `cargo fmt --check`; `./dev cargo clippy -p tddy-session-split -p tddy-session-activity --all-targets -- -D warnings`
- [ ] **Re-measure**: `/analyze-code-issues` adds no record and closes every backlog row. B1's record is deleted, and its final numbers go in the change-history entry
- [ ] **Documentation**: the Final Checklist executed

## Backlog — each item, and the intent that closes it

The plan is [`2026-11-15-split-alive-sweep-plans/plan.jsonl`](./2026-11-15-split-alive-sweep-plans/plan.jsonl)
(nine operations, ids `b1-x1` … `b3-x2`). **It is intents only.** Its anchors name the containing
item by path, without ranges, fingerprints or a header. For each operation `[G]` emits the range with
`restructure anchors --at L:C-L:C` over the statements named below, writes the header with
`restructure snapshot` and proves it with `check --deep`. No range may hold an early `return`. A `?`
is fine.

### B1 — `spawn_split_agent`: 157 lines, 9 parameters → ≤ 60 lines, 5 parameters

`packages/tddy-session-split/src/svc_spawn_split_agent.rs:52`, an inherent method of `SplitSessions`.

- **H1 (hand, first commit).** Add
  `pub(crate) struct SplitAgentStart<'a> { os_user, session_id, sessions_base, codebase_instance_id, codebase_session_id }`
  beside `SplitAgentProcess`. The signature becomes `(&self, start: &SplitAgentStart<'_>, room, req, progress)`,
  which is 5 parameters counting `self`, and the `#[allow(clippy::too_many_arguments)]` goes. Two callers change:
  `split_claude_cli_start.rs:171` and `svc_start_sandboxed_codebase_session.rs:100`. The body binds
  the five fields to locals of the same names on its first line, so the extracts below see the names
  they see today. The engine has no parameter-object operation (`plan-schema.md` § Signature and
  call-site operations covers only types, order and arguments). That is why H1 is written by hand.
- `b1-x1` → `prepare_split_launch`: from `let session_dir = …` through
  `self.record_first_changeset(…).await?;`. It returns `(session_dir, initial_prompt, tddy_tools_path, remote)`.
- `b1-x2` → `split_agent_launch_args`: from `let roster = …` through the
  `split_agent_context_and_args(…).await?` binding. It returns `(context_dir, extra_args)`.
- `b1-x3` → `ensure_split_agent_alive`: from `let recording = …` through the second
  `record_main_capture(…)`. It takes `&handle`.
- `b1-x4` → `split_start_response`: from `let placement = …` to the closing `}))` of the `Ok(Response…)`,
  as a non-`async` associated function.

The body that remains is the five bindings, four calls, the process spawn and the metadata write, at
about 50 lines. Closed when the function is at most 60 lines, nesting at most 4 and 5 parameters.
Delete the record.

### B2 — `session_list_status_from_session_dir`: 133 → ≤ 60

`packages/tddy-session-activity/src/session_list_enrichment.rs`. It has three `SessionListStatusDisplay`
literals. Each is extracted into a free function and its call site becomes one line:

- `b2-x1` → `cli_harness_row(meta, session_type, session_dir)`: the CLI arm, from
  `let changeset = read_changeset(session_dir).ok();` to the literal's end. The range stops before
  the `return Ok(` wrapper.
- `b2-x2` → `state_only_row(session_dir, &changeset, workflow_state, elapsed_display)`: the
  "not found in changeset sessions" literal.
- `b2-x3` → `workflow_row(session_dir, &changeset, row-derived fields …)`: the final literal, from
  `let workflow_goal = …` through the `log::debug!`.

### B3 — `split_claude_extra_args`: 67 → ≤ 60

`packages/tddy-session-split/src/agent_argv.rs`.

- `b3-x1` → `host_variant_args(spec, session_dir)`: the `variant_args` call and the
  `variant_hooks_args` extend.
- `b3-x2` → `split_denied_tools(spec, &withdrawn_refs)`: the `natives` lookup and the
  `build_claude_disallowlist` filter.

## Execution record ([G])

Commits after `252a33292`, in order:

| Commit | What | Engine or hand |
|---|---|---|
| H1 | `SplitAgentStart` struct, `#[allow(too_many_arguments)]` removed, two callers | hand |
| b1-x1..x4 | `prepare_split_launch`, `split_agent_launch_args`, `ensure_split_agent_alive`, `split_start_response` (one group, applied bottom-up) | engine |
| b1 fix-ups | `prepare_split_launch` 8 params -> `(&self, start, room, req, progress)` (the engine's own note: `clippy::too_many_arguments` fires above 7); the roster comment moves into `split_agent_launch_args`; `split_start_response(room, start, handle)` | hand |
| b3 prep | `natives` lookup moved beside the denylist that filters by it | hand |
| b3-x1, b3-x2 | `host_variant_args`, `split_denied_tools` | engine |
| b2-x1 | `cli_harness_row` (the literal expression; `return Ok(` stays in the caller, so the range cannot hold the `let changeset`) | engine |
| b2-x3 | `workflow_row(session_dir, &changeset, row, workflow_state, elapsed_display)` | hand. Engine refusal at apply: `session_list_enrichment.rs:239:37: error[E0505]: cannot move out of \`changeset\` because it is borrowed` (the assist moved `changeset` while `row` borrowed it); `check --deep` had reported no findings |
| b2-x2 | `state_only_row` | hand. Engine refusal at apply: `session_list_enrichment.rs:236:9: error: expected identifier, found \`*\`` (shorthand fields `workflow_state`/`elapsed_display` rewritten to `*workflow_state,`); `check --deep` had reported no findings |
| b2 fix-up | `read_or_log` and `runs_no_workflow` helpers, because the three row extracts alone left the function at 77 lines | hand |

`restructure verify --against HEAD` after each engine group reports only the statements whose
arguments changed shape by extraction (`&session_dir` -> `session_dir`, `&withdrawn_refs` -> parameter),
which it cannot excuse; nothing else was lost.

### Re-measured (fn line to closing brace; nesting; parameters)

| Function | Before | After | Nesting | Params |
|---|---|---|---|---|
| `spawn_split_agent` | 157 lines, 9 params | 60 lines | 2 | 5 (with `self`) |
| `session_list_status_from_session_dir` | 133 | 57 | 2 | 2 |
| `split_claude_extra_args` | 67 | 51 | 1 | 5 |

## Green wave

Wave 1 after every other node: n1–n4 are delivered (green), so nothing blocks it.

## Final Checklist

- [ ] B1's record deleted, with its final row (lines, nesting, params) in the change-history entry
- [ ] `docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md` item 3 removed
- [ ] `packages/tddy-session-split/docs/module-layout.md` names the new helpers if it lists the file's functions
- [ ] Changelog and changeset history entries, one file each
