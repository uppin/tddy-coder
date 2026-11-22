# Changeset: The `#orchestra` sweep — one orchestrator-caller check, hermetic tests, the stack's deferrals recorded

**Date**: 2026-11-22
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 2 — one engine move plus a duplicate removed, two test-hermeticity fixes, consent lines in six todo entries; no behaviour change
**Stack**: `#orchestra` 11/11 — branch `feature/orchestra/sweep`, base `feature/orchestra/session-files-cross-host` (#996)

## Initial Discovery

[2026-11-22-orchestra-sweep-initial-discovery.md](./2026-11-22-orchestra-sweep-initial-discovery.md)

## Affected Packages

- **tddy-daemon-rpc**: `orchestrator_caller` moves from `exec_tool/base_freshness.rs` to `exec_tool/project_home/caller.rs`, beside `project_home_caller`; the copy in `exec_tool/session_files.rs` is removed and its caller uses the moved one
- **tddy-tools**: `server.rs` test `a_system_prompt_for_a_remotely_routed_agent_is_carried_rather_than_refused` made hermetic against the `TDDY_REMOTE_*` environment
- **tddy-rust-typescript-tests**: `tests/echo-unary.test.ts` binds an ephemeral port instead of the fixed `8887`
- **docs/dev/todo/**: six entries the stack added gain the developer's deferral consent

## Summary

The stack's sweep node. It pays down what `#orchestra` 1–10 left behind: a role check written twice, two tests that depend on the machine they run on, and six deferred todo entries that do not yet record the developer's consent.

## Responsibility

- One `orchestrator_caller`, in the module that already holds the project-home caller checks.
- Hermetic `tddy-tools` and `echo-unary` tests.
- The deferral consent of the six todo entries.

## Boundaries

- No behaviour change: every refusal keeps its status code; the merged check keeps the wording of the copy most callers already use (`base_freshness.rs`).
- Does not fix any deferred todo — the developer deferred all six (2026-11-22).
- Does not run `/analyze-code-issues`: the developer chose to skip it for this sweep (2026-11-22, "Skip the analyzer") — the coverage builds of ~10 crates were several hours on a loaded host. Nodes n5–n8 did not run it either, so the stack's code-issue debt is **unmeasured** for those crates, not proven zero.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#orchestra` 1–9 (merged, #979–#987) | the orchestrator tools, their callers, the todo entries | moves/merges the caller check; records consents | change any tool's behaviour |
| `#orchestra` 10 (#996) | `session_files.rs` in its cross-host shape | removes its `orchestrator_caller` copy | change the forwarding |

## Draft PR contract

The first push of this PR: the green baseline of the suites covering the touched code, and the recorded refusal of the restructure engine plus the developer's consent to a hand move. There is no `plan.jsonl`: `restructure check --deep` refused the only planned operation (see Decisions & Trade-offs).

## Green wave

**Wave:** 2 (after #996)
**Greenable independently:** yes, once #996's head is its base
**Concurrent with:** none
**Blocks:** none

## Prerequisites

None.

## Scope

- [ ] `orchestrator_caller` moved **by hand** to `exec_tool/project_home/caller.rs`; `session_files.rs`'s copy removed; `lsp_roots.rs`, `path_access.rs`, `base_freshness.rs`, `session_files.rs` import it from there
- [ ] `a_system_prompt_for_a_remotely_routed_agent_is_carried_rather_than_refused` clears `TDDY_REMOTE_*` for its duration (the pattern `tests/approval_prompt_deny_paths.rs` uses)
- [ ] `echo-unary.test.ts` asks the OS for a free port
- [ ] Append this refusal (`orchestrator_caller`, #999) as a recorded instance to `docs/dev/todo/2026-11-21-restructure-ignores-path-attribute-modules.md`
- [ ] Consent recorded in: `2026-11-21-add-lsp-root-warms-by-symbol-query.md`, `2026-11-21-back-channel-for-sandboxed-and-acp-children.md`, `2026-11-21-base-freshness-tools-are-not-forwarded-to-a-peer-host.md`, `2026-11-21-cursor-muse-terminal-paste-unverified.md`, `2026-11-21-restructure-ignores-path-attribute-modules.md`, `2026-11-21-trunk-watch-reads-github-com-only.md`

## Technical Changes

### State A (Current)

`orchestrator_caller` is defined twice (`base_freshness.rs:68`, `pub(super)`, imported by `lsp_roots.rs` and `path_access.rs`; `session_files.rs:99`, private), with near-identical bodies. The `tddy-tools` test reads `TDDY_REMOTE_SESSION_ID` from the process environment, so it fails inside a tddy session. `echo-unary.test.ts` binds `8887`, colliding with any concurrent run.

### State B (Target)

One check in `project_home/caller.rs`; both tests independent of their host; every deferred entry names who deferred it and when.

## Acceptance Tests

Refactor — no new tests. Green baseline (green before and after), recorded 2026-11-22 on `f309a4604`:

- `./test -p tddy-daemon-rpc`: **692 passed / 8 failed / 1 ignored**.
  - 7 pre-existing macOS sandbox/seatbelt failures (`/bin/cat: illegal option -- -`, 7 hits in the log):
    - `sandboxed_codebase_lifecycle_exec_tool_acceptance`: `resuming_a_sandboxed_codebase_session_re_provisions_its_jail`
    - `sandboxed_codebase_placement_exec_tool_acceptance`: `a_sandboxed_codebase_sessions_tool_call_is_served_by_its_jail`, `a_sandboxed_codebase_sessions_tool_call_is_refused_when_its_jail_is_gone`
    - `sandboxed_codebase_seatbelt_acceptance`: `a_read_outside_the_checkout_is_refused_by_the_tool_engines_path_containment`, `a_shell_cannot_reach_outside_the_checkout_either`, `a_shell_from_a_jailed_codebase_session_runs_with_the_checkout_as_its_cwd`, `a_write_from_a_jailed_codebase_session_lands_inside_the_checkout`
  - 1 load-induced: `orchestrator_role_exec_tools_acceptance::a_session_started_by_an_orchestrator_records_it_before_its_agent_runs` timed out under host load ~80 in the baseline (10 s PTY wait, "the PTY has produced no output at all"); passed alone and its whole binary passed 8/8 on re-run, and it passed on #996 CI. Not a pre-existing failure.
- `a_system_prompt_for_a_remotely_routed_agent_is_carried_rather_than_refused` (`tddy-tools --lib`):
  - green with `TDDY_REMOTE_SESSION_ID` unset (1 passed);
  - **red with `TDDY_REMOTE_SESSION_ID=x`** (the defect this node fixes, the target): `server.rs:2834`: "the override must reach the daemon that runs the agent; got: {"error":"agent 'Gemma@ws-01' is routed by daemon 'ws-01': OpenAgentConversation: NOT_FOUND: session 'x' has no readable metadata at /Users/mantasi/.tddy/sessions/x: artifact write failed: No such file or directory (os error 2)","is_error":true}"
- `echo-unary.test.ts` in `tddy-rust-typescript-tests`: not run in this baseline.

After: all of the above green except the 7 sandbox failures; the tddy-tools test also green with `TDDY_REMOTE_SESSION_ID=x`.

## Decisions & Trade-offs

- Analyzer skipped by the developer (see Boundaries).
- **The engine refused the planned move.** `move_item` of `tddy_daemon_rpc::exec_tool::base_freshness::orchestrator_caller` into `tddy_daemon_rpc::exec_tool::project_home::caller` (`reexport: none`) was refused by `restructure check --deep`: "0: plan is malformed: `mod project_stacks_unit_tests;` leads to neither `project_stacks_unit_tests.rs` nor `project_stacks_unit_tests/mod.rs` in packages/tddy-daemon-rpc/src/pr_stack/project_stacks" (`pr_stack/project_stacks.rs:317-319` declares it with `#[path = "project_stacks_unit_tests.rs"]`). Known engine gap: `docs/dev/todo/2026-11-21-restructure-ignores-path-attribute-modules.md`.
- **Developer consent to a hand move, 2026-11-22 ("Hand move, with consent").** The implementation moves `orchestrator_caller` by hand into `exec_tool/project_home/caller.rs`, re-points `base_freshness.rs`, `lsp_roots.rs`, `path_access.rs` and `session_files.rs` at it, and deletes the private copy in `session_files.rs`. Every hand step is noted here; no TODO in code, since nothing is left unfinished. The moved fn's visibility may need `pub(in crate::exec_tool)` (the module itself is `pub(in crate::exec_tool) mod caller;`).
- The six deferrals were decided at the stack's TODO gate (developer, 2026-11-22: "Defer all three" for the second group; the first group's other rows deferred with it).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-22-orchestra-sweep-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [x] Green baseline and recorded engine refusal (no `plan.jsonl`)
- [x] USER REVIEW — plan and baseline (developer chose the hand move, 2026-11-22)
- [ ] Restructure (/code-restructuring)
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
