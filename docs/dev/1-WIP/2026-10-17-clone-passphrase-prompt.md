# Changeset: Clone passphrase prompt

**Date**: 2026-10-17
**Status**: 🚧 In Progress
**Type**: Feature (with a Bug Fix component: an SSH clone hangs instead of failing)

## Initial Discovery

Full exploration record: [2026-10-17-clone-passphrase-prompt-initial-discovery.md](./2026-10-17-clone-passphrase-prompt-initial-discovery.md)

## Affected Packages

- **tddy-spawn**: [README](../../packages/tddy-spawn/README.md) — the clone gains an environment channel on the forked-worker backend and fail-fast ssh settings
- **tddy-agent-launch**: [module-layout.md](../../packages/tddy-agent-launch/docs/module-layout.md) — `ensure_project_available_for_start` builds the clone's askpass environment and bounds the wait
- **tddy-host-service**: [host-add-key.md](../../packages/tddy-host-service/docs/host-add-key.md) — a second trigger for the passphrase prompt; the helper's loopback endpoint
- **tddy-service**: proto — an askpass request/response pair
- **tddy-web**: [README](../../packages/tddy-web/README.md) — the passphrase dialog on the create-session path
- **tddy-askpass** *(new)*: the `SSH_ASKPASS` helper binary. A skeleton (manifest, empty lib, no-op `main`, placeholder README) exists so its tests have a package; added to workspace `members`
- **tddy-e2e**: `dev_runtime_binaries.rs` drift test
- **Repo scripts**: `install`, `release`, `publish.sh`, `scripts/dev-runtime-binaries.sh`

## Related Feature Documentation

- [PRD-2026-10-17-clone-passphrase-prompt.md](../ft/daemon/1-WIP/PRD-2026-10-17-clone-passphrase-prompt.md)
- [remote-codebase-mode.md](../ft/daemon/remote-codebase-mode.md)
- [hosts-screen-add-key.md](../ft/web/hosts-screen-add-key.md)

## Summary

An SSH clone run by the daemon as an OS user runs `ssh` with an askpass helper. When ssh asks for a key
passphrase the helper asks the daemon, the daemon raises the existing host prompt for the operator who
started the session, decrypts the encrypted answer, and the helper hands it to ssh. ssh loads the key into
that OS user's existing agent. All other SSH failures are made fail-fast.

## Background

A remote managed codebase on `udoo` could not be created from Tddy Desktop: the clone as `mantas` blocked
until the 300 s deadline because `~/.ssh/id_rsa` is passphrase-protected and nothing could answer ssh's
prompt. The passphrase flow the Hosts screen already has was never connected to the clone. See the
discovery for the log analysis, including the independent 30 s peer-forward deadline.

## Prerequisites

Open items this change runs into. The scan covered `packages/{tddy-agent-launch,tddy-host-service,tddy-web,
tddy-daemon-kernel,tddy-service,tddy-remote-git-repo}/docs/code-issues/` and `docs/dev/todo/`. **No ⛔
blocking and no 🚧 claimed items.** `tddy-spawn` and `tddy-projects` have no `docs/code-issues/` directory:
they have not been analysed, which is not the same as clean.

### ⚠ DURING — Loading a key on a supervised host needs a privileged socket path — [`2026-09-06-loading-a-key-needs-a-privileged-socket-path.md`](../dev/todo/2026-09-06-loading-a-key-needs-a-privileged-socket-path.md)

Recorded, not fixed here. This change **side-steps** the problem for the clone because ssh, running as the
OS user, performs `AddKeysToAgent`; the daemon only names the socket in `SSH_AUTH_SOCK`. `AddHostKey`'s own
write path (daemon process opening another user's socket) is unchanged and the entry stays open.

### ⚠ DURING — An unprivileged daemon cannot probe another OS user — [`2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md`](../dev/todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md)

Same constraint governs the in-process clone fallback (`setuid` in `pre_exec`). The supervised backend is
the one that works on a supervised install, so the plan carries the environment through
`allowed_env_keys` (AC13) rather than assuming the fallback.

### ⚠ DURING — `tddy-spawn` modules are over budget — [`2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md`](../dev/todo/2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md)

`spawner.rs` is 2,152 lines. The environment channel and the ssh clone settings go in a **new module**
(`clone_env.rs`), with only the call-site wiring touching `spawner.rs`. The file must not grow.

### ⚠ DURING — `ensure_project_available_for_start` complexity — `packages/tddy-agent-launch/docs/code-issues/complexity-svc-ensure-project-available-for-start-ensure-project-available-for-start.md`

99 lines, over the 60-line budget, unclaimed. Building the askpass environment goes in its own module
(`clone_ssh_env.rs`); the function gains a field on `ProjectClone` and one call, and its measured length
must not increase.

### ⚠ DURING — `service.rs` oversized — `packages/tddy-host-service/docs/code-issues/oversized-file-service.md`

937 production lines and a `#keyring` stack still blocks the split. The askpass handler is a new module
(`askpass_prompt.rs`) and the `HostService` impl gains at most a delegating stub. No new logic in
`service.rs`.

### ⚠ DURING — `CreateSessionPane.tsx` oversized — `packages/tddy-web/docs/code-issues/oversized-file-create-session-pane.md`

785 lines. The prompt subscription and dialog are a new component/hook beside the file
(`StartPassphrasePrompt.tsx`); the pane gains a single mount, not logic.

### ℹ ANSWERED — Does the 30 s abandonment come from the clone? — (discovery, not a TODO file)

No: `PEER_FORWARD_TIMEOUT` (30 s) on the forwarded `StartSession`. Answered by AC11 / Scope item 6.

## Scope

- [ ] Fail-fast clone environment: `ConnectTimeout`, `GIT_TERMINAL_PROMPT=0`, `AddKeysToAgent=yes`
- [ ] Environment channel on the forked-worker clone (`CloneRequest`)
- [ ] `tddy-askpass` helper binary: classify ssh's prompt text, ask the daemon, print the answer
- [ ] Daemon askpass endpoint: loopback, per-clone token (valid for the clone's life; ssh re-asks after a wrong passphrase), raises `SSH_KEY_PASSPHRASE`
- [ ] Prompt issued for the GitHub user who started the session (thread identity into the clone)
- [ ] Clone resolves the OS user's agent socket → `SSH_AUTH_SOCK`; "no agent" is an error
- [ ] Supervised clone carries and documents the new `allowed_env_keys`
- [ ] Forward deadline for a clone-bearing `StartSession` = prompt TTL + clone time
- [ ] Web: passphrase dialog on the create-session path, subscribed to the cloning host
- [ ] Packaging: `install`, `release`, `publish.sh`, `dev-runtime-binaries.sh`, drift test
- [ ] OpenSSH < 8.4 reports "prompting unavailable" instead of hanging
- [ ] Operator docs for the clone's SSH requirements (agent, OpenSSH ≥ 8.4)

## Technical Changes

### State A (Current)

- `clone_as_user_with_env` (`tddy-spawn/src/spawner.rs:620`) runs `sh -c 'test -e "$1" || exec git clone "$2" "$1"'`
  as the OS user with `stdin` null, `stdout`/`stderr` piped, `HOME` set and the caller's `extra_env`.
  `cmd.output()` has no deadline; the 300 s bound is the caller's `tokio::time::timeout`, which abandons
  the blocking task and leaves the child running.
- `svc_ensure_project_available_for_start.rs:62-80` picks a backend: **Supervisor** (env passed through
  `clone_request_with_env`, filtered by the supervisor's `allowed_env_keys`), **ForkedWorker** with
  `CloneRequest { os_user, git_url, destination }` (no env channel; used whenever `ssh_command` is `None`),
  or the in-process `clone_as_user_with_env`. The only env it ever sets is `GIT_SSH_COMMAND` for a
  facilitator clone.
- `AddHostKey` (`service.rs:632`) issues `PromptKind::SshKeyPassphrase`, awaits the encrypted answer,
  decrypts it in the daemon, unlocks the key with `ssh-key` and writes it to the agent over the wire
  protocol. Prompt lifetime 120 s (`host_prompts.rs:28`), single answer, scoped to the GitHub user.
- `WellKnownAgentSockets::socket_for(os_user)` returns the daemon's own `SSH_AUTH_SOCK` for its own user,
  else `/run/user/<uid>/{ssh-agent.socket,gnupg/S.gpg-agent.ssh,keyring/ssh}`, handing the path back even
  when it cannot `stat` it.
- `HostPassphraseDialog` is mounted by `HostAddKeyAction` (`:155,397`) and `HostDesktopOverlay`
  (`:175,243`) only.
- A forwarded `StartSession` is bounded by `PEER_FORWARD_TIMEOUT = 30 s`.
- No askpass helper exists.

### State B (Target)

- The clone's environment is composed by a pure function (`clone_env.rs`) from: the agent socket, the
  helper path, the clone token and the askpass endpoint, with the fixed fail-fast settings. Both
  backends carry it: `CloneRequest` gains `env`, the supervised request already does.
- ssh runs with `SSH_ASKPASS=<tddy-askpass>`, `SSH_ASKPASS_REQUIRE=force`, `GIT_TERMINAL_PROMPT=0`,
  `GIT_SSH_COMMAND="ssh -o AddKeysToAgent=yes -o ConnectTimeout=<n>"` (only when no facilitator
  `GIT_SSH_COMMAND` is set), `SSH_AUTH_SOCK=<agent>` and `TDDY_ASKPASS_URL` / `TDDY_ASKPASS_TOKEN`.
- `tddy-askpass <prompt-text>`: parses ssh's prompt. `Enter passphrase for key '<path>':` → POST
  `{token, kind: passphrase, subject: <path>}` to the endpoint, print the answer on stdout, exit 0. Any
  other text, an unreachable endpoint, a rejected token or a timeout → message on stderr, exit 1.
- The daemon's askpass handler validates the clone token (valid while the clone runs, revoked when it ends), issues the
  prompt for the clone's GitHub user, awaits via `awaited_answer`, decrypts with the host keypair and
  returns the plaintext on the same loopback response. It logs the outcome only.
- `forward_start_session` (`start_request_checks.rs:72`) calls `forward_start_session_via_livekit_within` with
  `start_forward_deadline(config)` — the budget `split_forward_deadline` already gives split starts — instead
  of `forward_start_session_via_livekit`'s 30 s.
- The create-session UI subscribes to `StreamHostPrompts` for the cloning host while a start is in flight
  and renders `HostPassphraseDialog` over the form.

### Delta (What's Changing)

#### tddy-spawn
- **API**: `spawn_worker::CloneRequest.env: BTreeMap<String,String>` (serde default for old workers);
  `clone_env::{CloneSshEnv, build}` — pure.
- **Implementation**: worker applies `req.env`; `clone_as_user_with_env` unchanged in signature.
- **Docs**: `allowed_env_keys` list for `supervisor.yaml` extended in the supervised-clone docs.

#### tddy-agent-launch
- **Implementation**: `ProjectClone` gains `acting_github_user` and the askpass endpoint handle;
  `clone_ssh_env.rs` assembles the environment (agent socket via `tddy_host_service` resolver); cloner
  closure passes it to every backend; a clone-bearing start reports its own forward deadline.

#### tddy-host-service
- **API**: `askpass_prompt::raise_passphrase(prompts, keypair, github_user, subject) -> Result<Zeroizing<Vec<u8>>, AskpassFailure>`
  — issue → await → decrypt, shared with nothing that unlocks or adds a key.
- **Endpoint**: loopback Connect-HTTP `AskpassService.Prompt`, token registry (`ClonePrompts`) with
  `register(github_user) -> CloneGrant` / revoke-on-drop.

#### tddy-service (proto)
- `message AskpassRequest { string token = 1; string prompt = 2; }`,
  `message AskpassResponse { Outcome outcome = 1; bytes answer = 2; string reason = 3; }`,
  `service AskpassService { rpc Prompt(...) }`. Existing messages untouched.

#### tddy-web
- `StartPassphrasePrompt.tsx` (+ hook) subscribes via `useHostPrompts(hostId)` while a start is pending;
  `CreateSessionPane.tsx` gains one mount.

#### tddy-askpass (new)
- Binary + lib: `prompt_text::classify`, `client::ask`, `main`. Depends only on `reqwest`, `serde_json`,
  `clap` already in the workspace (no new external crates expected — confirm before `Cargo.lock` changes).

## Implementation Milestones

- [x] M1: `clone_env.rs` + `CloneRequest.env`; clone fails fast on unreachable host / no key (AC7, AC13)
- [x] M2: `tddy-askpass` classifies prompt text, refuses non-passphrase requests, talks to the daemon (AC6, AC15)
- [x] M3: Daemon endpoint raises the prompt for the right user, token scoped to the clone (AC1, AC5, AC9, AC10)
- [x] M4: Real clone against a passphrase-protected key via sshd + agent (AC1–AC4, AC8) — 9/9
- [x] M5: Forwarded `StartSession` reuses the split-start budget (AC11, unary path)
- [x] M6a: Wrong-passphrase hint — the daemon flags a re-ask for a key it already answered; the helper says so on stderr (AC4)
- [x] M6b: Keep-alive events while a streamed start provisions, so the relay's 30 s idle timeout keeps its meaning (AC11, streamed path)
- [x] M7: Web — `useHostPromptAnswer` extracted from `HostAddKeyAction`; `StartPassphrasePrompt` mounted in `CreateSessionPane` (AC12)
- [x] M8: Regenerated TS bindings in `tddy-web` and `tddy-rust-typescript-tests`; `scripts/generated-code.sh check` is clean
- [x] M9: Packaging — `install`, `release`, `publish.sh`, dev-runtime list, `./test` (AC14)
- [ ] M10: Operator docs for the clone's SSH requirements (agent, OpenSSH ≥ 8.4, `allowed_env_keys`)

## Testing Plan

### Testing Strategy

**Primary Test Approach:** integration, with one real-process acceptance test per risk that fakes would
hide. The central risk is ssh's *actual* askpass and agent behaviour — which prompt text it emits, when it
calls the helper, whether `AddKeysToAgent` fires — and a hand-written fake `ssh` would only encode our
assumptions about it. So the acceptance tests run **real `git`, real `ssh`, a real user-mode `sshd` on an
ephemeral loopback port with a passphrase-protected host-generated key, and a real `ssh-agent`** in a temp
directory. Only the human is replaced: a test double for the prompt registry answers with an RSA-OAEP
ciphertext, which is exactly what the browser sends.

**Why not a faked ssh:** it would pass with a wrong prompt-text pattern, with askpass never invoked, and
with the key never reaching the agent — the three ways this feature actually fails.

**Environment dependency, stated rather than hidden:** the real-process tests need `sshd` and `ssh-keygen`
on `PATH` (the nix dev shell's OpenSSH provides them). They are in the same suites CI already runs.

### Testing Options Analysis

#### Option 1: Real-process acceptance (chosen for the core flow)
**Test Level**: Integration (process boundary)
**Scope**: git → ssh → `tddy-askpass` → loopback endpoint → prompt registry → agent.
**Reliability Considerations**: ephemeral port and temp `HOME`; sshd/agent killed by a drop guard;
`poll-until` on the prompt appearing, never a sleep.
**Implementation Location**: `packages/tddy-agent-launch/tests/clone_askpass_acceptance.rs`

#### Option 2: Daemon-side handler tests with doubles
**Scope**: token handling, prompt scoping, expiry, log hygiene (`AskpassEndpoint::ask`, the in-process entry the HTTP handler calls) — behaviour that needs no ssh.
**Trade-offs**: **Pros** fast, deterministic. **Cons** cannot prove ssh calls the helper — complements
Option 1, does not replace it.
**Implementation Location**: `packages/tddy-host-service/tests/askpass_prompt_rpc.rs`

#### Option 3: Unit tests for pure pieces
Prompt-text classification, environment composition, `CloneRequest` serde compatibility.

### Acceptance Tests

All fluent-tests style (Given/When/Then helpers, one behaviour per test, no conditionals).

**`packages/tddy-agent-launch/tests/clone_askpass_acceptance.rs`** *(real git/ssh/sshd/ssh-agent; lives in `tddy-agent-launch` because `tddy-spawn` cannot depend on `tddy-host-service`)*
- `a_passphrase_protected_key_is_asked_for_once_and_the_clone_completes` — AC1, AC2 (clone tree exists and
  holds the repo's commit; exactly one prompt raised)
- `the_prompt_is_a_passphrase_request_for_the_key_in_question_and_is_the_operators` — AC1, AC9 (kind, subject, `issued_for`)
- `a_key_path_longer_than_ssh_displays_still_raises_the_prompt` — AC1 (ssh truncates the prompt's path at 100 chars)
- `the_unlocked_key_is_left_in_the_users_agent` — AC2 (`ssh-add -l` lists the key's fingerprint)
- `a_second_clone_with_the_key_in_the_agent_raises_no_prompt` — AC3
- `a_wrong_passphrase_fails_the_clone_and_is_not_echoed_anywhere` — AC4 (error says rejected; the typed
  wrong passphrase is absent from the error, stdout and stderr)
- `a_host_key_confirmation_request_is_refused_and_the_clone_fails_in_seconds` — AC6
- `an_unreachable_host_fails_within_the_connect_timeout` — AC7
- `a_key_with_no_passphrase_that_the_server_rejects_fails_with_permission_denied` — AC7
- `the_agent_lookup_names_the_os_user_when_it_finds_no_agent` — AC8 (the agent lookup and its error). The wiring is pinned by four planner tests in `clone_planner.rs`: no agent → `BatchMode`, no `SSH_ASKPASS`, failure names the missing agent; a failed lookup is not called a missing agent; an agent present → the askpass environment

**`packages/tddy-host-service/tests/askpass_prompt_rpc.rs`**
- `a_helper_request_raises_one_passphrase_prompt_for_the_clones_operator` — AC1
- `another_operator_never_sees_the_prompt_and_cannot_answer_it` — AC9
- `an_unanswered_prompt_ends_the_request_as_expired_at_its_deadline` — AC5
- `the_decrypted_answer_appears_in_no_log_line_or_error` — AC10
- `a_token_the_daemon_never_issued_is_refused_without_raising_a_prompt` — AC10
- `a_token_is_refused_once_its_clone_has_ended` — AC10
- `a_non_passphrase_prompt_text_is_refused_without_raising_a_prompt` — AC6

**`packages/tddy-spawn/tests/clone_env_openssh_version.rs`** — AC15
- `openssh_8_3_cannot_be_told_to_ask_without_a_display`
- `openssh_8_4_is_the_first_that_can`
- `a_two_digit_major_version_is_newer_not_smaller`
- `a_banner_that_is_not_openssh_is_not_guessed_at`

**`packages/tddy-agent-launch/tests/start_forward_deadline.rs`**
- `a_forwarded_start_waits_longer_than_a_passphrase_prompt_lives` — AC11
- `a_forwarded_start_gets_the_budget_a_split_start_already_has` — AC11 (one budget, not two)
- ⚠ **Gap, stated openly:** these pin the *deadline value*, not that the forward honours it over LiveKit.
  Proving the latter needs the LiveKit testkit (a docker container); see Open Items in the summary.

**`packages/tddy-web/cypress/component/CreateSessionClonePassphraseAcceptance.cy.tsx`** *(mountWithRpc +
anInMemoryRpcBackend, `aHostPromptFeed`)*
- `shows_the_passphrase_dialog_when_the_cloning_host_raises_a_prompt_during_a_start` — AC12
- `encrypts_the_answer_under_the_hosts_pinned_public_key` — AC12
- `clears_the_dialog_when_the_prompt_is_answered_expires_or_is_cancelled` — AC12
- `subscribes_to_nothing_and_shows_nothing_while_no_start_is_in_flight` — AC12
- `clears_the_dialog_when_the_start_ends_without_the_prompt_being_answered` — AC12
- `refuses_to_take_an_answer_when_the_hosts_public_key_is_not_the_one_pinned_for_it` — AC12 (existing pinning contract)

**`packages/tddy-e2e/tests/askpass_helper_shipping.rs`** — AC14 (the existing `dev_runtime_binaries.rs` only
compares the dev list with `install`, which holds when *neither* names the helper)
- `install_ships_the_helper_beside_the_daemon_in_both_deployments`
- `release_builds_the_helper`
- `publish_packages_the_helper_into_the_deb`
- `a_dev_run_built_with_the_build_flag_has_the_helper`
- `the_test_runner_builds_the_helper_before_the_suites_that_spawn_it`

### Unit / Integration Tests (red phase, Step 7) — written, all failing

Each is an integration test under `tests/` rather than a `#[cfg(test)]` module, so a red file breaks only
itself and not every other test in its crate's lib binary.

- `packages/tddy-askpass/tests/prompt_text.rs` (10) — `classify`: passphrase with path, spaces, an embedded
  quote, a path ssh cut at 100 chars, trailing newline; host-key question, account password, security-key
  PIN, empty/garbage, and a server banner that merely *mentions* a passphrase are all `Other`.
- `packages/tddy-askpass/tests/askpass_cli.rs` (8) — the real binary against the daemon's real
  `AskpassEndpoint`: answer on stdout + exit 0; host-key question, expiry, unknown token, missing
  `TDDY_ASKPASS_URL` / `TDDY_ASKPASS_TOKEN`, unreachable daemon all exit non-zero with nothing on stdout;
  the passphrase never reaches stderr.
- `packages/tddy-spawn/tests/clone_env.rs` (9) — `CloneSshEnv`: askpass + `force`, `GIT_TERMINAL_PROMPT=0`,
  helper URL/token, agent socket (unset rather than empty when absent), `AddKeysToAgent` and
  `ConnectTimeout`, extra ssh args, and `ALLOWED_ENV_KEYS` equals exactly what a clone sets (AC13).
- `packages/tddy-spawn/tests/clone_request_env.rs` (2) — `CloneRequest.env` round-trips; a request from a
  daemon that predates the field parses with no environment.
- `packages/tddy-host-service/tests/clone_grants.rs` (5) — `CloneGrants`: token names its operator, dies with
  its clone, leaves other clones alone, unknown token names no one, 200 tokens distinct and ≥ 32 chars.
- `packages/tddy-agent-launch/tests/clone_ssh_env.rs` (5) — `clone_ssh_env_for` uses the resolver's socket;
  carries the grant's URL/token; a failed lookup is `LookupFailed`, not `NoAgentReachable`; `ClonePlan`
  keeps a facilitator clone to its transport shim only.

**Public surface the tests pin** (the contract `/green` implements): `tddy_askpass::prompt_text::{classify,
AskpassPrompt}`; `tddy_spawn::clone_env::{CloneSshEnv, OpenSshVersion}` with `CloneSshEnv::ALLOWED_ENV_KEYS`;
`CloneRequest.env`; `tddy_host_service::askpass::{AskpassEndpoint, AskpassOutcome, CloneGrant, CloneGrants}`
(`start`, `answer_window`, `register_clone`, `ask`; `CloneGrant::{url, token}`; `CloneGrants::{new,
register, operator_for}`); `tddy_agent_launch::{start_forward_deadline, clone_ssh_env::{agent_socket_for,
clone_ssh_env_for, CloneAgentError, ClonePlan}}`; `test_util::encrypt_for_host`.

**Not covered by a red test, on purpose:** that the daemon refuses to *start* the clone when no agent is
found (the wiring inside `ensure_project_available_for_start`, which would need a whole `LaunchSessions`
to exercise one branch — see Technical Debt). The lookup and its error are pinned; the wiring is a green
task verified through the acceptance suite.

### Testing Principles Applied

Strong assertions on observable outcomes (the repo in the destination, the fingerprint in the agent, the
exact error text), no ranges on deterministic behaviour, one deliberately wide bound only where wall-clock
is the thing under test (connect timeout), and the leak assertions (AC4, AC10) check *absence* across
every captured channel, not one.

## Technical Debt & Production Readiness

- `CloneSshEnv::ssh_args` has no production caller (the acceptance suite aims ssh at a fixture server with it);
  marked `FIXME(clone-ssh-options)`. `CloneGrants::new()` (grants with no endpoint URL) and the endpoint's
  `ask` / `ask_reporting_rejection` are likewise used only by tests — the HTTP layer calls the core directly.
- `askpass_prompt_rpc.rs` carries its own small log recorder, a second copy of the one in
  `host_add_key_handler_tests.rs`. Lift both into `tddy-testing-commons` when a third caller appears.
- Real-process acceptance tests need `sshd`, `ssh-keygen`, `ssh-agent` and a built `tddy-askpass`; `./test`
  must add `-p tddy-askpass` to its prebuild line (pinned by `askpass_helper_shipping.rs`).
- The acceptance tests pin ssh's own wording (`incorrect passphrase`, `timed out`,
  `Host key verification failed`, `Permission denied (publickey)`); an OpenSSH release that rewords
  them fails the test with the observed text.

## Preparatory restructure — done

`HostAddKeyAction.tsx` owned, inline, everything a prompt dialog needs. It is now `useHostPromptAnswer` beside
it (behaviour-preserving: its three existing specs have the same counts before and after), shared with
`StartPassphrasePrompt`. **Follow-up, not done:** `HostDesktopOverlay.tsx` calls `useHostPrompts` directly and
may duplicate the same pinning/answer logic; it was not read or changed.

## Decisions & Trade-offs

| Decision | Alternatives | Why |
|---|---|---|
| `SSH_ASKPASS` helper | fail→prompt→retry via `AddHostKey`; pre-flight probe | ssh itself adds the key as the OS user (no privileged-socket problem) and the clone proceeds in one attempt. Interview 2026-10-17 |
| OS user's existing agent, never start one | daemon-owned agent; start if absent | No new long-lived process; headless hosts without an agent get a clear error. Interview 2026-10-17 |
| Clone only + fail-fast | every git-over-SSH call | Bounded blast radius; rest recorded as a TODO. Interview 2026-10-17 |
| Forwarded `StartSession` reuses `split_forward_deadline` (330 s) | early-return provisioning state; keep 30 s; per-start "will clone" detection | The caller cannot know whether the peer will clone, and split starts already pay this exact trade-off. Interview 2026-10-17 chose the longer deadline; the code survey found the existing budget to reuse |
| Loopback HTTP + per-clone token for the helper | unix socket | An unprivileged daemon cannot `chown` a socket to another OS user; the pattern matches `tddy-remote-git-repo`. **To confirm in review** |
| Refuse non-passphrase askpass requests | answer host-key `yes/no` too | Auto-accepting host keys would silently weaken verification; the operator trusts hosts on the host. **To confirm in review** |
| Real sshd/ssh-agent in acceptance tests | fake `ssh` script | Fakes encode our assumptions about askpass and agent behaviour — the very thing that can be wrong |
| Daemon flags a re-ask for an already-answered key; helper reports it on stderr | loosen the AC4 test to "fails"; pin ssh's wording | ssh 10.2 prints no "incorrect passphrase" by default, and without a hint a mistyped passphrase just re-opens the dialog. Decided with the developer 2026-10-17 |
| Peer emits `StartKeepAlive` every 5 s while provisioning | widen the 30 s idle timeout; defer | The relay's idle timeout keeps its meaning (a dead peer is noticed in 30 s). Decided with the developer 2026-10-17 |
| Older OpenSSH (< 8.4 or unknown) **or no reachable agent**: run the clone with `BatchMode=yes`, no askpass, append an explanation on failure | refuse to clone; use askpass anyway | An unprotected key must keep working on an older host, and nothing may hang. The only degradation, deliberate |
| `AskpassEndpoint` is optional on `LaunchSessions` (`None` outside the daemon runtime) | make it required | Test fixtures that build a host without the daemon runtime keep their old behaviour; making it required changes `DaemonSessionHost::new`, which many tests call |

## Refactoring needed

### From /validate-tests (2026-10-17) — all addressed
- `with_no_agent_reachable_the_clone_says_so_and_starts_none` named behaviour the code no longer has (AC8 was
  reworded) and tested only the lookup — renamed `the_agent_lookup_names_the_os_user_when_it_finds_no_agent`.
- `a_passphrase_protected_key_is_asked_for_once_and_the_clone_completes` asserted five things — split in two.
- `CloneSshEnv::ssh_args` joined arguments with bare spaces into a string git passes to a shell, so a key path
  containing a space split silently; found while checking for missing edge cases, fixed test-first
  (`shell_word`, two new tests). `ssh_args` has no production caller yet — see `/validate-prod-ready`.

### From /analyze-clean-code (2026-10-17) — all addressed
See *Validation Results*. One needs-attention item left: `ClonePlanner::new` takes 5 parameters.

### Left as is, on purpose
- `an_unreachable_host…` and `an_unanswered_prompt…` bound wall-clock time with wide margins (20 s for a 2 s
  timeout; 5 s for 0.3 s): the time *is* the property under test.
- `a_wrong_passphrase…` asserts `1..=3` prompts: ssh's own retry policy (`NumberOfPasswordPrompts`).
- `askpass_prompt_rpc.rs` carries a copy of the log recorder from `host_add_key_handler_tests.rs` (see Debt).
- The negative stream assertions in the two web specs run right after mount; each file also shows the stream
  opening in a sibling test, so they are not vacuous, but they would not catch a stream that opens late.

## Validation Results

### Green, scoped (2026-10-17)

Scoped to the touched packages (`tddy-askpass`, `tddy-spawn`, `tddy-host-service`, `tddy-agent-launch`,
`tddy-daemon-kernel`, `tddy-service`, `tddy-session-lifecycle`, `tddy-e2e`); CI owns the whole workspace.
All new tests pass, including `clone_askpass_acceptance` 9/9 against real git/ssh/sshd/ssh-agent.

Regression found and fixed while verifying: `install` now requires `tddy-askpass`, and three e2e fixtures
that fake a release tree (`install_supervisor.rs`, `sandbox_runner_shipping_acceptance.rs`,
`install_script.rs`) did not list it — 18 failures, now `install_supervisor` 18/18,
`sandbox_runner_shipping_acceptance` 3/3, `install_script` 29/29.

**Failing on `master` too (17), not caused by this change**, verified by running the same targets in a clean
worktree of `origin/master` (`b73c97e05`): `tddy-e2e::grpc_full_workflow` (1),
`tddy-session-lifecycle::{sandbox_behavior_acceptance (5), sandboxed_claude_cli_acceptance (5),
sandboxed_cursor_cli_acceptance (4), sandboxed_session_lifecycle_acceptance (2)}`.

### Web (2026-10-17, after `./dev bun run local-registry-install`)

Cypress component specs, scoped: `CreateSession*` and `Host*` together — **42 specs, 330 tests, all passing**
(includes `CreateSessionClonePassphraseAcceptance` 7/7 and `CreateSessionClonePassphraseMount` 3/3; the
`HostAddKeyAcceptance`, `HostsScreenAddKeyAcceptance`, `HostAddKeySelector` baselines are unchanged at 15/14/6).
Mutation check: pointing the pane's mount back at the agent host fails exactly the split-start test.
No type errors in any touched file (`bunx tsc --noEmit`; the package has 600 pre-existing errors elsewhere).

Two things only running it revealed: the hook read `useAuthContext()` in **two** places
(`useHostPromptAnswer` and `useHostPrompts`), which throws in the create-session pane (handed its token as a
prop, mounted without that context), so the token is now an explicit input
(`useHostPromptAnswer(instanceId, active, sessionToken)`, `useHostPromptsFor`); and on a split start the
clone happens on the **codebase** host, so the pane listens to `codebaseDaemonInstanceId` there.

**Not run:** `tddy-daemon`'s own suite.

⚠ **Draft PR opened while the final scoped pass (fmt ✅, clippy and tests) was still running** — the first
attempt died on a `target/` dep-info error and was re-run; its result is not yet recorded here.

### `/analyze-clean-code` (2026-10-17) — **A**

Measured with a brace-matching script over 82 functions in the new and changed production files (59 of them in
new modules). New modules: length 51 excellent / 8 acceptable; nesting 51 / 8; parameters 56 / 2 / **1 needs
attention** (`ClonePlanner::new`, 5). Largest new function 40 lines; largest new module 184 production lines.
Magic values: none — `REPLY_TIMEOUT`, `DEFAULT_CONNECT_TIMEOUT`, `START_KEEPALIVE_INTERVAL`, `PROMPT_TTL` are named.

Found and fixed (all mine): `relay_frames` took 6 parameters → `Relay { idle_timeout, decode_frame, idle_status }`
with a `run` method (3); `ensure_project_available_for_start` had grown 99 → 108 lines, against the promise
in *Prerequisites* → back to 99 (planner built by `ClonePlanner::for_launch`, refusal mapped inside the
blocking closure); `spawn_worker_main` grew by 6 → `CloneRequest::run`, now 136 (base 137); the wire path was a
literal in two crates → `tddy_service::proto::askpass::PROMPT_PATH`. Improved: `spawn_project_clone` 86 lines /
nesting 7 → 57 / 3.

Pre-existing, not touched by this change: `spawn_worker_main` (136 lines, nesting 5), `build_spawn_request`
(9 parameters), `ensure_project_available_for_start` (99 lines; its code-issue record stands).

### `/validate-prod-ready` (2026-10-17)

✅ Ready. 26 production files checked (new modules, the touched `lib.rs`/wiring files, the web hook and
component). Mock code 0, dev fallbacks 0 beyond the two approved degradations (older OpenSSH / no agent →
no-prompt clone; `AskpassEndpoint` optional outside the daemon runtime), `unwrap`/`panic!` 0, `TODO`/`FIXME`
added 1 (`FIXME(clone-ssh-options)` on a test-driven builder), debug output 0 (`eprintln!` in the
`tddy-askpass` binary is its stderr contract with ssh, not TUI output), unused code 1 (the same builder).

### `/validate-tests` (2026-10-17)

98 tests analysed across 14 Rust files, the shared real-process support and 2 Cypress specs. No `#[ignore]`,
`.skip`/`.only`, tautology or raw `cy.get`; every sleep is on a paused clock. 0 critical, 2 warnings (both
fixed), the third finding was a production edge case (fixed). Details under *Refactoring needed*.

### `/validate-changes` (pr-wrap step 1, 2026-10-17)

Ordinary branch (no PR, base `master`); work is uncommitted, so the working tree was diffed against the
merge-base. **Branch is 5 commits behind `origin/master`** — rebase before opening the PR.

| Check | Result |
|---|---|
| Build / `cargo check --all-targets` | ✅ all touched crates; `cargo fmt --check` was ❌ on two `lib.rs` module orders (`tddy-agent-launch`, `tddy-daemon-kernel`) — fixed |
| Stubs / TODO / FIXME / `println!` in new production code | ✅ none |
| Secrets | ✅ the passphrase is redacted in `Debug`, zeroed on drop, never logged; tokens are 244-bit and per clone |
| Unsafe | ✅ one `write_volatile` zeroing loop with a SAFETY comment |

Changeset sync: every Scope item is implemented except the three below; M10 (operator docs) is wrap-time.

Open findings (see the summary of this run): **W1** an agent-less host failed an SSH clone even for a
key with no passphrase — ✅ fixed 2026-10-17 by decision: the clone now runs without prompting (same path as old OpenSSH), the failure names the missing agent; AC8 reworded; `From<CloneAgentError> for Status` removed as dead; **W2** the keep-alive covers provisioning only;
**I1** the clone token sits in the child's environment (same-OS-user visibility).

## TODO

- [x] Record initial discovery (`2026-10-17-clone-passphrase-prompt-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — Rust: all fail for missing API / missing shipping entries; Cypress spec written but **not run** (no `node_modules` in this worktree; needs `./dev bun run local-registry-install`)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests per touched package (`./test -p {package}`) — CI runs the full suite
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [x] Validate tests (/validate-tests)
- [x] Refactor test issues
- [x] Validate production readiness (/validate-prod-ready)
- [x] Refactor production readiness issues
- [x] Analyze code quality (/analyze-clean-code)
- [x] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {pkg} -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-17-clone-passphrase-prompt-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
