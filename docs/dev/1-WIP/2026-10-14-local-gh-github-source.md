# Changeset: Local gh as a project's GitHub source

**Date**: 2026-10-14
**Status**: 🚧 In Progress — 🟡 implementation written for M1–M10; **WIP draft, checks skipped** — scoped runs reported passing per package by the implementing agents, not yet confirmed by CI

> **Implementation state (2026-10-15, after /pr-wrap gate work).** M1–M10 are implemented.
> Scoped runs: `tddy-agent-launch` + `tddy-daemon-rpc` + `tddy-session-lifecycle` 739 passed / 16 failed;
> the 16 are the known "sandbox RPC bridge not installed" harness gap (sandbox suites), not this change.
> Cypress: `ProjectLocalGh` 6/6, `ProjectsScreenAcceptance` 13/13, `ProjectAccountsAcceptance` 8/8.
> Scoped clippy `--all-targets -D warnings` clean on agent-launch, session-lifecycle, daemon-rpc.
>
> **Fixed since the draft:** the relay-guard hole (`SessionIdentity.is_local_gh`, test-first); sidecar-write and
> repoint tests added (`repoint_local_gh_token.rs`); red-phase test fmt/clippy hygiene;
> `svc_start_session_core.rs` 504 → 493 and `ProjectsScreen.tsx` 585 → 458 production lines.
>
> **Still open (⚠):**
> - Child sessions (conversations, stack children) read this host's row, not the parent's forwarded source.
> - Telegram builds its own `GithubCli`; `with_github_cli` does not reach it.
> - One unidentified flake seen once in `ProjectLocalGh.cy.tsx` (not reproduced in later runs).
> - `session_sync_livekit_acceptance` needs `tddy-remote-git-repo` built (unrelated).
> - Oversized files this change grew, deferred with consent: see
>   [2026-10-15-local-gh-oversized-files-deferred.md](../todo/2026-10-15-local-gh-oversized-files-deferred.md).
> - Validation steps (/validate-changes, /validate-tests, /validate-prod-ready, /analyze-clean-code) and the docs wrap have **not** run.
**Type**: Feature

## Initial Discovery

[2026-10-14-local-gh-github-source-initial-discovery.md](./2026-10-14-local-gh-github-source-initial-discovery.md)
— three passes: feature docs and record scan; identity, token, sandbox and web paths; propagation and
`StartSession` forwarding.

## Prerequisites

### ⛔ BLOCKING — `GH_TOKEN` / `GITHUB_TOKEN` reach session processes outside `tddy-spawn` — discovery § Combined Conclusions 5

No TODO file records this; discovery found it. Only `tddy-spawn/src/spawner.rs:1149-1157` removes
the two variables.
- **Who inherits them:**
  - PTY agent spawns (`tddy-pty/src/runtime.rs:261-270`);
  - the host-side shell relay (`tddy-tool-engine/src/contained_shell.rs:48-85`);
  - every `spawn_as_user` child (`tddy-daemon-kernel/src/spawn_as_user.rs:186-193`).
- **Why it blocks:** `gh` prefers `GH_TOKEN` (and `GH_ENTERPRISE_TOKEN` for an Enterprise host) over
  its own login. Every Local gh path would then silently act as whoever exported the variable.
- **Why the routes around it are wrong:**
  - Ignoring it ships exactly the wrong-account hole the feature exists to avoid.
  - Scrubbing only around our own `gh` calls leaves the agent's own `gh` exposed.
- Small and in this change's path, so it is fixed here: see `## Scope`.

### ⚠ DURING — `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`

- **State:** 650 production lines against a budget of 500; **unclaimed**. Its TODO is
  [2026-10-04-keyring-assignments-grew-project-coordinate-handlers.md](../todo/2026-10-04-keyring-assignments-grew-project-coordinate-handlers.md).
- **This change:** `set_project_accounts_at_project_coordinate` gains a few lines: the `local_gh`
  field and its conflict refusal. There is deliberately **no new RPC and no new handler**: the
  toggle rides on `SetProjectAccounts`.
- **Constraint:** the conflict refusal lives in `tddy-projects::project_storage` and the handler maps
  it; it must not be duplicated a third time.
- Recorded, not fixed here.

### ⚠ DURING — `docs/dev/todo/2026-10-08-keyring-github-identity-oversized-files.md` — [link](../todo/2026-10-08-keyring-github-identity-oversized-files.md)

- **State:** `tddy-toolcall/src/toolcall/listener.rs` is 766 lines.
- **This change:** the local-gh token read goes in `github_token_client.rs`, **not** in
  `listener.rs`. The listener is untouched.
- `tddy-daemon-rpc/src/pr_stack/ports.rs` is 808 lines and its repoint closure changes. The gh
  branch is a call into `tddy-agent-launch::local_gh_github_token`, so it adds a handful of lines.

### ⚠ DURING — `docs/dev/todo/2026-09-06-no-code-path-issues-gethosttooling.md` — [link](../todo/2026-09-06-no-code-path-issues-gethosttooling.md)

- **This change:** the Projects card becomes the **first production caller** of `GetHostTooling`.
  It asks for each host owning a Local gh project, for the project's GitHub host.
- **What stays open:** the Hosts screen still does not mount `HostRowTooling`. The entry stays open
  and is narrowed at wrap to the Hosts screen.

### — Considered, unrelated

- `packages/tddy-workflow-recipes/docs/code-issues/squatting-pr-stack-data-model.md` is unclaimed and
  is about the data model, not the token. The only claimed-field hit in scope, it is not in this
  change's path.
- [2026-09-18-desktop-install-configures-no-identity.md](../todo/2026-09-18-desktop-install-configures-no-identity.md)
  is about dashboard sign-in, which this change does not touch. Local gh does give a desktop a session
  identity without a vault, which is worth telling its owner, but it does not close the entry.

### Packages with no `docs/code-issues/`

These have not been analyzed, which is not the same as clean: `tddy-accounts`, `tddy-projects`.

## Affected Packages

- **tddy-service**: [README](../../packages/tddy-service/README.md) — `ProjectEntry.local_gh = 9`,
  `SetProjectAccountsRequest.local_gh = 5`, `StartSessionRequest.github_source = 40`,
  `enum GithubSource` (types.proto), `GetHostToolingRequest.github_hostname = 3`.
- **tddy-projects**: [project-service.md](../../packages/tddy-projects/docs/project-service.md) —
  `ProjectData.local_gh`; `set_project_accounts` takes the flag and refuses a GitHub assignment with
  it.
- **tddy-changeset**: session sidecar `github-source.yaml` (`SessionGithubSource`).
- **tddy-accounts**: [github-identity-resolution.md](../../packages/tddy-accounts/docs/github-identity-resolution.md)
  — `local_gh` module: GitHub host of a git URL, and the noreply identity of a `gh` login.
- **tddy-host-service**: [host-tooling-probe.md](../../packages/tddy-host-service/docs/host-tooling-probe.md)
  — `GithubCli` (who `gh` is logged in as, and its token, as an OS user); the tooling probe asks
  per hostname.
- **tddy-daemon-kernel**: `spawn_as_user` removes the GitHub token variables.
- **tddy-daemon-livekit**: `session_git_environment_of(&GitIdentity)`.
- **tddy-agent-launch**: [module-layout.md](../../packages/tddy-agent-launch/docs/module-layout.md)
  — Local gh session identity, relay guard, `local_gh_github_token`, forwarded start carries the
  source, sidecar write.
- **tddy-session-lifecycle**: [session-identity.md](../../packages/tddy-session-lifecycle/docs/session-identity.md)
  — `DaemonSessionHost::with_github_cli`; identity reads the requested source, the sidecar, or the row.
- **tddy-daemon-rpc**: `SetProjectAccounts` carries `local_gh`; `RepointPlannedPr` reads `gh` for a
  Local gh session; `GetHostTooling` passes the hostname.
- **tddy-toolcall**: `SessionTokenSource`; `TDDY_GITHUB_SOURCE`; the token client reads `gh` in its
  own process.
- **tddy-tools**: PR tools take the token through `SessionTokenSource`.
- **tddy-coder**: GitHub PR tools are offered to a Local gh session.
- **tddy-spawn**: [commit-identity.md](../../packages/tddy-spawn/docs/commit-identity.md) — the
  spawn wire admits `TDDY_GITHUB_SOURCE`; the supervisor policy lists it.
- **tddy-pty**, **tddy-tool-engine**: remove the GitHub token variables.
- **tddy-telegram-control**: [architecture.md](../../packages/tddy-telegram-control/docs/architecture.md)
  — a Telegram start of a Local gh project carries the `gh` identity; no "start from the dashboard"
  notice.
- **tddy-testing-commons**: `a_gh_cli` stub builder.
- **tddy-web**: [projects-screen.md](../../packages/tddy-web/docs/projects-screen.md) — the Local gh
  toggle, the hidden picker, and per-host `gh` state.

## Related Feature Documentation

- [PRD-2026-10-14-local-gh-github-source.md](../../ft/daemon/1-WIP/PRD-2026-10-14-local-gh-github-source.md)
- [Per-project GitHub identity](../../ft/daemon/github-identity.md),
  [Project concept](../../ft/daemon/project-concept.md),
  [Projects screen (multi-host)](../../ft/web/projects-screen-multi-host.md),
  [Telegram session control](../../ft/daemon/telegram-session-control.md)

## Summary

A project can be set to **Local gh**. Its sessions then derive their commit identity from the host's
`gh` login, for the project's GitHub host. Every reader of a GitHub token asks `gh` in its own
process, as the session's OS user: the agent, tddy's PR tools, the workflow, and the daemon-side
repoint. No token crosses tddy's session socket. A forwarded session start carries the source to the
target host.

## Background

Today the vault is the only source of a session's GitHub identity
([github-identity.md](../../ft/daemon/github-identity.md)). People with `gh auth login` already done
on every host want tddy to use it, without linking an account through the device flow and without
a vault to keep unlocked. Telegram-started sessions cannot open a vault at all. Developer decisions,
from the planning interview, are listed in `## Decisions & Trade-offs`.

## Scope

- [ ] **⛔ Env hygiene**: `GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN` and
      `GITHUB_ENTERPRISE_TOKEN` are removed from PTY spawns, the shell relay and `spawn_as_user`.
- [ ] **Project toggle**: storage, wire and `SetProjectAccounts`, with the conflict refused.
- [ ] **Identity from gh**: GitHub host of a git URL; `gh api user` as the OS user; four commit pairs
      plus `TDDY_GITHUB_SOURCE`; the start continues with a reason when `gh` cannot answer.
- [ ] **Token readers**: the token client, `tddy-tools`, the workflow and `tddy-coder` read `gh`;
      `RepointPlannedPr` reads `gh` as the session's OS user.
- [ ] **Forwarded start and resume**: `StartSessionRequest.github_source`, filled by the forwarding
      host; the sidecar `github-source.yaml`.
- [ ] **Sandbox guard**: the relay carries the identity only when it runs as the session's OS user;
      otherwise it carries a logged-out `GH_CONFIG_DIR`.
- [ ] **Telegram**: a Local gh project's Telegram start carries the `gh` identity.
- [ ] **Web**: the toggle, the hidden picker, and per-host `gh` state from `GetHostTooling`.
- [ ] **Package Documentation**: wrapped at the end.
- [ ] **Testing**: all acceptance and unit tests below pass; scoped runs per package.
- [ ] **Code Quality**: scoped clippy per package; `cargo fmt`.

## Technical Changes

### State A (Current)

- **Identity source.**
  - `tddy_accounts::acting_identity` resolves a project's GitHub identity from the vault only:
    `ActingIdentity { account, token, git }`.
  - `session_git_environment(&ActingIdentity)` renders the four `GIT_*` pairs.
  - `SessionAccountAccess::session_identity(session_id, accounts)` yields `SessionIdentity {
    git_environment, github_credential }`. A refusal logs a warning and the session still starts.
- **Assignment storage.** The assignment is read from **this host's** `projects.yaml` at start and
  at resume. `SetProjectAccounts` writes one host. A forwarded `StartSession` carries no source.
- **Token readers.**
  - The token is asked for per call over `github-token`, served by the session listener or the
    host-session socket.
  - `tddy-tools` and the workflow use `tddy_toolcall::request_github_token`, which needs
    `TDDY_SOCKET`.
  - `RepointPlannedPr` calls `project_github_token` over the vault.
- **Environment hygiene.**
  - `GH_TOKEN`/`GITHUB_TOKEN` are removed only by `tddy-spawn`.
  - The spawn wire admits only the four `GIT_*` keys.
  - Jails start with `env -i`. Their shell commands run host-side as the daemon's user.
- **Host probe.** `GetHostTooling` probes `gh auth status` (any host) as the OS user. Nothing in the
  web app calls it.
- **Telegram.** Starts carry no identity and send a "start from the dashboard" notice when an account
  is assigned.

### State B (Target)

- **Toggle.** A project row has `local_gh: bool` (written only when true). `SetProjectAccounts`
  carries it with the accounts in one replace-write. A GitHub assignment together with `local_gh` is
  refused (`InvalidArgument`) by `tddy-projects`, naming both. `ProjectEntry` carries it.
- **Which source a session uses** is decided at start and resume:
  1. `StartSessionRequest.github_source`, when set (a forwarded start), is used and recorded in the
     session's sidecar `github-source.yaml`;
  2. otherwise, on resume, the sidecar if present;
  3. otherwise this host's row (`local_gh` → Local gh, else the vault assignment, as today).

  A forwarding host fills an unset `github_source` from its own row before forwarding.
- **Local gh identity.**
  - `tddy_accounts::local_gh::github_hostname_of(git_url)` gives the GitHub host (https, ssh-scp and
    `ssh://` forms); an unreadable URL is a refusal, never a default.
  - `tddy_host_service::GithubCli::logged_in_user(os_user, hostname)` runs
    `gh api user --hostname H` as the OS user, with the token variables removed and a timeout.
  - `local_gh_git_identity(login, id)` gives the noreply identity.
  - `SessionIdentity.git_environment` gets the four pairs plus `TDDY_GITHUB_SOURCE=local-gh:H`.
  - When `gh` cannot answer, the pairs are absent, the source entry is still present, and the reason
    is logged.
  - `github_credential` is a handler that **refuses** with a Local gh reason. Nothing that reads the
    source entry asks it; a jailed `tddy-tools`, which cannot see that entry, is answered with the
    reason.
- **Token readers.**
  - `tddy_toolcall::SessionTokenSource::from_environment()` gives `LocalGh { hostname, gh_program }`
    when `TDDY_GITHUB_SOURCE` is set, and `HostSocket(TDDY_SOCKET)` otherwise.
    `request_github_token_from(&source)` runs `gh auth token --hostname H` for Local gh and **never**
    asks the socket. A malformed value is an error, not a fallback.
  - `tddy-tools`, the workflow and `tddy-coder`'s tool availability use it.
  - `RepointPlannedPr` uses `local_gh_github_token(&GithubCli, os_user, git_url)` when the session
    (sidecar, else row) is Local gh.
- **Sandbox guard.** `SessionIdentity::relayed_by(RelayUser, logged_out_gh_config)` keeps the
  identity when the relay runs as the session's OS user. For a Local gh identity relayed by another
  user, it yields only `GH_CONFIG_DIR=<empty dir>`.
- **Environment hygiene.**
  - Token variables are removed in `tddy-pty`, `tddy-tool-engine` and `spawn_as_user`.
  - The spawn wire admits `TDDY_GITHUB_SOURCE`, and the rendered supervisor policies allow it.
- **Host probe.** `GetHostToolingRequest.github_hostname` makes the probe run
  `gh auth status --hostname H`.
- **Telegram.** A Local gh project's Telegram start resolves the same identity through `GithubCli`
  and sends no notice.
- **Web.**
  - `ProjectCard` shows a **Local gh** checkbox. Turning it on sends `SetProjectAccounts` with the
    GitHub assignment removed and `localGh: true`.
  - While it is on, the GitHub account row is hidden, and each host row shows its `gh` state from
    `GetHostTooling { daemonInstanceId, githubHostname }`.

### Delta (What's Changing)

#### tddy-service
- `types.proto`: `enum GithubSource { GITHUB_SOURCE_UNSPECIFIED = 0; GITHUB_SOURCE_VAULT = 1; GITHUB_SOURCE_LOCAL_GH = 2; }`.
- `session.proto`: `StartSessionRequest.github_source = 40`.
- `project.proto`: `ProjectEntry.local_gh = 9`, `SetProjectAccountsRequest.local_gh = 5`.
- `host.proto`: `GetHostToolingRequest.github_hostname = 3`.
- No new RPC, so `RESIDUAL_METHODS` is unchanged.

#### tddy-projects
- `ProjectData.local_gh: bool` (`serde(default, skip_serializing_if = "is_false")`).
- Every struct-literal site gets `local_gh: false`, in one mechanical commit.
- `set_project_accounts(dir, id, accounts, local_gh)` refuses `local_gh` together with a `github`
  assignment: *"a project that uses local gh is assigned no github account; turn local gh off to
  assign one"*.

#### tddy-changeset
- `session_github_source.rs`: `SessionGithubSource { Vault, LocalGh }`,
  `write_session_github_source(dir, source)`, `read_session_github_source(dir) -> Result<Option<_>>`.
  The file is `github-source.yaml`, kept outside `.session.yaml`, so `deny_unknown_fields` is not
  disturbed and the daemon, not the child, owns it.

#### tddy-accounts
- `local_gh.rs`: `github_hostname_of`, `local_gh_git_identity`, `LocalGhError::UnreadableGitUrl`.

#### tddy-host-service
- `github_cli.rs`:
  - `GithubCli::{on_spawn_child_path, at}`;
  - `logged_in_user`, `auth_token`;
  - `GhUser { login, id }`;
  - `GithubCliRefusal::{NotInstalled, NotLoggedIn { hostname, os_user }, Failed }`, each with a
    message naming the action.
- `HostToolingProbe::probe(os_user, github_hostname: Option<&str>)`; three implementations updated.

#### tddy-daemon-kernel
- `as_user_command` removes the four GitHub token variables.

#### tddy-daemon-livekit
- `session_git_environment_of(&GitIdentity)`; `session_git_environment` delegates to it.

#### tddy-agent-launch
- `ProjectGithub::{Vault(&[AccountAssignment]), LocalGh { os_user, git_url }}`.
- `SessionAccountAccess::with_github_cli`, `project_session_identity`.
- `SessionIdentity.local_gh_hostname`, `SessionIdentity::relayed_by`, `RelayUser`.
- `local_gh_github_token`.
- `forwarded_start_request(req, local_row)`.
- The sidecar is written when the request carried a source.
- The jail launch and relaunch use `relayed_by`.

#### tddy-session-lifecycle
- `DaemonSessionHost::with_github_cli(GithubCli)`; wiring passes the source through `LaunchHost`.

#### tddy-daemon-rpc
- `SetProjectAccounts` maps `local_gh` both ways.
- The repoint closure branches to `local_gh_github_token`.
- `GetHostTooling` passes the hostname.
- `entries::project_entry_from` maps `local_gh`.

#### tddy-toolcall
- `GITHUB_SOURCE_ENV`, `local_gh_source_value`, `parse_github_source`.
- `SessionTokenSource`, `request_github_token_from`.
- `request_github_token_from_session` goes through the source.

#### tddy-tools / tddy-coder / tddy-workflow-recipes
- `github_token_for_agent`/`real_gh` use `SessionTokenSource::from_environment()`.
- `ToolHostHandlers::github_pr_tools_available` is true for a Local gh session.
- The workflow is unchanged: it already calls `request_github_token_from_session`.

#### tddy-spawn
- `require_commit_identity_only` admits `GITHUB_SOURCE_ENV`.
- The supervisor policy templates (`dev.supervisor.yaml`, install-rendered config) list it.

#### tddy-pty / tddy-tool-engine
- Both remove the four token variables before spawning.

#### tddy-telegram-control
- `telegram_git_environment(project, os_user, &GithubCli)`.
- `account_identity_unavailable_notice` returns `None` for a Local gh project.

#### tddy-testing-commons
- `gh_cli.rs`: `a_gh_cli(dir).logged_in_as(login, id).at_host(hostname).with_token(token).build()`,
  and `.logged_out()`. It writes a `/bin/sh` stub answering `api user --hostname H` and
  `auth token --hostname H`, and records its environment.

#### tddy-web
- `ProjectsScreen` props:
  - `onSetProjectAccounts` input gains `localGh`;
  - a new `loadHostGithubCli({ daemonInstanceId, githubHostname })`.
- `ProjectsAppPage` wires `HostService.getHostTooling`.
- Page object helpers: `localGhToggle`, `turnLocalGhOn`, `hostGhState`.

## Implementation Milestones

- [x] M1 Env hygiene in `tddy-pty`, `tddy-tool-engine`, `spawn_as_user` (tests pass)
- [x] M2 Proto fields + `ProjectData.local_gh` (mechanical literal commit) + `SetProjectAccounts` refusal
- [x] M3 `tddy-accounts::local_gh` + `tddy-host-service::GithubCli` + `session_git_environment_of`
- [x] M4 `SessionAccountAccess` Local gh identity; tool / claude-cli / cursor starts and resumes carry it
- [x] M5 Token readers: `SessionTokenSource`, `tddy-tools`, `tddy-coder`, repoint
- [x] M6 `StartSessionRequest.github_source`, forwarding fill, sidecar, resume
- [x] M7 Sandbox relay guard
- [x] M8 Telegram
- [x] M9 Web toggle and per-host `gh` state
- [x] M10 Docs: correct `project-concept.md` "every host" and `github-identity.md` "pushes" claims

## Testing Plan

**Levels.**
- **Integration:** the real `StartSession`/`ResumeSession` into a recording tool, and the real
  `SetProjectAccounts` handler.
- **Process-level:** a stub `gh` executable, injected by path, never by `PATH` mutation.
- **Unit:** pure resolution, the relay guard, the spawn wire and the scrubs.
- **Cypress component:** the card, with `mountWithRpc` + `anInMemoryRpcBackend`.

**Why a stub `gh` and not a trait double.** The behaviour under test includes *how* `gh` is
invoked: which subcommand, which `--hostname`, as which user, and with the token variables gone. A
script that answers only the exact argv a logged-in `gh` would, and records its environment, pins
all of that. A trait double would pin none of it.

**What is not tested here, and why.**
- A real `gh` against github.com needs a real login; that is the developer's manual check.
- The sandbox's *same-user* path end to end needs a jail.
- The guard is a pure function over the relay env (unit); the jail plumbing it feeds is already
  covered by `jail_launch_steps` tests.

**Env-mutating tests** (`GH_TOKEN` exported) live in their own test binaries or use `#[serial]`, as
the existing `GITHUB_TOKEN` tests do.

## Acceptance Tests

Written and verified failing 2026-10-14. All 30 ran; 28 fail for missing behaviour and 2 pass by design: they pin an absence that today's code already satisfies, and must keep holding after green.

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/set_project_local_gh.rs`
- `turning_local_gh_on_stores_it_and_list_projects_carries_it`
- `turning_local_gh_on_keeps_every_other_providers_assignment`
- `a_local_gh_project_assigned_a_github_account_is_refused_naming_both`
- `turning_local_gh_off_leaves_the_project_with_no_github_account`

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/tool_session_local_gh.rs`
- `a_local_gh_tool_session_commits_as_the_hosts_gh_login`
- `a_local_gh_tool_session_on_an_enterprise_host_asks_gh_about_that_host`
- `a_local_gh_tool_session_is_told_its_github_source_and_host`
- `a_local_gh_tool_session_starts_with_no_token_in_any_variable` — **passes today (pin)**
- `a_local_gh_tool_session_where_gh_is_logged_out_still_starts_with_no_identity` — **passes today (pin)**
- `a_local_gh_tool_session_where_gh_is_logged_out_is_still_told_its_github_source`
- `the_gh_a_local_gh_session_start_asks_never_sees_the_daemons_github_tokens`
- `a_start_naming_local_gh_acts_as_gh_although_this_hosts_project_was_never_set`
- `a_resumed_session_that_was_started_naming_local_gh_still_acts_as_gh`
- `a_resumed_session_of_a_local_gh_project_acts_as_gh`

### tddy-toolcall — `packages/tddy-toolcall/tests/github_token_from_local_gh.rs`
- `a_session_told_local_gh_reads_its_token_from_gh_for_that_host`
- `a_session_whose_project_lives_on_another_host_gets_no_token_from_gh`
- `gh_that_is_logged_out_is_reported_in_its_own_words`
- `a_host_with_no_gh_installed_is_reported_as_such`

### tddy-toolcall — `packages/tddy-toolcall/tests/github_source_from_environment.rs` (env-mutating, serialized by a `Mutex`)
- `a_session_told_local_gh_never_asks_its_session_host`
- `a_malformed_github_source_is_an_error_and_not_a_reason_to_ask_the_host`
- `the_gh_a_session_runs_for_its_token_inherits_none_of_its_github_token_variables`

### tddy-web — `packages/tddy-web/cypress/component/ProjectLocalGh.cy.tsx`
- "turns Local gh on and sends the remaining assignments without GitHub"
- "hides the GitHub account picker while Local gh is on"
- "shows the gh login each host has for the project's GitHub host"
- "shows a host whose gh is not logged in for the project's GitHub host"
- "asks every host of a Local gh project about its own gh"
- "turns Local gh on over the daemon and shows it after the list refreshes"

### Contract surface added with them (`todo!()` bodies)
- `tddy_host_service::{GithubCli::{on_spawn_child_path, at, logged_in_user, auth_token}, GhUser, GithubCliRefusal}`.
- `tddy_changeset::{SessionGithubSource, write_session_github_source, read_session_github_source, SESSION_GITHUB_SOURCE_FILENAME}`.
- `tddy_toolcall::toolcall::{GITHUB_SOURCE_ENV, local_gh_source_value, parse_github_source, SessionTokenSource, request_github_token_from}`.
- `DaemonSessionHost::with_github_cli` (stores the handle).
- `ProjectData.local_gh` and its literal sites (mechanical); `entries::project_entry_from` maps it.
- Proto fields in `project.proto`, `session.proto` (`GithubSource`) and `host.proto`; TS code regenerated.
- Web: the `ProjectsScreen` props `localGh` / `loadHostGithubCli`, test ids, page-object helpers.
- Test infrastructure: `tddy_testing_commons::gh_cli::a_gh_cli`.

## Unit / Integration Tests

Written and verified failing 2026-10-14. There are 50 new tests: 48 fail for missing behaviour, and 2 pass by design (pins, marked). Every pre-existing test in the touched targets still passes.

### tddy-projects — `packages/tddy-projects/tests/project_local_gh_storage.rs`
- `a_row_written_before_local_gh_existed_reads_as_not_using_it` — **passes today (pin: serde default)**
- `a_row_not_using_local_gh_writes_no_local_gh_key` — **passes today (pin)**
- `setting_local_gh_stores_it`
- `setting_local_gh_keeps_the_other_providers_assignments_it_was_sent`
- `local_gh_with_a_github_assignment_is_refused_naming_both`
- `a_refused_local_gh_setting_leaves_the_row_exactly_as_it_was`

### tddy-accounts — `packages/tddy-accounts/tests/local_gh_unit.rs`
- `the_github_host_of_an_https_url_is_its_host`
- `the_github_host_of_an_scp_style_ssh_url_is_its_host`
- `the_github_host_of_an_ssh_scheme_url_is_its_host_without_user_or_port`
- `a_git_url_naming_no_host_is_refused_rather_than_assumed_to_be_github_com`
- `an_unreadable_git_url_is_refused_in_words_that_say_what_to_fix`
- `a_gh_login_commits_under_its_noreply_address`

### tddy-host-service — `packages/tddy-host-service/src/github_cli_unit_tests.rs`
- `the_logged_in_user_is_who_gh_answers_for_that_host`
- `a_gh_logged_out_for_that_host_is_refused_as_not_logged_in`
- `not_being_logged_in_is_explained_naming_the_host_the_user_and_the_command`
- `a_missing_gh_is_refused_as_not_installed`
- `the_token_is_what_gh_answers_for_that_host`
- `gh_answering_something_unrecognised_is_a_failure_not_a_login`

### tddy-host-service — `packages/tddy-host-service/tests/github_cli_environment.rs` (own binary: sets process env)
- `the_gh_asked_as_a_user_is_given_none_of_the_daemons_github_tokens`

### tddy-host-service — `packages/tddy-host-service/src/host_tooling.rs` (tests module)
- `the_gh_probe_for_a_named_host_asks_gh_about_that_host`
- `the_gh_probe_with_no_host_named_asks_gh_about_every_host`

### tddy-changeset — `packages/tddy-changeset/src/session_github_source.rs` (tests module)
- `a_session_whose_start_named_no_source_reads_as_none`
- `a_recorded_source_survives_a_write_and_a_read`
- `the_record_is_kept_beside_the_session_metadata_and_never_inside_it`

### tddy-spawn — `packages/tddy-spawn/tests/session_git_environment.rs`
- `a_planned_child_may_carry_its_github_source_beside_its_commit_identity`
- The token refusal was already pinned by `a_github_token_is_refused_as_session_environment`, so no new test was added for it.

### Env hygiene (each in its own binary, or a module whose other tests ignore the variables)
- `packages/tddy-daemon-kernel/tests/spawn_as_user_github_tokens.rs` — `a_command_run_as_a_user_inherits_none_of_the_daemons_github_tokens`
- `packages/tddy-tool-engine/tests/relayed_shell_github_tokens.rs` — `a_relayed_shell_command_inherits_none_of_the_daemons_github_tokens`
- `packages/tddy-pty/src/runtime.rs` (tests module) — `a_pty_agent_inherits_none_of_the_daemons_github_tokens`

### tddy-toolcall — `packages/tddy-toolcall/src/toolcall/github_token_client.rs` (tests module)
- `a_github_source_value_names_local_gh_and_its_host`
- `a_github_source_naming_no_host_is_refused`
- `a_session_with_no_github_source_asks_its_session_host`
- `a_session_told_local_gh_asks_gh_about_its_host_and_not_the_socket`

### tddy-coder — `packages/tddy-coder/src/tool_host_wiring.rs` (tests module)
- `a_local_gh_session_is_offered_the_github_pr_tools_without_a_host_socket`
- `a_malformed_github_source_offers_no_github_pr_tools`

### tddy-agent-launch — `packages/tddy-agent-launch/src/session_acting_identity_tests.rs`
- `a_local_gh_project_yields_the_gh_logins_four_pairs_and_its_github_source`
- `a_local_gh_project_whose_gh_is_logged_out_carries_its_source_and_no_pairs`
- `a_local_gh_project_whose_git_url_names_no_host_carries_neither`
- `a_local_gh_sessions_host_never_hands_out_a_token`
- `a_local_gh_identity_relayed_by_another_os_user_carries_only_a_logged_out_gh`
- `a_local_gh_identity_relayed_by_the_session_user_keeps_its_identity`
- `a_vault_identity_relayed_by_another_os_user_is_unchanged`
- `a_daemon_side_operation_on_a_local_gh_session_reads_the_token_from_gh`
- `a_daemon_side_operation_on_a_local_gh_project_naming_no_host_is_refused_naming_its_url`

### tddy-agent-launch — `packages/tddy-agent-launch/src/start_request_checks_unit_tests.rs`
- `a_forwarded_start_of_a_local_gh_project_names_local_gh`
- `a_forwarded_start_of_a_vault_project_names_the_vault`
- `a_forwarded_start_that_already_names_a_source_keeps_it`
- `a_forwarded_start_of_a_project_this_host_does_not_hold_names_no_source`
- `a_forwarded_start_is_otherwise_the_request_as_received`

### tddy-telegram-control — `packages/tddy-telegram-control/src/telegram_session_control/workflow_spawn.rs` (`local_gh_tests`)
- `a_telegram_start_of_a_local_gh_project_commits_as_the_hosts_gh_login`
- `a_telegram_start_of_a_local_gh_project_is_not_told_to_start_from_the_dashboard`

### Surface added with them (`todo!()` bodies unless noted)
- `tddy_accounts::local_gh::{github_hostname_of, local_gh_git_identity, LocalGhError}`.
- `tddy_host_service::host_tooling::gh_auth_status_args`.
- `project_storage::set_project_accounts(…, local_gh)`. This is a signature change: the body ignores the flag, and its callers pass `false`; the handler passes `req.local_gh`.
- `SessionTokenSource::resolve`; `ToolHostHandlers::with_github_source`.
- In `tddy-agent-launch`:
  - `ProjectGithub`, `RelayUser`, `SessionIdentity.local_gh_hostname` (`None` at the existing sites);
  - `SessionIdentity::relayed_by`;
  - `SessionAccountAccess::{with_github_cli, project_session_identity}`;
  - `local_gh_github_token`, `forwarded_start_request`.
- `workflow_spawn::telegram_git_environment`.
- New workspace-internal edges:
  - dev-dependency `tddy-testing-commons` for `tddy-host-service` and `tddy-agent-launch`;
  - dependency `tddy-host-service` for `tddy-telegram-control`.

## Technical Debt & Production Readiness

(populated during development)

## Decisions & Trade-offs

**Developer decisions (planning interview, 2026-10-14)**
- **Scope:** only the session's GitHub identity changes; dashboard sign-in stays GitHub OAuth.
- **Shape:** a per-project toggle, not a pseudo-account. Turning it on clears the GitHub assignment.
- **Agent token:** the agent runs `gh` itself; tddy delivers no token.
- **tddy's own GitHub steps** read `gh` where they run, as the session's OS user.
- **Commit identity:** the `gh` login's noreply identity, from `gh api user`.
- **`gh` unavailable:** the session starts with no identity and a reason, reusing the consented
  exception.
- **Sandbox:** same-user only. Otherwise the session gets no identity, and the relay's `gh` is logged
  out by an empty `GH_CONFIG_DIR`.
- **Signal:** an env entry on spawn, `TDDY_GITHUB_SOURCE`, chosen over a socket answer. The spawn
  wire and the supervisor allow-list grow by one non-secret key.
- **Storage:** a new `ProjectData` field, with literal sites edited in a separate mechanical commit.
- **Propagation:** a forwarded `StartSession` carries the source. The fan-out of `SetProjectAccounts`
  is not fixed here; it is
  [2026-10-14-project-account-assignment-reaches-one-host.md](../todo/2026-10-14-project-account-assignment-reaches-one-host.md).
- **Extras:** Telegram, GitHub Enterprise via `git_url`, and per-host `gh` state are in scope.
- **Single PR**, not a stack.

**Planning decisions (stated in the PRD presentation)**
- **No new RPC:** the toggle rides on `SetProjectAccounts`. A new RPC would hit the closed
  `RESIDUAL_METHODS` tripwire and add a handler to an oversized file.
- **Sidecar `github-source.yaml`, not a `.session.yaml` field.**
  - `.session.yaml` is `deny_unknown_fields`, which would break older daemons.
  - For tool sessions it is written by the child, so the daemon writing it would race.
  - It is written only when the start *carried* a source. Otherwise resume re-reads the row, matching
    how account assignment is re-read today.
- **Four token variables are scrubbed**, not two: `gh` honours `GH_ENTERPRISE_TOKEN`/
  `GITHUB_ENTERPRISE_TOKEN` for an Enterprise host.
- **The Local gh source entry is set even when `gh` could not answer at start.** The steps that later
  need a token then fail with `gh`'s own reason at the step, not with a tddy-invented one.
- **`GithubCli` is injected by program path** (`DaemonSessionHost::with_github_cli`). Production uses
  `on_spawn_child_path()`, resolved per call, so a `gh` installed after the daemon starts is found.

## Refactoring Needed

### From /validate-changes
### From /validate-tests

- `tddy-toolcall`'s two test files carry their own small `gh` stubs instead of `tddy_testing_commons::gh_cli`, because `tddy-testing-commons` depends on `tddy-core`, which depends on `tddy-toolcall`. If the stub wording changes, change all three together.
- `ProjectsScreen.loadHostGithubCli` is a required prop. `ProjectsScreenAcceptance.cy.tsx` and `ProjectAccountsAcceptance.cy.tsx` do not pass it yet: tsc is not a gate here, but green should add it to their mounts.
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-14-local-gh-github-source-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved 2026-10-14)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (⚠ open items in the status block)
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests per touched package (`./test -p <pkg>`); full run on CI
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
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-14-local-gh-github-source-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
