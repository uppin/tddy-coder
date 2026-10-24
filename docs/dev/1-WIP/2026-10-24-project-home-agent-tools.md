# Changeset: Project-home agent reaches its tools

**Date**: 2026-10-24
**Status**: 🚧 In progress — green: 12 planned tests pass; validating
**Type**: Bug Fix

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-project-home-agent-tools-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | What this change does about it |
|---|---|---|
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md` | ⚠ During | Touched (resume re-wires the main). The role wiring enters `resume_claude_cli_session` as **one call** replacing today's `role_allowed_tools_args` line, with assembly in a helper; net growth ≤ 2 lines. Re-measured at wrap |
| [2026-10-24-sandboxed-main-needs-session-role.md](../todo/2026-10-24-sandboxed-main-needs-session-role.md) | ⚠ During | Not fixed here. The wiring function takes no sandbox assumptions, so the follow-up that allows a sandboxed main can call it; noted in that TODO's "What would close it" at wrap |
| [2026-10-23-project-home-git-push-remote-and-token.md](../todo/2026-10-23-project-home-git-push-remote-and-token.md) | ⚠ During | Not fixed here. Becomes **reachable** for the first time: once main has its tools, `PushSessionBranch` runs with the checkout's credentials and pushes to `origin` |
| [2026-10-17-desktop-cursor-hooks-reach-daemon.md](../todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md) | — Unrelated | Same embedded-socket choice (`embedded_host_agent_tool_socket`), cursor hooks only |

No `**Claimed by:**` issue in any touched package. `tddy-session-tool-client` and
`tddy-sandbox-recipes` have no `docs/code-issues/` (not analyzed); both are read-only consumers here.

## Affected Packages

- [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md) — the main's launch wiring at start and resume
- [`tddy-session-split`](../../../packages/tddy-session-split/README.md) — agent-token mint and co-located env builder reached from `tddy-agent-launch`
- [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md) — acceptance tests and the shared project-home fixture

## Related Feature Documentation

- PRD: [PRD-2026-10-24-project-home-agent-tools.md](../../ft/daemon/1-WIP/PRD-2026-10-24-project-home-agent-tools.md)
- [Project concept](../../ft/daemon/project-concept.md) § Project-home session, § Project-home tools

## Summary

A claude-cli main is launched and resumed with `tddy-tools --mcp` as its only MCP server, wired to
this daemon (agent-tool socket on an embedded host, HTTP URL on a binary one) under its own session
id and a token minted for it; every native filesystem and shell tool withdrawn; and a role prompt.

## Background

See the PRD. Short form: session `01a16dc6-…` had `--allowedTools mcp__tddy-tools__*` with no MCP
server behind them, kept native Bash, and had no idea it was main.

## Scope

- [x] Main's launch wiring: MCP config, `TDDY_REMOTE_*` env per host kind, native-tool disallow list, role prompt file
- [x] Start: mint the agent token from the caller's and apply the wiring
- [x] Resume: same, with a fresh token
- [x] Mint refusal no longer says "a split session" (now "cannot sign for the caller: …" — the same check backs the room-poll minter too)
- [x] Shared project-home test fixture: signer + signed caller token; stub dumps `TDDY_REMOTE_*`
- [x] TODOs for the two side findings (written in this planning pass)

## Technical Changes

### State A (Current)

- `claude_cli_spawn_steps::role_allowed_tools_args(role)` → `--allowedTools` per project-home tool;
  the **only** role input to the spawn (`spawn_claude_cli_process`) and to resume
  (`svc_resume_claude_cli_session.rs:71`).
- No `--mcp-config`, `--strict-mcp-config`, `--disallowedTools` or `--append-system-prompt-file` for
  a main; no `TDDY_REMOTE_*` env. `TDDY_SOCKET` only for managed recipes.
- `spawn_claude_cli_session_inner` has no caller token; `LaunchSessions::start_claude_cli_session`
  does.
- Reusable, unused by this path: `split_session::colocated_jail_tool_env`,
  `agent_credentials::mint_agent_session_token` (pub(crate)), `SplitSessions::agent_session_token_for`
  (pub(crate)), `split_session::NATIVE_FILESYSTEM_TOOLS`, `tddy_sandbox_recipes::write_claude_mcp_config`,
  `resolve_tddy_tools_path`, `tddy_daemon_kernel::embedded_host_agent_tool_socket`.

### State B (Target)

```rust
// packages/tddy-agent-launch/src/claude_cli_spawn_steps.rs
pub(crate) struct ProjectHomeLaunch<'a> {
    pub(crate) config: &'a DaemonConfig,
    pub(crate) tddy_data_dir: &'a Path,     // the *resolved* one: the socket path digests it
    pub(crate) session_dir: &'a Path,       // where the MCP config and prompt file are written
    pub(crate) session_id: &'a str,         // the main's own id
    pub(crate) agent_session_token: &'a str,// minted, never the caller's
    pub(crate) tddy_tools_path: &'a Path,   // absolute toolchain binary
}

#[derive(Debug, Default)]
pub(crate) struct RoleLaunch {
    pub(crate) extra_args: Vec<String>,     // --allowedTools…, --disallowedTools…, --append-system-prompt-file, --mcp-config, --strict-mcp-config
    pub(crate) env: Vec<(String, String)>,  // TDDY_REMOTE_* (colocated_jail_tool_env(..).env_pairs())
}

pub(crate) fn project_home_launch(launch: ProjectHomeLaunch<'_>) -> Result<RoleLaunch, Status>;
```

- Start: `LaunchSessions::start_claude_cli_session` mints (via `SplitSessions`) when the role is
  `project-home` and threads the token to `spawn_claude_cli_session_inner`, which calls
  `project_home_launch` and merges `extra_args`/`env` into the spawn. Ordinary sessions: unchanged.
- Resume: one call replaces `role_allowed_tools_args` in `resume_claude_cli_session`.
- `tddy-session-split`: a `pub` way to mint an agent token from a caller token
  (`SplitSessions::mint_agent_session_token` or equivalent); `colocated_jail_tool_env` and
  `NATIVE_FILESYSTEM_TOOLS` are already `pub`.

### Delta (What's Changing)

| Package | Change |
|---|---|
| `tddy-agent-launch` | `project_home_launch` + prompt text; start and resume call it; token threaded into the start |
| `tddy-session-split` | `pub` token mint for a caller token; refusal wording neutral |
| `tddy-session-lifecycle` (tests) | fixture signer, signed caller token, env dump; new acceptance cases |

## Implementation Milestones

- [x] `project_home_launch` passes its unit tests (embedded + binary hosts)
- [x] An ensured main is launched with the wiring (acceptance)
- [x] A resumed main is launched with the wiring and a fresh token (acceptance)
- [x] Ordinary claude-cli start unchanged (existing + new regression case)
- [ ] `./test -p tddy-agent-launch -p tddy-session-lifecycle -p tddy-session-split` green

## Testing Plan

### Testing Strategy

- **Acceptance (integration)** in `tddy-session-lifecycle` through `EnsureProjectHomeSession` on a
  real `DaemonSessionHost` with a stub `claude` that echoes argv and the `TDDY_REMOTE_*` env. This is
  the user-visible contract: what the main's process is launched with. The fixture is a binary host
  (no agent-tool socket bound), so it pins the HTTP transport.
- **Unit** in `tddy-agent-launch` (`claude_cli_spawn_steps_tests.rs`) over `project_home_launch`
  with the existing `AnEmbeddedHost` / `ARegularHost` fixtures — the embedded-vs-binary split, which
  the acceptance fixture cannot bind cheaply, and the file contents written.

### Testing Options Analysis

| Option | Verdict |
|---|---|
| Real `tddy-tools --mcp` against a live daemon (`ListProjectSessions` round trip) | Rejected: needs the full runtime (`tddy-daemon` + socket server); the `tddy-daemon-rpc` harness resolves only a fixed `TEST_TOKEN`. The transport selection is already tested in `tddy-session-tool-client` |
| Verify the agent's token under the daemon's own `SessionTokens` | **Chosen** for the credential: the production user resolver does exactly this, so "verifies as the caller's login" is what `ExecuteTool` will accept |
| Assert argv/env only at unit level | Rejected alone: would not catch the start/resume call sites forgetting to call it — the actual bug |

### Coverage Requirements

Every acceptance criterion in the PRD has at least one test; start and resume each covered.

## Acceptance Tests

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs`

- `an_ensured_main_loads_tddy_tools_as_its_only_mcp_server` — argv has `--mcp-config <main's session dir>/claude-mcp-config.json` and `--strict-mcp-config`; the config registers `tddy-tools` with `--mcp` at an absolute path
- `an_ensured_mains_tools_reach_this_daemon_as_the_main` — env `TDDY_REMOTE_SESSION_ID` = main's id; `TDDY_REMOTE_DAEMON_URL` = this daemon's local URL; no `TDDY_REMOTE_DAEMON_SOCKET`
- `an_ensured_main_is_given_a_token_of_its_own_that_verifies_as_the_caller` — `TDDY_REMOTE_SESSION_TOKEN` is not the caller's token and verifies under this daemon's tokens with the caller's login
- `an_ensured_main_has_no_native_shell_or_file_tools` — `--disallowedTools` for each of `Bash`, `Edit`, `Write`, `Read`, `Grep`, `Glob`
- `an_ensured_main_is_told_it_is_the_projects_main` — `--append-system-prompt-file` names a file that mentions `StartProjectSession` and `ListProjectSessions` and says it has no shell
- `a_resumed_main_is_wired_to_its_tools_again_with_a_fresh_token` — stop, advance, ensure → resumed argv has `--mcp-config`, env has the main's id and a token different from the first launch's

Dropped while writing them: *a main whose caller cannot be verified is refused*. The ensure resolves
the caller through the daemon's user resolver before anything is minted, so an unverifiable token is
already refused with `UNAUTHENTICATED` today; a test would pass before the change and pins nothing
new. The production resolver verifies the same signature the mint does, so the mint's refusal is
unreachable except across a token expiring mid-request.

Ordinary sessions: covered by the unit test `an_ordinary_spawn_adds_no_allowed_tools` (unchanged) and
the existing `claude_cli_session_acceptance.rs` argv cases, which run with no role.

### tddy-agent-launch — `packages/tddy-agent-launch/src/claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs`

- `on_an_embedded_host_a_main_reaches_the_daemon_over_its_agent_tool_socket`
- `on_a_regular_host_a_main_reaches_the_daemon_over_its_http_url_and_no_socket`
- `a_mains_tools_carry_its_own_session_id_and_the_minted_token`
- `a_main_loads_only_the_tddy_tools_mcp_server_from_its_session_dir`
- `a_main_is_pre_approved_its_tool_set_and_denied_every_native_filesystem_tool`
- `a_mains_role_prompt_is_written_to_its_session_dir_and_appended`

## Technical Debt & Production Readiness

- ✅ `project_home_launch` implemented; the red-phase `todo!()` and its TODO are gone.
- A project-home start or resume now **fails** when the toolchain's `tddy-tools` cannot be found (`resolve_tddy_tools_path` refuses a missing binary). Deliberate: a main without its MCP server is the bug this fixes. Ordinary sessions are unaffected.
- `start_role_launch` takes 6 parameters (repo budget 5) — checked at `/analyze-code-issues`.

## Decisions & Trade-offs

- **Scope** (developer, 2026-10-24): all three gaps — MCP wiring, native tools withdrawn, role prompt; both embedded and binary hosts; side findings recorded as TODOs.
- **Bash withdrawn fully** (developer): main's only reach is the daemon-enforced tool set.
- **Token minted, never forwarded**: the caller's token lives 5 min. Unverifiable caller → `UNAUTHENTICATED`, nothing started (no fallback).
- **No `--permission-prompt-tool`**: main is an interactive terminal; unapproved calls prompt in the TUI.
- **Main's own session id** in `TDDY_REMOTE_SESSION_ID`: its record holds the home worktree, so `ExecuteTool` resolves `HostWorktree` and applies the role guard.
- **Env on the process, not in the MCP config's `env` block**: matches the split/sandboxed-codebase placements; `tddy-tools --mcp` inherits it.
- **Shared fixture changes** (`project_home_support`): signer + signed caller token, touching both project-home suites.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)

### From @red (TDD Red Phase)

- Red-phase contract: `ProjectHomeLaunch`, `RoleLaunch`, `project_home_launch` (signatures as in State B). Green also wires the call sites (start mint + resume) — the acceptance tests fail on those, not on this function alone.

### From @green

- `tddy-agent-launch` does not depend on `tddy-sandbox-recipes`, so the MCP-only argv is a new `tddy_session_split::agent_argv::tddy_tools_only_mcp_args`, which `split_claude_extra_args` now also uses (split argv byte-identical).
- The mint is the existing `SplitSessions::agent_session_token_for`, widened to `pub` (no second method).
- `resume_claude_cli_session`: 144 → 146 lines (within the ≤ 2 budget); the role wiring is `LaunchSessions::resumed_role_launch`.

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

### From @refactor (Completed Refactorings)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-10-24-project-home-agent-tools-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-agent-launch -p tddy-session-split -p tddy-session-lifecycle`) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p tddy-agent-launch -p tddy-session-split -p tddy-session-lifecycle -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
