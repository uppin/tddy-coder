# Changeset: A sandboxed session's shell tab runs in a jail of its own

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — several crates (terminal-rpc, changeset, daemon-sandbox, agent-launch, session-lifecycle, service proto, session-web), a proto change, a second-runner-per-session registry with per-tab jail paths, teardown ordering at close/delete/resume/shutdown
**Stack**: `#session-liveness` 5/8 — branch `feature/session-liveness/sandbox-shell-tab`, base `feature/session-liveness/resume-roster`
**PR**: #964 https://github.com/uppin/tddy-coder/pull/964

## Initial Discovery

[2026-11-21-sandbox-shell-tab-initial-discovery.md](./2026-11-21-sandbox-shell-tab-initial-discovery.md)

## Affected Packages

- **tddy-terminal-rpc**: `TerminalKind::from_wrapped_request` accepts `sandbox` around a shell; proto comment on `wrappers`
- **tddy-changeset**: `SANDBOX_KIND` and `LaunchNode::sandbox()`
- **tddy-daemon-sandbox**: a per-tab jail registry keyed `(session_id, terminal_id)`; per-tab jail paths (IPC socket, ready marker, exec plan, profile, egress dir)
- **tddy-agent-launch**: the tab-jail launch (shell recipe, session grants, session worktree); `sandbox` restore kind
- **tddy-session-lifecycle**: roster start/end through the tab-jail port; store routes every non-main id to its owner; shutdown stops every jail
- **tddy-service**: `SessionEntry.sandbox`
- **tddy-session-web**: "In sandbox" / "On host" menu group
- **tddy-coder**: the participant `CoderTerminalRoster` refuses `sandbox`

## Related Feature Documentation

- PRD: [PRD-2026-11-21-sandbox-shell-tab.md](../../ft/daemon/1-WIP/PRD-2026-11-21-sandbox-shell-tab.md)

## Summary

A shell tab opened "In sandbox" on a sandboxed session runs in a jail of its own: a second `tddy-sandbox-runner` over the session's worktree and grants, recorded `[sandbox, shell]` and restored into a new jail on Resume. "On host" tabs of a sandboxed session become reachable.

## Responsibility

- `TabJails` (tddy-daemon-sandbox): the registry of tab jails, keyed `(session_id, terminal_id)`, with `start`, `stop`, `stop_session`, `stop_all`, `live`, and a death watch that ends the tab when its runner exits. An exited tab jail's tab stays in the roster, like a host tab, so Resume reopens it (developer ruling, 2026-11-21).
- `TabJailSpawn` port: start a shell tab jail for `(session, terminal_id)` with the session's worktree and grants; the daemon's implementation reuses `lower_tool_jail` / `build_tool_jail_plan` / `spawn_sandbox_plan` / the host relay, under per-tab paths.
- Per-tab jail paths: `<session>/sandbox/tabs/<terminal_id>/…`, an IPC socket hashing session and terminal id, a per-tab egress dir.
- Wire: `sandbox` accepted around `bash` in `StartTerminalSession.wrappers`; `TerminalKind::Sandboxed…` variant; participant refusal arm.
- Recording `[sandbox, shell]`; restore kind `sandbox` → shell (`restore_plan` + `carry_out`), drop with reason when the jail cannot start.
- `DaemonTerminalSessionStore` routes a non-main id of a sandboxed session to the CLI manager or `TabJails`, not to `None`.
- Teardown: `close_tab`, delete (`forget_session_state`), `HeldSessions::stop_session`, resume, and `shut_down_children` — which also stops the **main** jails (`SandboxSessionManager::stop_all`).
- `SessionEntry.sandbox` (proto) and the web menu group.

## Boundaries

- No agent tab in a jail — n7 (`sandbox-agent-tab`) owns that; a `sandbox` wrapper around an agent is refused here.
- No resize or control lease for jails (developer decision; jail terminals stay as the main's).
- No change to the main jail's launch, resume or relay beyond the shutdown stop.
- No target tabs (n6).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n4` resume-roster (#961) | `restore_plan` per-kind dispatch, `RosterRestore`, `restore_roster_on_resume` on both resume paths, unknown kinds DROPPED | adds the `sandbox` kind and a `carry_out` arm; gives `RosterRestore` the tab-jail port | change the dispatch shape, the outcome proto, or the host resume |
| `n2` tmux-roster (#959) | `tabs.yaml` (`TabRoster`, `LaunchNode`), `TabRosterStore`, `CliManagerTerminalRoster` start/end/close, start under a recorded id | records `[sandbox, shell]` through the store; adds a sandbox branch to start/end | change the roster schema or the store |
| `n1` agent-tab-liveness (#958) | liveness from held agent tabs; `HeldSessions` | a shell tab jail never counts as live | change the liveness rule |

Sequencing found writing the tests (pass 2):

- n4's unit test `a_tab_whose_chain_holds_a_wrapper_this_daemon_does_not_know_is_dropped_naming_it` (`restore_tabs_tests.rs`) used `sandbox` as its example of a kind no daemon knows. By developer ruling (2026-11-21) its example is now `ssh` (one fixture line in n4's test), so it stays green on this contract and through the stack.
- The jailed-tab resume tests resume a **host** `claude-cli` main: a jailed main cannot be started without a kernel sandbox, and `restore_roster_on_resume` restores the whole roster the same way on every resume path. No parent `TODO(` is reached by any of this node's tests (n1–n4 are green).
- Tab jails are injected through `DaemonSessionHost::with_tab_jail_spawn`; `RosterRestore` gains `tab_jails` + `tab_jail_spawn`, `LaunchSessions` gains `tab_jails` + `tab_jail_spawn` (this node's wiring; n4's `restore_plan` dispatch shape is unchanged — one arm added).

## Draft PR contract

The first push publishes `TabJails` and `TabJailSpawn` with `todo!()` bodies, `SANDBOX_KIND`, the `TerminalKind` variant, `SessionEntry.sandbox`, and the failing tests below.

## Green wave

**Wave:** 4 of 5
**Greenable independently:** yes, once n4 is in — tests drive the fake `TabJailSpawn`; the real-jail Linux test needs the prerequisite CI PR
**Concurrent with:** `target-shell-tab` (n6)
**Blocks:** `sandbox-agent-tab` (n7)

Real dependency edges: `n4 → n5, n6      n5 → n7      n6 → n8`

## Prerequisites

### ⛔ BLOCKING (real-jail test only) — [2026-11-02-ci-runs-no-linux-jail.md](../todo/2026-11-02-ci-runs-no-linux-jail.md)

A prerequisite PR off `master`, outside the stack (developer decision 2026-11-21), enables the Linux jail suites in CI. Until it lands, `sandbox_shell_tab_jail_linux.rs` self-skips there like every jail suite. The behaviour tests do not depend on it.

### ✅ RESOLVED HERE (part 1, and part 2 for tab jails) — [2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md](../todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)

Shutdown stops main and tab jails; tab jails get a death watch. Part 2 for the **main** jail (nothing watches it for death) stays open — narrow the entry at wrap.

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-daemon-sandbox/docs/code-issues/oversized-file-sandbox-session.md` (924) — `TabJails` and the per-tab paths go in new modules; `sandbox_session.rs` gains only `stop_all`.
- `packages/tddy-daemon-sandbox/docs/code-issues/heavy-params-sandbox-session-dial-and-bridge.md` (12 params) — the tab relay must not add a parameter; if it needs one, introduce the `DialAndBridge` options struct the record names (and close the record).
- `packages/tddy-agent-launch/docs/code-issues/complexity-jail-launch-launch-jailed.md` (151 lines) — the tab-jail launch is a new function that shares extracted pieces; `launch_jailed` must not grow.
- `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md`, `oversized-file-test-util.md` — test helpers for tab jails go in a new test-support module.

### ℹ ANSWERED — [2026-11-02-jailed-claude-loads-project-mcp-json.md](../todo/2026-11-02-jailed-claude-loads-project-mcp-json.md)

Agent-only; n7's concern.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [x] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

`CliManagerTerminalRoster::start` (svc_terminal_ports.rs:301) starts every tab on the host; `DaemonTerminalSessionStore::get_terminal` (terminal_session_adapter.rs ~395-415) answers only `main` for a sandboxed session, so a host tab there cannot be streamed. `SandboxSessionManager` holds one jail per session (insert overwrites → Drop kills), runner frames say `terminal_id: main`, and IPC socket / ready marker / exec plan / egress dir are per session. `shut_down_children` stops no `SandboxSessionManager` jail. `from_wrapped_request` refuses every wrapper but `tmux`.

### State B (Target)

`sandbox` → `shell` starts a tab jail through `TabJailSpawn`, registered in `TabJails` under `(session, terminal)`; frames from that runner are routed to that tab by the daemon. The tab streams, lists, closes, and is recorded `[sandbox, shell]`. Resume restores it into a new jail, dropping it with its reason on failure. Non-main ids of a sandboxed session reach their owner. Every teardown path stops tab jails; shutdown also stops main jails.

## Acceptance Tests

`packages/tddy-session-lifecycle/tests/sandbox_shell_tab_acceptance.rs` (fake `TabJailSpawn`):
- [x] `a_shell_tab_opened_in_sandbox_starts_a_jail_of_its_own_over_the_session_worktree`
- [x] `a_jailed_shell_tab_streams_its_output_and_is_listed`
- [x] `a_jailed_shell_tab_is_recorded_as_sandbox_then_shell`
- [x] `two_jailed_tabs_of_one_session_get_distinct_jail_paths`
- [x] `closing_a_jailed_tab_stops_its_jail_and_leaves_the_roster`
- [x] `a_tab_jail_that_exits_ends_its_tab`
- [x] `deleting_a_sandboxed_session_stops_every_tab_jail`
- [x] `an_on_host_tab_of_a_sandboxed_session_streams_its_output`
- [x] `a_sandbox_wrapper_around_an_agent_is_refused`
- [x] `a_sandbox_wrapper_on_an_unsandboxed_session_is_refused`

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs` (extended):
- [x] `a_jailed_shell_tab_is_restored_into_a_new_jail_on_resume`
- [x] `a_jailed_tab_whose_jail_cannot_start_is_dropped_with_its_reason`
- [x] `a_mixed_roster_restores_each_tab_where_its_chain_says`

`packages/tddy-session-lifecycle/tests/sandbox_shutdown_acceptance.rs`:
- [x] `shutting_down_the_daemon_stops_main_and_tab_jails`

`packages/tddy-session-lifecycle/tests/sandbox_shell_tab_jail_linux.rs` (`#[cfg(target_os = "linux")]`, real jail):
- [x] `a_jailed_shell_tab_works_in_the_worktree_and_cannot_write_outside_its_grants`

`packages/tddy-session-web/cypress/component/NewTerminalMenuSandboxAcceptance.cy.tsx`:
- [x] `offers In sandbox (default) and On host for a sandboxed session`
- [x] `offers no sandbox choice for an unsandboxed session`

Added in pass 2: `sandbox_shell_tab_acceptance.rs` — `a_sandboxed_session_is_listed_as_sandboxed` (`SessionEntry.sandbox`, which the menu reads).

## Unit Tests

- `from_wrapped_request`: `sandbox` around `bash` accepted; around an agent, or nested in `tmux`, refused.
- `TabJails`: insert/stop per `(session, terminal)`, `stop_session` stops only that session's tabs, `live` reflects the death watch.
- Per-tab jail paths: distinct per terminal id; the socket path stays inside the AF_UNIX limit.
- `restore_plan`: `[sandbox, shell]` plans a jailed fresh shell; `[sandbox, agent]` drops with a reason (n7 adds it).

Written (pass 2): `tddy-terminal-rpc/tests/terminal_kind_sandbox_wrapper_unit.rs` (6),
`tddy-daemon-sandbox/src/tab_jails_tests.rs` (10), `tab_jail_paths_tests.rs` (7),
`sandbox_session_stop_all_tests.rs` (2), `tddy-agent-launch/src/opencode_cli_spawn/restore_tabs_sandbox_tests.rs` (4).

## Decisions & Trade-offs

- **One jail per tab** (developer decision), same worktree and grants as the session: a tab's crash or exit never takes the main's jail with it; costs one runner per tab.
- **Per-tab choice, jail by default** (developer decision); "On host" stays.
- **Jail terminals keep the main's limits** — no resize, no control lease (developer decision).
- **An exited tab jail stays in the roster** (developer ruling, 2026-11-21), like a host tab: unlisted, still recorded `[sandbox, shell]`, reopened by Resume.
- **Shutdown stops every jail**, main ones included (developer decision) — resolves part 1 of the 2026-09-15 entry.
- **Real-jail coverage via a prerequisite CI PR** (developer decision), not inside this node.

## TODO

- [x] Record initial discovery (`2026-11-21-sandbox-shell-tab-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
