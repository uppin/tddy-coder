# 2026-10-22 — Sandboxed CLI sessions on a provider account

**Type:** Feature + Fix

`#provider-route` 9/9 ([#670](https://github.com/uppin/tddy-coder/pull/670)), across
`tddy-sandbox-recipes` (routed runner plans: secrets via `TDDY_SECRET_*`, route env, owner-only route
files and `OPENCODE_CONFIG`, no OAuth token or inbound port beside a route; `OpenCodeCli` / `MuseCli`
recipes), `tddy-sandbox` (`SecretSource` redacted in `Debug`), `tddy-sandbox-runner`
(`--agent-kind opencode | muse` in `harness_pty.rs`), `tddy-daemon-sandbox`
(`SandboxRunnerSpawn.route`), `tddy-agent-launch` (routed sandboxed claude-cli start and relaunch;
jailed OpenCode and Muse start and resume, `cli_harness_jail.rs`), `tddy-sandbox-app` (parses both
new kinds and refuses them) and `tddy-session-lifecycle` (the jail acceptance suite).

Decisions: a sandboxed OpenCode or Muse start with a non-blank first prompt is refused, because the
runner hosts none. One with no provider account is refused up front ("a sandboxed <harness> session
needs a provider account"), because the jail holds no credentials of its own. A recorded account
survives a subagent warm-up failure (the record is written before warm-up). The workspace tool jail
and the action runner pass route `None`, since neither hosts an agent.

Pre-existing sandbox-relay defects from #481, fixed here at the developer's request: the runner
acknowledges a terminal subscription (a PTY-less jail never attached); both halves of a finished
`SessionChannel` let go of it (`WeakSender` on the host reader, `release_outbound` in the jail), so a
runner exits after its command; and the stdio acceptance fixture keeps its fake agent alive and
terminates its jail on drop (as does `tddy-daemon-rpc`'s `in_jail_conversation_acceptance`, which had the same `/bin/sleep` fixture and orphaned a runner when it failed).

Restructuring at wrap: `svc_start_sandboxed_cli_harness_session.rs` 560 → 390 (the pure helpers to
`cli_harness_jail.rs`, and `bridge_and_register_cli_harness_jail` extracted by the engine);
`svc_start_sandboxed_claude_cli_session.rs` held at 493 through the shared `canonical_exec_path`.

Backlog: **added** `2026-10-22-sandbox-app-cannot-start-opencode-or-muse`.

Code issues: **added, open**
`tddy-agent-launch/…/complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session`
(250 lines; the engine refused the remaining extractions). **Recorded, pre-existing over budget,
grown**: `tddy-sandbox-app` `oversized-file-spawn` (788 → 823), `oversized-file-main` (973 → 976),
`oversized-file-sandboxed-session` (736 → 737); `tddy-sandbox` `oversized-file-builder` (562 → 573).
**Regressed**: `oversized-file-runner` (2,639 → 2,698), `oversized-file-host-relay` (949 → 971),
`complexity-host-relay-run-host-relay-inner` (263 → 282), `oversized-file-sandbox-session`
(917 → 922), `oversized-file-workspace-tool-sandbox` (739 → 741). **Touched**:
`heavy-params-sandbox-session-dial-and-bridge`.

Docs: [Claude CLI § provider account](../../ft/daemon/claude-cli-session.md#running-on-a-provider-account),
[OpenCode § Sandbox mode](../../ft/daemon/opencode-cli-session.md#sandbox-mode),
[Muse Code § Sandbox mode](../../ft/daemon/muse-cli-session.md#sandbox-mode),
[tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md),
[tddy-agent-launch module layout](../../../packages/tddy-agent-launch/docs/module-layout.md).
