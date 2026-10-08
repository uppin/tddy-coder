# 2026-10-08 — Git and GitHub operations resolve the project's account

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)

Every git and GitHub operation the daemon performs for a session uses **the account the project is
assigned** — token and commit identity, from one resolution. No environment fallback exists: nothing reads
`GITHUB_TOKEN` or `GH_TOKEN`, and a project with no usable account is refused with a reason of its own.
Behaviour: [per-project GitHub identity](../../ft/daemon/github-identity.md). Mechanism:
[identity resolution](../../../packages/tddy-accounts/docs/github-identity-resolution.md) and
[session identity](../../../packages/tddy-session-lifecycle/docs/session-identity.md).

## What changed

- **Resolution (`tddy-accounts`).** `acting_identity` returns one `ActingIdentity { account, token, git }`;
  `IdentityError` has a message per outcome plus `Unusable`. The identity derives from provider metadata, never
  the mutable label.
- **No environment (`tddy-github`, `tddy-workflow-recipes`, `tddy-tools`, `tddy-pr-stack`).**
  `github_token_from_env`, `github_env_token_present` and `TokenSource::ProcessEnv` are deleted; REST entry
  points take `token: &str`. `RealGithubPrApi` gains `with_api_base`, `asking` and `asking_the_session_host`.
  Pinned structurally by `no_crate_still_resolves_a_github_token_from_the_process_environment`
  (`tddy-daemon-auth`).
- **Asked, never delivered (`tddy-toolcall`, `tddy-tools`, `tddy-workflow-recipes`).** The token is requested
  per call over the `github-token` verb on the session's toolcall socket. The PR-stack tasks ask only when an
  action reaching GitHub runs. Prompt awareness reads `github_pr_tools_available` from the context
  (`tddy-presenter`, `tddy-workflow`).
- **Session start (`tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-daemon-livekit`).** One
  `session_identity` lookup binds the commit pairs and a token handler pinned to the start's outcome on every
  spawn path the daemon owns; `RepointPlannedPr` resolves the account itself. A refused resolution still
  starts the session with no pairs (developer-consented; not a token fallback).
- **Tool sessions (`tddy-spawn`, `tddy-host-service`, `tddy-coder`, `tddy-session-lifecycle`).** The four
  commit-identity pairs ride the spawn wire; the token is relayed over one owner-only socket per OS user with
  a session registry, a stop watch, and a resume message after a daemon restart. `spawn_as_user` no longer
  lets a child inherit `GITHUB_TOKEN` / `GH_TOKEN`.
- **Supervised hosts (`tddy-supervisor`, `tddy-daemon`).** `supervisor.yaml` declares `host_sockets`; the root
  supervisor makes each socket and hands it to the daemon from descriptor 4. Operators must list each session
  user and allow the four git identity environment keys (`supervisor.yaml.production`, `dev.supervisor.yaml`,
  `daemon.yaml.production`); without them every tool-session spawn for an account-resolving project is
  refused, loudly.
- **Telegram (`tddy-telegram-control`).** A Telegram-started session has no session token, so no account is
  available to it; the chat is told so. Tool sessions started there get no host-session socket
  (`TODO(stdio-relay)`, `workflow_spawn.rs`).
- **Product docs.** `docs/ft/daemon/github-identity.md` is new; `docs/ft/coder/github-pr-tools-mcp.md`,
  `project-concept.md` and `telegram-session-control.md` describe the account-based behaviour.

## Deferred, with the developer's consent

- **The supervisor-declared host socket has never run under a real root supervisor.** Built and unit-proven
  with the current user standing in; the `chown` to a different uid, the daemon accepting on a socket it does
  not own, the real descriptor hand-over and the three Linux-only tests in
  `tddy-supervisor/tests/supervisor_socket_handoff.rs` were not exercised. The acceptance criterion is
  marked `[~]`. Backlog entry kept: "the supervisor-declared host socket has never run under a real root
  supervisor" (2026-10-08).
- **Seven files grew while over the 500-production-line budget.** Backlog entry kept: "`#keyring` 9/9 left
  files over the 500-production-line budget" (2026-10-08). Code-issue records re-measured below.
- Still not built: re-attach of a running tool session after a daemon restart (needs the owner's session
  token, which is not durable by design); a Telegram tool-session socket; a PR-tool token for plain
  cursor-cli (no toolcall listener); sandboxed cursor-cli resume.

**Backlog entries resolved by this change: none.** The changeset claimed no `docs/dev/todo/` entry and no
code-issue record; the two entries above are this change's own deferrals.

## Code issues re-measured

Production lines, inline-test-block rule (a naive count to the first `#[cfg(test)]` for files with no
inline test block), `origin/master` → branch. Every record is **kept**, marked regressed, with a
Measurement-history row:

| File | Before → after |
|---|---|
| `tddy-toolcall/src/toolcall/listener.rs` | 676 → 766 |
| `tddy-daemon-rpc/src/pr_stack/ports.rs` | 775 → 808 |
| `tddy-presenter/src/presenter/workflow_runner.rs` | 1,015 → 1,036 |
| `tddy-daemon/src/runtime.rs` | 1,776 → 1,781 |
| `tddy-telegram-control/.../session_start.rs`, `pickers.rs` | 711 → 713, 705 → 706 |
| `tddy-tools/src/server.rs` | 2,808 → 2,831 |
| `tddy-github/src/pr_api.rs` | 922 → 1,018 (gate count 315 → 393) |
| `tddy-session-lifecycle/src/connection_service.rs` | 509 → 510 |

`tddy-supervisor/src/supervisor.rs` (948 → 965) and `tddy-session-lifecycle/src/cursor_cli_spawn.rs`
(532 → 548) have no oversized-file record and none was created; both are in the deferred backlog entry.
`tddy-coder/src/run.rs` shrank (4,010 → 3,976 by the inline-test-block rule); its record, measured by
another count, is unchanged.

## Verification

Scoped, not whole-workspace. Over the touched packages, 3,569 passed and 25 failed, every failure
pre-existing or environmental: 16 *sandbox RPC bridge not installed*, 7 in `session_sync_livekit_acceptance`
and `session_agent_remote_acceptance`, `pr_stack_artifact_paths_acceptance` (`/tmp` vs `/private/tmp`), and one
LiveKit testkit *container startup timeout*. Scoped `clippy -D warnings --all-targets` is clean on the
touched packages. CI `Rust lint` and `Rust build (arm64)` pass on `a0a63e11f`; the x86 Rust build and tests
were still running. New tests were mutation-checked where noted in the package docs; a few guards were
written with their code and never seen red.
