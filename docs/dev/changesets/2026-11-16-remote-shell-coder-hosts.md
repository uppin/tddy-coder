# 2026-11-16 — Coder workspaces are hosts that run sessions

**Type:** Feature · `#remote-shell` 7/7, PR [#921](https://github.com/uppin/tddy-coder/pull/921), base `feature/remote-shell/remote-hooks`, wave 4 of 4

The workspaces of each connected Coder account appear as hosts derived from the daemon, with their status. Choosing a running one starts a session whose agent runs in that workspace, through the `coder` CLI, with nothing tddy-specific installed there.

## What changed

- **tddy-service**: `CoderTarget { account_id, owner, workspace, agent }` as `SessionTarget.coder = 2`; `DERIVED_HOST_KIND_CODER = 2`; `DerivedHost.usable = 6`, `DerivedHost.unusable_reason = 7`. TypeScript regenerated for `tddy-client-web` and `tddy-rust-typescript-tests`.
- **tddy-changeset**: `SessionTarget::Coder { account_id, owner, workspace, agent: Option }` with `SessionTarget::coder(..)` and `.on_agent(..)`.
- **tddy-host-service**: `CoderDerivedHosts`, a second `DerivedHostSource` built per call for one subject over the new `CoderAccountSource` port. A workspace is usable when running and the daemon has the `coder` CLI; a failed account is one unusable row (`<parent>/coder/<account_id>`, its label, the reason, no target). At wrap its `list` (82 lines) was split into `hosts_of` / `workspace_host` / `account_host`.
- **tddy-agent-launch**: `coder_target::shell_for_coder_target` resolves a Coder target to a `CoderShell` from the caller's vault (`CoderTargetError` is this PR's own type; the parent's `TargetError` gains no variant). A stopped workspace is refused at launch by the CLI's own complaint; a locked vault on resume is `FailedPrecondition` and the session is kept. The project-home start refuses a Coder target as it refuses an SSH one, now pinned by a test for both kinds.
- **tddy-daemon**: `VaultCoderAccountSource`, the `CoderAccountSource` adapter over the daemon's vaults, with its own tests; the session host carries the Coder credential source.
- **Web** (`tddy-client-web`, `tddy-hosts-web`, `tddy-session-create-web`): usability is read from a non-empty `unusable_reason`; a workspace row shows `Available` / `Unavailable` with its reason, and the Host field disables an unusable derived host and shows its reason.

## Verification

Scoped (`CARGO_INCREMENTAL=0`): `coder_agent_launch_acceptance` 4/4, `coder_derived_hosts_acceptance` 6/6, `coder_target_session_acceptance` 3/3, the daemon's `VaultCoderAccountSource` tests 5/5; full `tddy-agent-launch`, `tddy-host-service` and `tddy-changeset` suites green. Cypress component: `CoderHostsAcceptance` 2/2 and `CreateSessionCoderHostAcceptance` 2/2. `cargo clippy` on the touched Rust packages with `--all-targets -- -D warnings` and `cargo fmt` clean. The one failure in the wider lifecycle run is the known local-only macOS `/tmp` vs `/private/tmp` assertion in `claude_cli_from_spec_acceptance`, in a suite this PR does not touch. Whole-workspace health is CI's.

## File length

- 🔴 `packages/tddy-agent-launch/src/opencode_cli_spawn.rs` 476 → 529 production lines: this PR pushed it past 500. **Split at this wrap**: `CliSessionStart` and `shell_for_session_target` moved to `opencode_cli_spawn::session_start` by `tddy-tools restructure` (`move_item`, `check --deep` clean) — 529 → 422 production lines, the new module at 112.
- 🔴 `packages/tddy-agent-launch/src/remote_agent_launch.rs` 546 → 579 and `packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs` 533 → 544: already over budget, grown here. Left for the sweep after the stack lands and re-measured in their existing oversized-file records, as the lower nodes recorded.

## Code issues

No code issue is claimed by this PR, and none in a touched package carries another PR's claim. No record was added by this PR; the pre-existing records on code this PR touched were re-measured in place (`start_cli_harness_session` 347 → 368, `resume_cli_harness` 95 → 108, `resume_cli_harness_session` 157 → 166, `start_cli_harness_from_request` 98 → 100, `DaemonSessionHost::new` 99 → 100, `build_session_host` 73 → 82, `start_sandboxed_cli_harness_session` 291 → 292, `resumed_session_wiring` and `wire_session_agent` unchanged, `connection_service.rs` +6 by diff). They stay open for their recorded sweeps.

## Backlog

No `docs/dev/todo/` entry is resolved here; the changeset's ⚠ DURING entry (`2026-10-14-project-account-assignment-reaches-one-host.md`) stays — a Coder target names its account directly. One entry is added: `2026-11-16-coder-hosts-never-run-against-a-real-deployment.md` — nothing in this stack was run against a real Coder deployment or the real `coder` CLI, and the four facts pinned only by stand-ins are listed there. This is the top node of the stack: the stack's whole `docs/dev/todo/` and code-issues delta is listed in the PR for the sweep node the orchestrator planned.
