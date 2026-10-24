# Changeset: Headless `-p` coding agents as subagents

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — two crates, three spawned-process seams, permission routing through an MCP tool into n3's yield, per-CLI confinement facts partly unconfirmed
**Branch**: `feature/coagents/headless-driver`, base `feature/coagents/remote-codebase` (#725) — `#coagents` 7/8
**PR**: [#726](https://github.com/uppin/tddy-coder/pull/726)

## Initial Discovery

[2026-10-24-coagents-headless-driver-initial-discovery.md](./2026-10-24-coagents-headless-driver-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ℹ ANSWERED — Muse has no headless backend — [`2026-10-22-muse-code-headless-backend.md`](../todo/2026-10-22-muse-code-headless-backend.md)

Muse stays out; the entry is unchanged.

### ⚠ DURING — `backends` file budget — `packages/tddy-agent-backend/docs/code-issues/`

The headless driver goes in a new module; `claude.rs`, `cursor.rs`, `codex.rs` grow only by the
helpers they expose (argv builders, stream parsers made reusable).

## Responsibility

- `tddy_agent_backend::headless_driver::{HeadlessDriver, HeadlessHarness { Claude, Cursor, Codex, Oz }, HeadlessTurn}` — one turn = one CLI invocation with the conversation's session id (fresh → `--session-id`/none; then `--resume` / `exec resume`), stream → `AcpUpdate`s (n1's shape), cancel → kill the child, drop → kill.
- A `ConversationDriver` trait over `AcpDriver` and `HeadlessDriver` that `CodingAgentSession` holds (n2's session generalised; the trait lives with the drivers).
- Claude permission routing: `--permission-prompt-tool mcp__tddy-tools__approval_prompt` pointed at a per-conversation `tddy-tools` endpoint that parks the request into n3's `ParkedPermissions`.
- Catalog: `claude`, `cursor`, `codex` rows with `transport` (`acp` | `headless`) and `permission_policy` (`asks_caller` | `fixed:<text>`) on `SubagentInfo` / `SessionAgentEntry`.
- Confinement table rows for the three CLIs.
- Web: transport and policy text on the picker row.

## Boundaries

- No Muse. No change to the workflow backends' flags or behaviour. No new ACP behaviour.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `acp-driver` (#720) | `AcpDriver`, `AcpUpdate`, `AcpTurnEnd` | `ConversationDriver` abstracts over it; headless emits the same `AcpUpdate` | change `AcpDriver` |
| n2 `acp-subagent` (#721) | `CodingAgentSession`, catalog, `CodingAgentLauncher`, `HarnessConfinement` | generalises the session to the trait; adds rows | change kind fields |
| n3 `permission-yield` (#722) | `ParkedPermissions`, `StopReason::PermissionRequested` | claude's prompt tool parks into it | change the yield or the answer |
| n5, n7, n8 | inherited | transcripts / placement / relay apply unchanged | — |

## Draft PR contract

`HeadlessDriver`, `HeadlessHarness`, `ConversationDriver`, the catalog fields, the permission-tool
endpoint signature — bodies `TODO(headless-driver)` — and the failing tests below.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — needs n3's permission yield behaviour (claude routing test) and n2's session
**Concurrent with:** n6 `web-approvals`, n8 `remote-codebase`
**Blocks:** none

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6      n7 → n8

## Affected Packages

- **tddy-agent-backend**: `headless_driver` module; `ConversationDriver`; reusable parsers / argv
- **tddy-coding-subagent**: session over the trait; claude permission endpoint; catalog + confinement rows
- **tddy-service**: `SubagentInfo` / `SessionAgentEntry` `transport`, `permission_policy` (regenerated TS)
- **tddy-web**: picker row text

## Related Feature Documentation

- [PRD-2026-10-24-coagents-headless-driver.md](../../ft/coder/1-WIP/PRD-2026-10-24-coagents-headless-driver.md)

## Summary

A headless transport behind the same conversation interface; `claude`, `cursor`, `codex` become
coding subagents.

## Scope

- [ ] **Implementation** · [ ] **Testing** · [ ] **Package Documentation** · [ ] **Code Quality**

## Technical Changes

### State A (Current)

- `backend/claude.rs:69-139` `build_claude_args` (`-p … --output-format stream-json --verbose`, `--resume`, `--permission-prompt-tool`, `--mcp-config`, `--allowedTools`); `:549-565` `process_ndjson_stream`.
- `backend/cursor.rs:177-227` argv (`-p … stream-json --stream-partial-output --force --trust --approve-mcps`, `--resume`), writes `<wd>/.cursor/mcp.json`.
- `backend/codex.rs:421-464` `codex exec [-C wd] -s … --json [resume <thread>]`; parsed after exit (`:284-314`).
- Global `CHILD_PID` + SIGKILL (`mod.rs:168-193`) — one child per process; a per-conversation driver needs its own handle.

### State B (Target)

```
CodingAgentSession ─► dyn ConversationDriver ─┬─ AcpDriver (n1)
                                              └─ HeadlessDriver ─ claude -p | cursor agent -p | codex exec --json
                                                      claude permission prompt ─► tddy-tools approval_prompt ─► ParkedPermissions (n3)
```

## Testing Plan

**Level:** integration with **fake CLIs** — small scripts under `tddy-testing-commons` that emit
recorded stream-json / JSONL fixtures and record their argv, put on `PATH` via the harness env
overrides (`TDDY_CURSOR_AGENT`, a `claude` path override, `codex` path). Real CLIs are not run in CI.

## Acceptance Tests

`packages/tddy-agent-backend/tests/headless_driver_acceptance.rs` — `HeadlessDriver` over **fake CLIs**: shell scripts that append their argv to a record file and print the real event shapes (claude/cursor stream-json from `stream/claude.rs`'s fixtures, codex JSONL from `stream/codex.rs`'s):

1. `a_claude_turn_returns_its_final_text_and_the_next_turn_resumes_its_session` — `-p … stream-json`, then `--resume cl-1`
2. `a_cursor_turn_returns_its_final_text_and_the_next_turn_resumes_its_chat` — `--resume cu-1`
3. `a_codex_turn_returns_its_final_text_and_the_next_turn_resumes_its_thread` — `exec … --json`, then `resume th-1`
4. `cancelling_a_headless_turn_kills_the_cli_and_returns_cancelled` — a CLI that sleeps 30 s, cancelled within 5 s
5. `headless_tool_calls_become_transcript_updates` — claude `tool_use` → `AcpUpdate::ToolCall { id: "tu-1", title: "Read…" }`
6. `a_codex_turns_tool_calls_arrive_at_turn_end` — `command_execution` item → a `ToolCall` titled with the command
7. `the_model_and_the_permission_prompt_tool_reach_claudes_argv` — `--model`, `--permission-prompt-tool mcp__tddy-tools__approval_prompt`, `--disallowedTools`
8. `harness_ids_round_trip` — **passes at contract** (the published id table)

`packages/tddy-coding-subagent/tests/transports_acceptance.rs` — `transport_of` / `permission_policy_of`:

9. `acp_harnesses_take_the_acp_transport_and_ask_the_caller`
10. `claude_headless_asks_the_caller_through_its_permission_prompt_tool`
11. `cursor_and_codex_run_under_a_fixed_policy_the_row_names`
12. `a_pty_only_harness_has_no_subagent_transport` — negative, passes

`packages/tddy-daemon-rpc/tests/headless_listing_acceptance.rs`:

13. `headless_harnesses_are_listed_with_their_transport_and_permission_policy` — `transport: "headless"`, `permission_policy: "asks_caller"` / `"fixed: …"`
14. `a_harness_offered_both_ways_is_listed_once_per_transport`

**Changed from the plan:** the claude permission-prompt **yield** end to end (plan test 4) needs n3's `ParkedPermissions` behaviour and a per-conversation `tddy-tools` approval endpoint the daemon serves; the contract pins the CLI side (7) and green wires the endpoint, adding the round-trip test to `tddy-coding-subagent` once #722 is green. Plan test 10 (a managed session refusing an unconfinable headless harness) is covered by #721's admission test once `confinement_for` lists the CLIs. **Green must update #721's `a_headless_or_pty_harness_profile_is_not_offered_yet` and `a_headless_harness_profile_is_not_listed_yet`** — their premise ("not yet") is exactly what this node ends; they become "a PTY-only harness is not offered". `CodingAgentDef` is not widened: transport and policy are derived from the harness id (`transports.rs`), so #721's type is untouched. New proto: `SubagentInfo.transport = 10`, `.permission_policy = 11`; `SessionAgentEntry.transport = 16`, `.permission_policy = 17`. The web row text is green's (the badge already names the harness).

**Baseline (2026-10-24, isolated `CARGO_TARGET_DIR`):** `cargo check --all-targets` clean across agent-backend, coding-subagent, discovery, session-agents, daemon-rpc, tools, session-split, session-lifecycle, daemon; scoped clippy clean.

### Added 2026-10-24 — Warp via `oz agent run` (developer's decision)

Master gained Warp (#717/#718) as the interactive `warp-cli` session type; the `warp` TUI has no
headless or ACP mode. Warp's headless runner is the standalone `oz` CLI (`brew tap warpdotdev/warp &&
brew install --cask oz`). **Observed on a real `oz` (v0.2026.10.21, 2026-10-24)**, captures in
`packages/tddy-agent-backend/tests/fixtures/oz/` (ids, paths and user name replaced):

| Fact | Observed |
|---|---|
| argv | `oz agent run -p <PROMPT> --output-format ndjson -C <DIR> [--conversation <ID>] [--model <ID>] [--mcp <json\|path>]... [--profile <ID>]` |
| events | `{"type":"system","event_type":"run_started",run_id,run_url}`, `{"type":"system","event_type":"conversation_started",conversation_id}`, `{"type":"tool_call","tool":"run_command",command}` / `{"type":"tool_call","tool":"edit_files",title,file_paths}`, `{"type":"tool_result","tool":…,status?,exit_code?,output? \| diff}`, `{"type":"agent",text}`; exit 0 at turn end |
| resume | `--conversation <id>` continues a **local** conversation (same id echoed, context kept) |
| permissions | the default profile auto-approves commands and edits headlessly — no prompt, no wait |
| tool ids | `tool_call` carries none; a `tool_result` pairs with the preceding `tool_call` of the same `tool` |

Owned additions: `HeadlessHarness::Oz` (id `oz`, label "Warp (oz)"), `HeadlessCommand.profile`, `oz`
in `HEADLESS_HARNESSES` / `transport_of` (Headless) / `permission_policy_of` (Fixed, naming the agent
profile), `confinement_for("oz") = CannotWithdraw` until a confining profile is proven. Distinct from
master's `warp-cli`, which stays a PTY session type and is never a subagent.

Tests added (fail at the TODOs):

- `headless_driver_acceptance.rs` — `an_oz_turn_returns_its_text_and_the_next_turn_continues_its_conversation`, `an_oz_turns_commands_and_edits_become_tool_calls_with_their_results`, `the_worktree_model_mcp_and_profile_reach_ozs_argv`; `harness_ids_round_trip` covers `Oz`
- `transports_acceptance.rs` — `warp_oz_is_headless_and_runs_under_its_agent_profiles_policy` (and `warp-cli` has no subagent transport)
- `headless_listing_acceptance.rs` — `a_warp_oz_profile_is_listed_as_a_headless_coding_agent`

**Complexity** unchanged at 4: one more harness on the same seam.

## Technical Debt & Production Readiness

Green progress, as committed on `feature/coagents/headless-driver`:

- `c51c87a86` — `HeadlessDriver` for claude, cursor, codex and oz; `AcpConversation` delegates.
  `headless_driver_acceptance` 11/11.
- `1af20d5c6` — `transport_of` / `permission_policy_of` / `transport_and_policy_wire`; the catalog
  row and the roster entry carry transport and policy. `transports_acceptance` 5/5.
- `805dca13b` — the toolcall listener answers a conversation-tagged `approve` through a
  `ConversationApprovalHandler`; no handler means refused, never the presenter.
- `bddcc0122` — renames of those three test files to behaviour names (the testing guide forbids
  adding `*_acceptance.rs`).
- `e4323686c` — `approval_prompt` with `TDDY_CONVERSATION_ID` relays a tagged approve over
  `TDDY_SOCKET` and answers Claude from the reply; only `mcp__tddy-tools__*` skips the relay. Any
  error, timeout or missing socket is a denial.

Not done, and why:

- **Offering is not changed.** Headless harnesses are not yet listed. `headless_listing_acceptance`
  stays 0/3 until the offering lands. Offering must follow the open path, or a listed row opens as
  ACP and fails at `initialize`.
- **Production approval handler is not bound.** `tool_host_wiring.rs` passes
  `conversation_approval: None`, so a tagged approval is refused in production. The embedded
  host-agent tool socket that coding agents reach (`embedded_host_agent_tool_socket`) is
  daemon-wide, so the handler must check that the caller owns the tagged conversation before it
  parks anything. Decision needed.
- **No headless open path.** `CodingAgentSession` still holds `AcpDriver` only. Moving it over
  `ConversationDriver`, with the claude permission route wired to the conversation's
  `ParkedPermissions`, is the remaining node work.
- **Offering conflicts with an existing test.** `an_acp_allowed_agent_is_a_coding_agent_on_its_default_model`
  (`packages/tddy-coding-subagent/tests/catalog_acceptance.rs`) asserts that `cursor` as an
  `allowed_agents` id is not offered. Profiles can be offered without touching it; `allowed_agents`
  headless ids need that test's premise narrowed first.
- **Claude confinement stays `CannotWithdraw`** until the headless launcher applies
  `--disallowedTools`. A managed session refuses a headless claude until then.
- **Picker row text is not done.** No contract test pins it. A red Cypress test comes first.
- **Pre-existing, outside this node.** The untagged `relay_approve` in `tddy-tools` speaks the older
  newline-delimited protocol that the listener no longer serves. `workspace_roster_admission`
  (`packages/tddy-session-agents/tests/`) is red on the base commit (`TODO(remote-codebase)` stub).
- **Driver residuals.** Cursor surfaces no tool-call updates: its tool events differ from claude's.
  Assistant prose reaches the transcript only as the turn's final text, not as `AcpUpdate::Text`.
  Cursor's `mcp_config` is not written, and codex and oz ignore it. Cursor's argv drops `--force`
  (the PRD's read-only default). The normal path ends on stdout EOF, so a grandchild that holds
  stdout open stalls the turn; cancel does not have that problem.
- **Test-only gate removed.** The tagged relay reads `TDDY_CONVERSATION_ID` without a `cfg(test)`
  branch. A unit-test run from inside a managed conversation's shell therefore sees the tagged
  path. Running tests outside a conversation shell avoids it.

## Decisions & Trade-offs

- **ACP preferred, both listed.** The operator picks; the row says which transport and which policy.
- **Fixed policies for cursor / codex** because neither CLI can ask mid-turn; the policy is visible,
  never implied.
- **Fake CLIs** keep CI hermetic; a manual smoke against the real CLIs is recorded at green.
- **Tagged approvals skip local pre-allow.** Only `mcp__tddy-tools__*` is exempt, so a headless
  subagent yields every permission prompt to its conversation. `Bash(tddy-tools *)` is not kept,
  since a prefix match would let `tddy-tools x; <anything>` through.
- **Round-trip test in the listener first.** The conversation-tagged approve has its own red tests
  before the listener binds a handler, per the developer.

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-coagents-headless-driver-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer 2026-10-24: "finish the remaining nodes without stopping")
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-agent-backend -p tddy-coding-subagent -p tddy-daemon-rpc`) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p <each> -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
