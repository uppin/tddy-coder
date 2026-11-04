# Changeset: One launch path for every CLI agent; the claude and cursor dispatch is deleted

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Refactor (planned deletions)
**Complexity**: 4 — more than one crate (`tddy-agent-launch`, `tddy-worktree-service`, `tddy-cli-sessions`, `tddy-session-lifecycle` suites); ordering of tmux re-attach after a daemon restart; ~5,000 lines of bespoke claude/cursor launch removed with their suites re-pointed. Not 5: #820 built every replacement; this node deletes and re-points
**Stack:** `#agent-tools 17/19` — branch `feature/agent-tools/cli-dispatch-collapse`, base `feature/agent-tools/spec-backend` (#822)
**PRD:** none of its own. Serves the registry PRD on `feature/agent-tools/agent-registry` (`docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`): no agent is selected by id

## Initial Discovery

[2026-11-04-agent-tools-cli-dispatch-collapse-initial-discovery.md](./2026-11-04-agent-tools-cli-dispatch-collapse-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-claude-and-cursor-launch-by-agent-id.md](../todo/2026-11-04-claude-and-cursor-launch-by-agent-id.md) | ✅ **RESOLVED HERE.** `CliLauncher`, `launcher_of` and both agent-id constants go; branch intent comes from the spec |
| [2026-11-02-claude-cli-hand-built-launch-removal.md](../todo/2026-11-02-claude-cli-hand-built-launch-removal.md) | ✅ **RESOLVED HERE.** `spawn_hand_built`, `start_claude_in_pty`, the hand-built resume arm, `start_claude_in_tmux`, `relaunch_claude_cli`, `reattach_recorded_claude` |
| [2026-11-03-cursor-hand-built-launch-paths.md](../todo/2026-11-03-cursor-hand-built-launch-paths.md) | ◐ **PARTLY RESOLVED HERE.** `prepare_cursor_cli_launch` and `spawn_cursor_cli_process` go. The record's claim that `install_cursor_hooks_in_worktree` and `CliSessionManager::start_cursor` have no caller is **stale** — Telegram calls both; the record is narrowed to them and `feature/agent-tools/telegram-spec-launch` (n18) closes it |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`, `complexity-claude-cli-spawn-spawn-claude-cli-session-inner.md`, `complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner.md`, `complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md`, `complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session.md`, `crap-svc-start-sandboxed-cursor-cli-session.md`, `oversized-file-svc-start-sandboxed-cursor-cli-session.md`, `oversized-file-cursor-cli-spawn.md`, `complexity-claude-jail-launch-jailed-claude.md` | ⚠ During. The functions these records measure are deleted here; at wrap, delete each record whose code is gone (recording the final measurement), narrow the start-session-core one |
| `packages/tddy-worktree-service/docs/code-issues/complexity-branch-intent-resolve-branch-workflow.md` | — Reference. Only the constructors change |

## Affected Packages

- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): the dispatch, the bespoke claude/cursor spawns, the hand-built arms, the claude tmux paths, the conversation and pr-stack child spawns, re-attach
- [ ] [`tddy-worktree-service`](../../../packages/tddy-worktree-service/README.md): `BranchIntentPolicy::{claude_cli, cursor_cli}` deleted
- [ ] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): `build_claude_argv`/`with_claude_launch_flags` if no caller remains (Telegram's `start` is n18's)
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): the 20 claude/cursor suites re-pointed; the agent-id source scan

## Responsibility

- Delete the `CliLauncher {Claude, Cursor, Generic}` / `launcher_of` dispatch and `CLAUDE_AGENT_ID` (`harness_from_spec.rs:115`, `spec_launch.rs:389`) / `CURSOR_AGENT_ID` (`harness_from_spec.rs:118`, `cursor_from_spec.rs:18`). `cli_launcher` returns `Option<CliSessionType>`; start (`svc_start_session_core.rs:385`), resume (`svc_resume_session.rs:58`) and re-attach take #820's generic path for every agent.
- Delete the hand-built spawns: `spawn_hand_built` and `start_claude_in_pty` (`claude_cli_spawn_steps.rs:414-428,487`), the hand-built resume arm (`svc_resume_claude_cli_session.rs:193-247`), dead `spawn_cursor_cli_process` / `prepare_cursor_cli_launch` (`cursor_cli_spawn.rs:508,551`), and the `unwrap_or("claude-cli")` re-attach fallback (`claude_cli_tmux.rs:294`) — a record without a session type is skipped with a warning, never re-attached as claude.
- **No coding agents → `FailedPrecondition`**, on start, resume and re-attach.
- **Branch intent from the spec**: every caller uses `BranchIntentPolicy::cli_agent(st, spec.has(BranchNameRequired))`; `claude_cli()` and `cursor_cli()` are deleted (behaviour-identical for the built-ins: claude declares `branch_name_required`, cursor does not).
- **The conversation and pr-stack child spawns stop hard-coding claude** (`conversation_spawn_handler.rs:47`, `child_spawn_handler.rs:89`): the child's agent is the parent session's agent (or the request's), resolved through the registry.
- **Re-attach reads `HostCodingAgents`**, not `<tddy_data_dir>/tools` (`recorded_cli_launcher` :161, `recorded_resume_token_adapter` :283, `require_sandbox_placement` :298), closing the `TODO(agent-tools-cascade)` at :283.
- A **source scan finds no agent-id literal** (`"claude"`, `"cursor"`, `"claude-cli"`, `"cursor-cli"`) compared or dispatched on in `tddy-agent-launch` (extends n12's `no_agent_name_gates.rs`).

## Boundaries

- Does **not** touch Telegram (`feature/agent-tools/telegram-spec-launch`, n18). Therefore it keeps, because Telegram still calls them: `install_cursor_hooks_in_worktree`, `mint_cursor_chat_id` (as a function behind #820's adapter), `CliSessionManager::start`, `start_cursor` and the argv builders they use.
- Keeps `CliSessionManager::start_with_options` while `tddy-session-split/src/svc_spawn_split_agent.rs:262` calls it (split host agent: [2026-11-03-split-sessions-render-the-embedded-agent-spec.md](../todo/2026-11-03-split-sessions-render-the-embedded-agent-spec.md)).
- Does not touch `config.claude_cli`/`cursor_cli` readers or `daemon_config_from_fixture_file` callers beyond deleting code that reads them (`feature/agent-tools/agent-config-removal`, n19).
- Adds no launch feature: if a claude/cursor suite fails because the generic path lacks something, that is a gap in #820, raised there.
- No file splits; code-issue records only.

**Stack-wide rules this node keeps:**
- No production fallback without coding agents: `FailedPrecondition`; no default session type on re-attach.
- The agent process sees no `TDDY_*` variable (as #820's generic path renders).
- No hangs: re-attach probes (`agent_tmux_session_is_alive`) stay bounded.
- Fixtures install coding agents and the sandbox RPC bridge; Linux jail tests self-skip without user namespaces.
- **Deletions not listed in Removed need the developer's approval.**

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n14` cli-spec-features (#820) | the generic CLI launch with managed workflow, hooks, slots, role, notify, ssh, participant metadata, conversation id, tmux via `plan_host_pty`, jail; the two deadlines | claude and cursor start, resume and re-attach through it | add a slot, capability or step to it |
| `n13` route-support (#819) | spec-keyed routes | inherited through #820 | — |
| `n15`, `n16` (#821, #822) | (line predecessors only) | nothing — no real edge | — |
| `n12` agent-registry (#801) | `Capability::BranchNameRequired`, `no_agent_name_gates.rs` | branch intent; the scan is extended | change n12's scan rules for other crates |

## Draft PR contract

The first push of this PR publishes: `cli_launcher`'s new signature, `reattach_recorded_main(manager, &HostCodingAgents, os_user, session_id)`, the extended source scan, and the failing tests below. No deletion lands in the contract commit; deletions are green's work.

## Green wave

**Wave:** 3 of 5
**Greenable independently:** after #820 is green — every claude/cursor suite now exercises #820's generic path
**Concurrent with:** `feature/agent-tools/spec-backend` (#822)
**Blocks:** `feature/agent-tools/telegram-spec-launch` (n18: it routes Telegram through the one launch path this node leaves)

Real dependency edges, as opposed to the branch line:

    n12 → n13      n13 → n14, n15, n16      n15 → n16      n14 → n17      n17 → n18      n16, n18 → n19

## State A

- `CliLauncher` dispatch at `svc_start_session_core.rs:385` (3 arms), `svc_resume_session.rs:58` (3), `claude_cli_tmux.rs:299-317` and `cli_harness_tmux.rs:299-300` (re-attach); 9 matches in all.
- Bespoke claude/cursor launch: 13 files, ~5,000 lines (`claude_cli_spawn{,_steps}.rs`, `claude_cli_tmux.rs`, `claude_jail.rs`, `svc_{start,resume}{,_sandboxed}_claude_cli_session.rs`, `cursor_cli_spawn.rs`, `cursor_from_spec.rs`, `svc_start_sandboxed_cursor_cli_session.rs`, `resume.rs`, `chat.rs`).
- Hand-built arms reached when `coding_agents` is `None` (`claude_cli_spawn_steps.rs:414`, `svc_resume_claude_cli_session.rs:230`).
- `BranchIntentPolicy::claude_cli()` at `claude_cli_spawn.rs:148`, `svc_start_sandboxed_claude_cli_session.rs:172`; `cursor_cli()` at `cursor_cli_spawn.rs:193`, `svc_start_sandboxed_cursor_cli_session.rs:146`.
- `conversation_spawn_handler.rs:47`, `child_spawn_handler.rs:89` spawn a claude child.
- Re-attach loads specs from `<tddy_data_dir>/tools` (`harness_from_spec.rs:161,283,298`).

## State B

- One CLI launch path; no agent id is compared in `tddy-agent-launch`.
- A host without coding agents refuses every CLI start, resume and re-attach.
- After a daemon restart, a tmux session of any CLI agent re-attaches from the installed coding agents.

## Delta

### New
- `packages/tddy-session-lifecycle/tests/no_agent_id_dispatch.rs` (or a new case in `no_agent_name_gates.rs`)
### Modified
- `tddy-agent-launch`: `harness_from_spec.rs`, `svc_start_session_core.rs`, `svc_resume_session.rs`, `cli_harness_tmux.rs`, `cli_branch_starts.rs`, `conversation_spawn_handler.rs`, `child_spawn_handler.rs`, `opencode_cli_spawn.rs`, `svc_start_sandboxed_cli_harness_session.rs`, `project_home_start_spec.rs`, `start_request_checks.rs:103`, `jail_session_files.rs:70,107`, `svc_relaunch_sandboxed_runner.rs:124`
- `tddy-worktree-service/src/branch_intent.rs`
- The 20 session-lifecycle suites listed in the discovery §1.4, re-pointed (assertions unchanged unless a deletion removes what they pin)
- Docs at wrap: `docs/ft/daemon/claude-cli-session.md`, `cursor-cli-session.md`, `session-agent-roster.md`, `packages/tddy-agent-launch/docs/module-layout.md`
### Removed
- `CliLauncher`, `launcher_of`, `recorded_cli_launcher`, `recorded_resume_token_adapter`
- `CLAUDE_AGENT_ID` (`harness_from_spec.rs`, `spec_launch.rs`), `CURSOR_AGENT_ID` (`harness_from_spec.rs`, `cursor_from_spec.rs`)
- `spawn_hand_built`, `start_claude_in_pty`, the hand-built resume arm of `resume_claude_cli_session`, `install_claude_cli_hooks`
- `spawn_cursor_cli_process`, `prepare_cursor_cli_launch` (and their `#[allow(dead_code)]` / `TODO(cursor-cli)`)
- `start_claude_in_tmux`, `relaunch_claude_cli`, `reattach_recorded_claude`, the `unwrap_or("claude-cli")` default
- `launch_claude_cli`, `start_claude_cli_from_request`, `spawn_claude_cli_session_inner`, `spawn_cursor_cli_session_inner` and `…_reporting`, `spawn_cursor_cli_from_request`, `start_sandboxed_claude_cli_from_request`, `start_sandboxed_cursor_cli_from_request`, `resume_claude_cli_session`, `resume_cursor_cli`, `launch_jailed_claude`, `launch_jailed_cursor` — each once its last caller is the deleted dispatch (the files emptied by this are deleted too: **list them in this section at red before deleting**)
- `BranchIntentPolicy::{claude_cli, cursor_cli}` (and `cli_harness` if it has no caller)
- `CliSessionManager::{build_claude_argv, with_claude_launch_flags}` only if no caller remains after Telegram's is accounted for (n18)
- The `TODO(agent-registry)` at `harness_from_spec.rs:115-120` and the `TODO(agent-tools-cascade)` at :283

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_session_acceptance.rs`**

- [ ] `a_host_without_coding_agents_refuses_a_claude_cli_and_a_cursor_cli_start_failed_precondition_spawning_nothing`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs`**

- [ ] `removing_branch_name_required_from_an_override_of_claude_yaml_lets_a_blank_new_branch_name_generate_one`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_tmux_reattach_acceptance.rs`**

- [ ] `a_daemon_restart_reattaches_a_tmux_claude_cli_session_from_the_installed_coding_agents`
- [ ] `a_recorded_session_without_a_session_type_is_not_reattached`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_tmux_session_acceptance.rs`**

- [ ] `a_claude_cli_start_in_tmux_runs_the_argv_the_claude_spec_renders_and_resumes_the_same_way`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/stack_child_spawn_agent_acceptance.rs`**

- [ ] `a_pr_stack_child_of_a_cursor_cli_session_starts_a_cursor_cli_child`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/no_agent_name_gates.rs`**

- [ ] `no_agent_id_literal_is_compared_or_dispatched_on_in_agent_launch`

### Unit / integration tests

- [ ] `tddy-agent-launch`: `cli_launcher` returns the session type for every built-in CLI agent and `None` for `tool`/`workspace`.
- [ ] `tddy-worktree-service`: before deleting them, a one-off equivalence check that `cli_agent(st, spec.has(BranchNameRequired))` equals `claude_cli()`/`cursor_cli()` for the built-in specs (deleted with them).
- [ ] The existing 20 claude/cursor suites pass unchanged on the generic path.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Dispatch gone | Source scan + the existing suites | the scan keeps the property; the suites prove behaviour survived |
| Re-attach | Integration with tmux | restart ordering |

## Decisions & Trade-offs

- Telegram's callers keep four cursor/claude helpers alive one node longer (n18 deletes them), rather than widening this node into Telegram.
- Open: which agent a conversation/pr-stack child gets when the request names none — the parent's (proposed) or a refusal.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-cli-dispatch-collapse-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
