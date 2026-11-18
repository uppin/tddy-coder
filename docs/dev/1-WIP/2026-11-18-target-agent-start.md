# Changeset: Claude and Cursor start on a target with the target's own binary, environment, chat and trust

**Date**: 2026-11-18
**Status**: 🟢 Green (live Claude/Cursor check on `mantasi/remote-dev` left to [W])
**Type**: Fix
**Complexity**: 4. It spans several crates: `tddy-agent-launch`, `tddy-agent-backend` and possibly
`tddy-coding-agent-specs`. It changes the remote-command contract, adds a JSON merge on the target,
and uses integration tests on stand-in targets.
**PR**: #947 · **Stack**: `#coder-start` 7/10 — branch `feature/coder-start/target-agent-start`, base `feature/coder-start/idle-status`

## Initial Discovery

[`2026-11-18-target-agent-start-initial-discovery.md`](2026-11-18-target-agent-start-initial-discovery.md)

## Prerequisites

- ✅ RESOLVED HERE: [`2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md`](../todo/2026-11-18-a-target-launch-runs-the-daemon-hosts-agent-path.md)
- ✅ RESOLVED HERE: [`2026-11-18-cursor-chat-is-minted-on-the-daemon-host-for-a-target.md`](../todo/2026-11-18-cursor-chat-is-minted-on-the-daemon-host-for-a-target.md)
- ✅ RESOLVED HERE: [`2026-11-18-claude-on-a-target-will-meet-the-folder-trust-prompt.md`](../todo/2026-11-18-claude-on-a-target-will-meet-the-folder-trust-prompt.md)
- ⚠ DURING: [`2026-11-16-remote-shell-test-fixtures-are-duplicated.md`](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md). New stand-in behaviour (a target home, PATH and agent location of its own) goes into `L/remote_agent_launch_acceptance.rs`'s `ATarget`. The entry belongs to `target-test-kit`.
- ℹ Code issue `complexity-remote-agent-launch-launch-on-target.md` (6 parameters). This node must not add a seventh; new launch inputs go on `RemoteLaunch`.

## Affected Packages

- `tddy-agent-launch`:
  - `remote_agent_launch.rs`: `remote_command`, `remote_agent_command`, and `RemoteLaunch` (clone root).
  - `spec_launch.rs`: render with the bare `tool.command` for a target.
  - `cli_agent_steps/conversation.rs`: `ConversationStart` gains the target shell.
  - `opencode_cli_spawn.rs` and `resume.rs`: pass `target_shell`, and choose the `node_launcher` on a target.
  - A new `target_trust.rs`: merges `~/.claude.json`.
  - The callers `harness_launch.rs` and `tddy-session-driver/src/session.rs`.
- `tddy-agent-backend`: `adapters/conversation_id.rs` gains a shell-run mint for `CursorCreateChat`.
- `tddy-session-driver`: `--binary` and `--env` become optional workarounds. They stay as overrides, and their docs are updated.
- Docs: `docs/ft/daemon` (remote targets), and the architecture docs of `tddy-agent-launch` and `tddy-session-driver`.

## Scope

- [ ] Agent binary and base env resolved on the target
- [ ] Cursor mint through the target shell; a local spawn into a target path is refused
- [ ] Clone root pre-trusted in the target's `~/.claude.json`
- [~] Each pinned by a stand-in test that fails before it (red written, commit 2)
- [ ] Live Claude and Cursor starts on `mantasi/remote-dev`
- [ ] Scoped gates

## Responsibility

Everything a remote agent start needs from its target: which binary, which environment, where its
chat is minted, and whether its folder is trusted.

## Boundaries

- No change to local starts.
- No change to the client side of the terminal (`client_command`'s `SSH_AUTH_SOCK`/`PATH` is the local `ssh`/`coder`).
- No answering of runtime permission questions on a target.
- No seventh parameter on `launch_on_target`.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/coder-start/coder-transport` | pager-free, deadline-bounded one-shots | the home probe, the `~/.claude.json` read/write and the mint run as one-shots | change the wrapper |
| #944 (`feature/coder-start/sweep`) | `launch_on_target` split into helpers | edits `remote_command` and `plan_the_launch` | re-split it |

## Draft PR contract

Commit 2 contains:
- the failing tests listed below;
- the `RemoteLaunch` field for the clone root;
- the `ConversationStart` target-shell field;
- the `ConversationIdAdapter` shell-mint signature, with stub bodies.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes. Every test uses stand-in targets, and it needs no behaviour from
`coder-transport`. Its live check does.
**Concurrent with:** `coder-transport`, `idle-status`
**Blocks:** `target-test-kit` (the live Claude/Cursor starts)

## Technical Changes

### State A

See the discovery doc, Exploration 2:
- `remote_command` (`remote_agent_launch.rs:294-310`) feeds `std::env::vars_os()` to `agent_env`.
- `remote_agent_command` (56-67) quotes the host argv0.
- `start_conversation` builds `ConversationMint` from the host env and the target worktree.
- Nothing edits `~/.claude.json`.

### State B

- The remote command becomes ``cd '<wt>' && … && exec "${SHELL:-/bin/sh}" -lc 'exec env -i HOME="$HOME" PATH="$PATH" TMPDIR="${TMPDIR:-/tmp}" <base names expanded on the target> <explicit K=V> <bare command> <args>'``, so the target expands its own base set.
- The minted chat runs `cd '<wt>' && <same env> <bare command> create-chat` through `shell.exec`, and its output is parsed tolerantly of pty noise.
- `target_trust`:
  - probe `$HOME`;
  - `read_file` `~/.claude.json`, where an absent file is `{}`;
  - set `projects["<root>"].hasTrustDialogAccepted = true` plus the MCP choice;
  - `write_file` with `OwnerOnly`, and only when the file changed.

## Acceptance Tests

- `tddy-agent-launch/tests/remote_agent_launch_acceptance.rs`:
  - `a_target_runs_its_own_agent_with_its_own_home_path_and_tmpdir`
  - `the_daemon_hosts_environment_never_reaches_a_target_agent`
- `tddy-agent-launch/src/remote_agent_launch.rs` (unit): the exact remote command string, which updates the existing pins at 471 and 525.
- `tddy-agent-launch/tests/target_trust_acceptance.rs`:
  - `a_claude_start_on_a_target_trusts_its_clone_root`
  - `pre_trust_keeps_the_rest_of_claude_json`
  - `a_second_start_does_not_rewrite_claude_json`
- `tddy-agent-launch/src/cli_agent_steps/cli_agent_steps_tests.rs`: `a_minted_conversation_on_a_target_is_minted_through_its_shell`
- `tddy-agent-backend` (unit): `a_mint_into_a_missing_directory_names_the_directory`

## Decisions & Trade-offs

- **Red phase (commit 2).** Nine tests fail because the fix is missing: 7 new tests and 2 updated pins in `remote_agent_launch.rs`. The red run reports 595 passed and 9 failed in `tddy-agent-launch` + `tddy-agent-backend`, and clippy is clean.
- **Mint port.** The backend does not depend on `tddy-remote-shell`, so the mint goes through a port, `AgentOnTarget`, plus `ConversationIdAdapter::mint_on_target`. The implementation lives in `tddy-agent-launch`, and no dependency is added. `ConversationStart.target` carries the shell and the bare command.
- **MCP reject-all keys: verify live before relying on them.** The trust write goes to `~/.claude.json` as `projects[<root>].hasTrustDialogAccepted`. The planned rejection is `disabledMcpjsonServers`. Claude 2.1.311 migrates `enabled/disabledMcpjsonServers` into the project's `.claude/settings.local.json`, which the launch already renders on the target. Green should write the rejection where a real Claude reads it and confirm that live on `mantasi/remote-dev`. If the `.claude.json` keys are ignored, the rejection moves to the rendered `settings.local.json`. The orchestrator decided this under the developer's finish-without-stopping instruction.
- **When to pre-trust.** Only for a spec whose hooks adapter is Claude's. `launch_on_target` has no agent-kind flag, so the spec decides.
- **Existing target tests move to the target's login `PATH`.** The four acceptance files that resolve the agent at a shared absolute host path switch to the new helpers at green. Their assertions do not change.
- **Flaky in the baseline:** `tddy-agent-backend/tests/spec_headless_turn_deadline::a_turn_that_never_ends_is_cancelled_at_its_deadline_and_its_whole_process_group_is_killed` failed once ("the agent never wrote agent.pid"), then passed. Flaky counts as red, so green fixes its race here.

- Bare command through the target's login shell (developer, 2026-11-18). It needs no per-target
  config and no extra probe. If the binary is missing, the error is the shell's `command not found`
  on the terminal, and that is what the start reports.
- Pre-trust over answering the dialogs (developer, 2026-11-18).
- **MCP choice: reject all** (developer, 2026-11-18). The pre-trust records every server in the
  repository's `.mcp.json` as not enabled, as the live run's Esc did; a target session uses the MCP
  servers the daemon configures. The exact `~/.claude.json` keys Claude reads for that answer are
  confirmed against a real Claude in commit 2.

## Refactoring Needed

### From @red (TDD Red Phase)

- `ConversationStart` gained `target: Option<ConversationTarget { shell, command }>`, not a bare shell: the mint needs the agent's **bare command** (`ResolvedTool::command`), and `binary` is a host path.
- `tddy-agent-backend` has no `tddy-remote-shell` dependency, so the shell-run mint goes through a port of its own, `AgentOnTarget::run(args, deadline)`; `tddy-agent-launch` implements it over `RemoteShell` (cd worktree, login shell, `env -i`, bare command). `ConversationIdAdapter::mint_on_target` is the new method.
- The pre-existing acceptance tests (`remote_agent_launch_acceptance.rs`, `remote_hooks_acceptance.rs`, `coder_agent_launch*_acceptance.rs`) resolve the agent at an absolute host path that the stand-in target shares. After the fix they need the agent on the target's login `PATH` (`a_target_with_a_login_of_its_own`, `an_agent_that_stays_up_on`).
- `target_trust_acceptance.rs` duplicates a small stand-in ssh target: belongs to `target-test-kit`.
- Claude 2.1.311 migrates a project's `enabledMcpjsonServers` / `disabledMcpjsonServers` out of `~/.claude.json` into the project's `.claude/settings.local.json` at startup (`migrateEnableAllProjectMcpServersToSettings`). The legacy keys are still honoured through that migration, but writing `disabledMcpjsonServers` into the `settings.local.json` the launch already renders is the durable route. Confirm with a real Claude before green.

## TODO

- [x] Record initial discovery
- [x] Create PRD
- [x] Create changeset
- [x] Draft PR contract: failing tests (commit 2)
- [x] USER REVIEW: acceptance tests (6e) — pre-approved under the developer's "finish the remaining ones without stopping" (2026-11-18)
- [ ] TDD Green
- [ ] Validate changes
- [ ] Wrap

### From @green

- **Clone root is derived, not plumbed** (orchestrator, under the user's finish-without-stopping instruction): `clone_root_of_worktree(worktree, session_id)` reads `<root>/.worktrees/<session id>`, the layout `RemoteCheckout` documents; any other layout gives `None` and trusts nothing. Used by `harness_launch` and `tddy-session-driver`.
- **Spec's own binary on a target**: `plan_the_launch` renders the argv with `tool.binary = tool.command`; the node launcher is `None` on a target.
- **Environment on a target**: only the spec's `pass_env` values and the launch's own variables travel; the base set (`HOME`, `PATH`, `TMPDIR`) is expanded by the target's login shell. A chat mint on a target uses the same form and takes the last stdout line as the id (login banners).
- **Trust failure** is `RemoteLaunchError::Trust`; it fails the start before any terminal is registered.
- **Local mint into a missing directory** now names the directory.
- **Flaky deadline test**: the cause was the first-execution scan of a fresh stub eating the 3 s turn deadline; the stub is now warmed up before the turn.
- MCP key location still to confirm live (see above).
