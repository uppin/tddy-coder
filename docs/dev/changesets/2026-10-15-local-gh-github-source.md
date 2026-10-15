# 2026-10-15 — Local gh as a project's GitHub source

**Type:** Feature

A project can be set to **Local gh**: its sessions take their GitHub identity from the host's own `gh`
login, as the session's OS user, instead of from a vault account. Every reader of a token asks `gh` in
its own process; no token crosses tddy's session socket. A forwarded session start carries the source.
Behaviour: [per-project GitHub identity](../../ft/daemon/github-identity.md#local-gh),
[project concept](../../ft/daemon/project-concept.md#local-gh),
[projects screen](../../ft/web/projects-screen-multi-host.md#local-gh),
[Telegram](../../ft/daemon/telegram-session-control.md). PR #622.

## What changed

- **Wire (`tddy-service`).** `ProjectEntry.local_gh = 9`, `SetProjectAccountsRequest.local_gh = 5`,
  `StartSessionRequest.github_source = 40`, `enum GithubSource`, `GetHostToolingRequest.github_hostname =
  3`. No new RPC, so `RESIDUAL_METHODS` is unchanged.
- **Storage (`tddy-projects`).** `ProjectData.local_gh`; `set_project_accounts` takes the flag and
  refuses it with a GitHub assignment.
- **Resolution (`tddy-accounts`, `tddy-host-service`).** `local_gh::github_hostname_of` (validated host)
  and `local_gh_git_identity`; `GithubCli::{logged_in_user, auth_token}` as an OS user;
  `HostToolingProbe::probe` takes the GitHub host.
- **Session start (`tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-changeset`).** The source is the
  request's, else the session's `github-source.yaml`, else the host's row. `SessionIdentity.is_local_gh`
  keys `relayed_by`, the sandbox guard: a Local gh identity relayed by another OS user carries only a
  logged-out `GH_CONFIG_DIR`.
- **Token readers (`tddy-toolcall`, `tddy-tools`, `tddy-coder`, `tddy-daemon-rpc`).**
  `SessionTokenSource` and `TDDY_GITHUB_SOURCE`; `RepointPlannedPr` asks `gh` as the session's OS user.
- **Environment hygiene (`tddy-pty`, `tddy-tool-engine`, `tddy-daemon-kernel`, `tddy-spawn`).** The
  four GitHub token variables are removed from PTY spawns, the shell relay and `spawn_as_user`; the
  spawn wire and supervisor policies admit `TDDY_GITHUB_SOURCE`.
- **Telegram (`tddy-telegram-control`).** A Local gh project's start carries the `gh` identity and no
  "start from the dashboard" notice.
- **Web (`tddy-web`).** The Local gh toggle, the hidden picker, per-host `gh` state; `ProjectsScreen`
  split into `ProjectLocalGh.tsx` and `ProjectAccountAssignment.tsx`.
- **Test support (`tddy-testing-commons`).** `a_gh_cli` stub builder and the `ProcessEnv` environment
  guard.

## Backlog entries touched

- `2026-09-06-no-code-path-issues-gethosttooling` — **narrowed**: the Projects card is now a production
  caller of `GetHostTooling`; the Hosts screen still does not mount `HostRowTooling`.
- `2026-10-08-keyring-github-identity-oversized-files` — **untouched**, still open.
- `2026-10-04-keyring-assignments-grew-project-coordinate-handlers` — **untouched**, still open
  (the file grew again; see the deferral below).
- `2026-10-14-project-account-assignment-reaches-one-host` — **kept**: the toggle rides on
  `SetProjectAccounts`, so it reaches one host; only a forwarded start carries the source.
- `2026-10-15-local-gh-child-sessions-and-telegram-gh` — **new**: child sessions read this host's row;
  Telegram builds its own `GithubCli`; blocking `gh` calls on async start paths; one unidentified
  Cypress flake in `ProjectLocalGh.cy.tsx`.
- `2026-10-15-local-gh-oversized-files-deferred` — **new**: the files this change grew past budget.

## Final measurements

- **File-length gate.** Fixed: `svc_start_session_core.rs` 504 → 493 and `ProjectsScreen.tsx` 585 → 458
  production lines. Grew, and deferred with consent: `ports.rs` 808 → 819, `coordinate_handlers.rs`
  650 → 658, `host_tooling.rs` 759 → 775 (new record), `service.rs` 934 → 937, `run.rs` 2,673 → 2,678.
- **Scoped runs.** `tddy-agent-launch` 63; `tddy-session-lifecycle` lib 112 + `tool_session_local_gh`
  10; `tddy-host-service` lib 157; `tddy-toolcall` `github_*` 4 + 3; `tddy-accounts` 12 + 8;
  `tddy-daemon-rpc` `set_project_local_gh` 4 + `repoint_local_gh_token` 3; Cypress `ProjectLocalGh`
  6/6, `ProjectsScreenAcceptance` 13/13, `ProjectAccountsAcceptance` 8/8.
- **Pre-existing failures.** 16 in `tddy-session-lifecycle`: the sandbox RPC bridge is not installed in
  that harness; unrelated to this change.
- Whole-workspace health is CI's answer, not a local run.
