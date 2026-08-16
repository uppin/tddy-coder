# Development TODO

## Known failing tests

### ~~`session_action_wait_times_out_while_running` is load-sensitive~~ — resolved 2026-08-14 (found 2026-08-13, pre-existing — not caused by the deterministic-test-suite changeset)

- `packages/tddy-tools/tests/session_action_jobs_acceptance.rs` — after the bounded wait timed out as
  intended, the test gave the job a fixed **1500 ms** to drain and asserted it reached
  `Completed`/`Failed`. On a loaded machine it was still `TimedOut { still_running: true }` — observed
  in two of three deliberately-loaded workspace runs.
- Same shape as the flakes the deterministic-test-suite changeset closed: a budget standing in for a
  readiness signal, in a package that changeset did not touch. `wait_session_action_job` already
  returns the instant the job reaches a terminal state, so no polling helper was needed — the ceiling
  was raised to a named `A_JOB_HAS_TIME_TO_DRAIN_MS` (30 s) safety net and the test still finishes in
  about 0.3 s.
- ⚠️ **Residual, not fixed:** the test still branches on the outcome (`matches!(Completed | Failed)`,
  and a `match` arm reading "allowed if PRD maps non-zero exits to Failed") — a fluent-tests
  violation. Removing it means deciding which disposition a non-zero exit *must* produce, which is a
  behaviour decision about the PRD, not a test cleanup.

### ~~`cursor_cli_peer_spawn_records_the_orchestrator_link_even_without_repo_path` fails on `master`~~ — resolved 2026-08-13 (source: session-attachment-start-materialization wrap, 2026-07-30)

- `packages/tddy-daemon/tests/cursor_cli_session_acceptance.rs` — the test (introduced by #358)
  starts a cursor-cli session whose `stack_parent` names an orchestrator session it never creates on
  disk, and expects `StartSession` to succeed. It fails with
  `FailedPrecondition: could not resolve stack parent branch: session file missing: parent session
  not found under sessions tree`, i.e. `tddy_core::session_chain::resolve_chain_base_ref` requires the
  parent session file that `unified_chain_base_resolution.rs` separately pins as a hard requirement.
- Either the test must write the parent session's metadata, or spawn-time resolution must tolerate a
  `stack_parent` with no session file (recording `orchestrator_session_id` while falling back to the
  default base) — a behaviour decision, not a test fix.
- Not a regression from any in-flight branch: reproduced with every file on the code path
  byte-identical to `master`. #367 (which last touched the test file) reported no CI checks.
- **Resolved** by the deterministic-test-suite changeset (#385): the fixture was what was wrong.
  `an_orchestrator_session(...)` now writes a real parent changeset, because production correctly
  refuses an unresolvable `stack_parent` with `FailedPrecondition` rather than silently basing the
  child off the default branch — the no-fallback behaviour is the one worth keeping.

### `cargo test --workspace` has 7 pre-existing failures on Linux, not 3 (source: session-attach-ui wrap, 2026-08-01)

A full `cargo test --workspace --no-fail-fast` run on 2026-08-01 was **513 suites pass, 7 fail**, and
every failure predates that branch — established by code location, not assumption. Worth recording
because the workspace suite is therefore not a clean signal for anyone, and a real regression would be
easy to lose among these:

- **The sandbox set** (5) — `sandboxed_bash_action_writes_to_output_dir`,
  `sandboxed_claude_cli_starts_on_linux_with_the_cgroups_backend`, the three
  `sandboxed_cursor_cli_*`, plus `cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`:
  the test scripts do not build `tddy-sandbox-runner`. `start_session_sandbox_unsupported_on_non_darwin`
  is macOS-only.
  **Refinement (2026-08-03):** building `tddy-sandbox-runner` first is not sufficient. With the runner
  built they still fail, with `spawn sandbox runner in cgroups jail failed: Operation not permitted
  (os error 1)`, and the self-skip that should cover it does not fire.
  **The error's parenthetical ("the host may forbid unprivileged user namespaces") is a red herring,
  and it misled an earlier diagnosis here.** This host has
  `kernel.apparmor_restrict_unprivileged_userns=1` *and*
  `apparmor_restrict_unprivileged_unconfined=0` — the second exempts unconfined processes, and a
  `cargo test` binary is unconfined, so `unshare(CLONE_NEWUSER)` is in fact **permitted**. Verified
  directly: the supervisor's own jail (the same `unshare` + uid/gid-map sequence) runs to completion
  on this host, producing `uid=0(root)` inside the namespace.
  So the `EPERM` comes from a **later** step, most plausibly the cgroup write — cgroup v2 delegation
  containment, which `packages/tddy-sandbox/docs/architecture.md` already documents as the reason an
  unprivileged process cannot place its own child in a limited scope. Whoever picks this up should
  make the error name the syscall that actually failed before theorising further; see
  "`unprivileged_userns_available()` under-approximates what the jail needs" below.
- `session_token::tests::verify_rejects_a_token_with_a_tampered_signature` — `packages/tddy-github`.
  Root cause found 2026-08-02: a ~1-in-64 base64 canonicalization flake, not a signature bug. See
  the dedicated entry under Future Enhancements for the exact mechanism and the fix.
- `cursor_cli::tests::cursor_agent_prerequisite_reads_include_install_dir_and_share_root` —
  `packages/tddy-sandbox-recipes`.
- `cursor_cli_peer_spawn_records_the_orchestrator_link_even_without_repo_path` — already tracked above.
- `cancel_task_cancels_a_bash_pty_task` (`task_service_acceptance.rs`) — PTY timing.

**Re-measured 2026-08-13 (pr-stack-base-session wrap): 11 suites / 24 tests**, every one attributed by
its own failure message, none from that branch. New entries beyond the list above:

- **The ACP-stub set** (8) — the six `acp_*` in `tddy-integration-tests`
  (`acp_backend_acceptance`, `acp_host_bridge_acceptance`) and the two `codex_acp_backend_*`: they abort
  with `tddy-acp-stub not built. Run: cargo build -p tddy-acp-stub`. This is the **same class of gap as
  `tddy-sandbox-runner`** — a fixture binary no test script builds — and it is the larger half of the
  workspace's noise. Both belong in whatever `./test` does before it runs.
- `factory_is_shared_per_room_so_two_clients_to_one_peer_never_collide` (`tddy-livekit`) — testcontainers
  loses a UDP port race: `failed to bind host port 0.0.0.0:<port>/udp: address already in use`. Use
  `./run-livekit-testkit-server` and `LIVEKIT_TESTKIT_WS_URL` to avoid it.
- `sandbox_runner_streams_demo_tui_dimensions_on_session_channel` (`tddy-sandbox-darwin`) — macOS-only.
- The sandbox set is **four** `sandboxed_cursor_cli_*`, not three (`..._connect_session_returns_empty_livekit`,
  `..._start_persists_metadata_and_empty_livekit`, `..._start_wires_specialized_agents_env_and_metadata`,
  `..._terminal_io_round_trips`), and on an unprivileged user the cgroups ones fail with
  `Operation not permitted … the host may forbid unprivileged user namespaces` rather than a missing binary.
- `verify_rejects_a_token_with_a_tampered_signature` is **genuinely flaky**, not consistently failing:
  it failed once in five consecutive runs. It expects `InvalidSignature` and intermittently gets
  `Malformed`, so the tampering helper sometimes produces a string that fails base64 decoding before the
  signature is ever checked. Fix the helper to mutate within the alphabet.

### `echoes_a_message_over_sandbox_service_served_over_stdio` is skipped in CI (source: ci-setup, 2026-08-15)

- `packages/tddy-daemon/tests/sandbox_runner_stdio_acceptance.rs` — fails on a GitHub Actions runner
  with `tool ipc server exited before bind`, **with `tddy-sandbox-runner` built and on disk**. It
  survived two nextest retries, so it is a permission failure rather than a flake. The other two
  tests in the same binary pass once the runner binary is staged, so only this one is skipped.
- Same family as the sandbox set above: an unprivileged process cannot place its own child in a
  limited cgroup scope. The distinguishing detail is that the failure surfaces here as a *silent
  runner exit before bind* rather than a named `EPERM`, so the runner is swallowing the real error —
  whoever picks this up should make it report the syscall that actually failed before theorising.
- Skipped via `default-filter` in `.config/nextest.toml` (`[profile.ci]`), which names this file.
- **Long-term fix: run these under the VM testkit rather than on the runner.** A QEMU guest gives a
  fully controlled environment with real root and a writable cgroup root, which is the only way this
  suite and the rest of the sandbox set become genuine CI coverage instead of permanent exclusions.
  See `docs/ft/vm/tddy-vm.md` § VM testkit and the `./vm-tests` script; the open question is cost,
  since the bakes currently take hours and `TDDY_CLOUDINIT_BASE_IMAGE` is never downloaded.

### `handles AbortSignal cancellation` asserts a log prefix production no longer emits (source: ci-setup, 2026-08-15)

- `packages/tddy-livekit-web/cypress/component/transport.cy.tsx:141` looks for a captured log line
  containing **`[LiveKitTransport]`** and `cancelled`, and fails `expected undefined to exist`.
  Grepping `packages/` for the literal `[LiveKitTransport]` finds it in exactly two places — this
  assertion and the capture filter in `cypress/support/component.ts:13`. **No production code emits
  it.** `src/transport.ts` logs through the `debug` package as
  `createDebug("tddy:rpc:livekit-transport")`, so the prefix is the namespace, not that bracketed
  string. The assertion cannot pass in any environment, with or without `DEBUG` set.
- The other five tests in the file pass; they assert on `[TEST] error:`, which the harness does emit.
- **Interim (2026-08-15):** the `transportError` assertion was dropped so the test proves what it is
  actually for — that cancellation reaches the caller, via the `[TEST] error: cancelled` assertions
  that do pass. The test is narrower than it was written to be, and knowingly so.
- **Still open:** whether the transport should emit a stable, capturable marker at all. Either
  production logs a `[LiveKitTransport]` prefix that `cypress/support/component.ts` captures, or the
  test asserts on the `debug` namespace and that filter is widened to match. That is a contract
  decision about what the transport promises, not a test cleanup — which is why it was not settled
  here.
- Found the first time this suite ran in CI. It had never run before — see the entry below.

### `reflection.cy.tsx` had never executed: wrong relative import (source: ci-setup, 2026-08-15)

- `packages/tddy-livekit-web/cypress/component/reflection.cy.tsx` imported
  `./support/ReflectionTestHarness`, but the harness lives at `cypress/support/`, one level up —
  `transport.cy.tsx` beside it correctly uses `../support/TransportTestHarness`. Vite failed the
  import, Cypress reported it as an uncaught error outside any test, and the spec's real assertions
  never ran.
- Fixed here by correcting the path. Worth noting **how long this survived**: nothing ran this suite,
  so a spec that could not even be parsed looked no different from a passing one. That is the
  argument for the suite being in the PR gate rather than run by hand.
- Its first execution then found two more things, both fixed here:
  - **`JSON.stringify` on a protobuf message.** `ReflectionTestHarness` logged the unary invoke
    result with `JSON.stringify(response.message)`, which throws `Do not know how to serialize a
    BigInt` on the 64-bit field — protobuf-es maps `int64` to `BigInt`. Both server-stream paths in
    the same file already used `toJsonString`; the unary path now matches them.
  - **Reflection did not advertise itself.** Callers of `reflection_entry_from` collect the names of
    the entries they already hold, which by construction cannot include the reflection entry the
    call is about to return, so `list_services` omitted `grpc.reflection.v1.ServerReflection`.
    Appending its own name moved into the helper, so all seven call sites (tddy-coder ×5,
    tddy-daemon, tddy-service) get the conventional gRPC behaviour rather than each fixing it. The
    existing "only registered names" tests construct `ServerReflectionImpl` directly and are
    unaffected — the impl still reports exactly what it is given; the helper decides what to give it.

### `accepts_ssh_as_the_policy_user_with_the_generated_per_vm_key` cannot shut its guest down in time (source: ci-vm-tests, 2026-08-15)

- `packages/tddy-vm/tests/vm_boot_control_acceptance.rs:297` fails with `guest accepted the
  powerdown but never released port 2235`. Deterministic, not a flake: it fails the same way run
  alone. It is the **only** remaining failure in the boot-control suite on x86_64; the other five
  pass.
- What is *not* wrong, established by experiment rather than reading: ACPI powerdown works fine on
  this guest. A manually booted guest that finished cloud-init and never saw an SSH connection
  powered off in **~40 s** — `Reached target poweroff.target` → `reboot: Power down`. The
  `shuts_a_running_vm_down_gracefully_via_the_qemu_monitor` test, which never opens an SSH session,
  also passes.
- So the distinguishing factor is the SSH session this test opens. The likely mechanism is systemd
  waiting on the `user@<uid>.service` / session scope during shutdown, which on Debian is bounded by
  `DefaultTimeoutStopSec` (90 s). Roughly 40 s + 90 s exceeds `BootedGuest`'s `SHUTDOWN_TIMEOUT` of
  120 s (`packages/tddy-vm-testkit/src/guest.rs`), which fits the observation — but the actual
  duration was **not measured**, so confirm it before choosing a number.
- Two candidate fixes, and the choice is a real one: raise `SHUTDOWN_TIMEOUT` past the measured
  worst case, or make the session teardown deterministic (close the connection and wait for the
  session to end) so shutdown does not depend on a systemd timeout at all. The second is a readiness
  signal rather than a budget, and is preferable if the session can be observed ending.

### The guest serial console is a 64 KiB pipe, drained only at shutdown (source: ci-vm-tests, 2026-08-15)

- `QemuVmArgs::build_with_serial(config, "stdio")` (`qemu.rs:433`) gives the console-driven boot a
  **pipe**, whose capacity is the kernel's, not ours — 64 KiB by default, 1 MiB ceiling per
  `/proc/sys/fs/pipe-max-size`. A Debian boot with cloud-init writes more than that (measured:
  71,443 bytes), so a guest whose console nobody reads **blocks writing to `ttyS0`**.
- That is what broke shutdown in `accepts_ssh_as_the_policy_user_...`: `wait_for_ssh_ready` polls
  SSH and never pumps the console, so `systemd-shutdown` blocked and the guest never powered off.
  Fixed by draining concurrently with the port-release wait (`SerialConsole::drain_for`).
- **The fix is narrow on purpose.** The console is still undrained between boot and shutdown, so a
  guest can sit blocked on `ttyS0` for the whole body of a test. Nothing in the current suite needs
  the guest to make progress during that window, so all six pass — but a future test that does will
  hang the same way, and the symptom (an accepted powerdown that never completes, or a guest that
  mysteriously stalls) points nowhere near the cause.
- Options, if it ever needs to be properly unbounded:
  - Drain continuously in the background rather than only at shutdown. Keeps the console
    bidirectional, which the login-over-serial tests require.
  - `fcntl(F_SETPIPE_SZ)` to 1 MiB. One line, but it moves the cliff rather than removing it.
  - `-serial file:` is genuinely unbounded and never blocks — `QemuVmArgs::build` already uses it
    for detached boots — but it is **write-only**, so it cannot serve the tests that log in over the
    serial console.
- Related: this fixture keeps only an in-memory tail of the console, which is why diagnosing the
  above needed guests booted by hand to see what they were saying. QEMU's `-chardev …,logfile=PATH`
  would persist a full transcript alongside the interactive backend, making a CI failure readable
  from the uploaded artifact. The bake path already writes `<name>-boot.log`; this one does not.

## Future Enhancements

### Session worktree sync — deliberate gaps (source: session-worktree-sync changeset, 2026-08-15)

- **A `tool` session gets no session room, so it cannot be mirrored.** At spawn the daemon hands
  `tddy-coder` the *project main repo path*; the coder creates the session worktree itself, later,
  from a branch suggestion inside its workflow. So the worktree does not exist when the room would
  open, and `SessionRoomRegistry::open` fails the start on `Measurement::Gone`. Opening against the
  main repo would pin the poll loop to the wrong checkout for the session's life; opening after the
  spawn breaks the first-participant ordering. Closing it means the coder reporting its worktree
  back. Every other agent-running type — claude-cli, sandboxed claude-cli, cursor-cli, sandboxed
  cursor-cli — now opens one.
- **A split session's calls are broadcast but not attributed.** Its room records no deltas (the
  checkout is on another daemon), so a seq would point into an empty ring and send the client to
  fetch a WIP ref that lives elsewhere. Records go out with `activity_seq: 0`; serving an empty
  patch instead would be a lie. Feeding a split session's deltas through its room is a separate
  piece of work.
- **A sandboxed session's in-jail tool calls are not broadcast.** They reach the durable log and the
  live hub but not the room's `session.activity` topic — `sandbox_session.rs` has no
  `SessionRoomRegistry`. Marked `TODO(session-worktree-sync)` at the call site; closing it means
  threading the registry through `dial_and_bridge`.
- **`AgentActivityDeltaRequest.daemon_instance_id` is not honoured.** The field documents peer
  forwarding "as on ExecuteTool", but the handler only looks up the local store; a request naming
  another daemon gets a local `NOT_FOUND` rather than being forwarded.
- ~~**Three room-dependent behaviours are wired but unpinned.**~~ **Closed** by
  `packages/tddy-daemon/tests/session_room_livekit_acceptance.rs`, which opens a real room over a
  real LiveKit server. Kept here because the suite is container-backed and therefore slower than the
  rest: it is the only place the wiring is checked, and the reason it exists is that
  `SessionDeltaStore::attribute` shipped with no production caller while every isolated suite stayed
  green. Original note follows.
  **Three room-dependent behaviours were wired but unpinned.** The `session.activity` broadcast, the
  `delta_store` lookup after a room is registered, and the `close`→`delete_wip_ref` call site all
  need a live LiveKit room to exercise; `SessionRoomRegistry::register` is private and
  `BroadcastPublisher`'s constructors are `pub(crate)`, so no seam exists to inject one. The
  *behaviours* are tested in isolation — what is untested is the wiring. A container-backed suite
  using `tddy-livekit-testkit` (already a dev-dependency) would close this.

- **A split session mirrors committed history only.** The facilitating daemon reaches a remote
  checkout through `GetWorktreeSnapshot`, whose response carries counts and paths but **no tree**
  (`packages/tddy-service/proto/connection.proto`), so it cannot diff a worktree it does not hold —
  see the `FIXME(session-worktree-sync)` at `packages/tddy-daemon/src/connection_service.rs`
  (`remote_worktree_snapshot`). Closing it means a `wip_tree` field on
  `GetWorktreeSnapshotResponse` and the codebase daemon writing it on every measurement. Until then
  a split session must **say** it is committed-only rather than mirror silently stale content.
- **`MintLiveKitToken` cannot grant a session room.** It grants the daemon's `common_room` and only
  that (`packages/tddy-daemon/src/auth.rs`), so a client that must join `session-{id}` has to hold
  `LIVEKIT_API_SECRET` and mint for itself — which is the fleet's session-token signing key, and
  therefore a real widening of the client trust surface versus `tddy-remote-git-repo`. Closing it
  means a mint that takes a session id and grants that room to a caller authorized for that session,
  which needs the room-ownership model recorded under *Remote git repo over LiveKit* below.
- **`StreamReadWorktreeFile` duplicates `StreamReadHostDocument`'s `SESSION_WORKTREE` scope.** Two
  RPCs read the same bytes through two resolvers, differing only in addressing (`project_id` +
  `worktree_path` versus `session_id`). They share the byte reader and every guard, so they cannot
  drift on what they *allow* — but collapsing them onto one reader is the tidier end state.
- **Per-tick attribution is not per-call attribution.** A delta covers every writer in its poll
  window, so `activity_seq` identifies which patch to fetch, never what one call changed on its own.
  Genuine per-call deltas would mean diffing around each tool call, which costs a `git diff` per
  call including read-only ones.
- **Ignored files never sync, and no RPC can reach them either.** A WIP tree is `git add -A`, which
  respects `.gitignore`, so build output and a local `.env` are outside the mirror. That is not a
  gap to close with `StreamReadWorktreeFile`: its listing gate exists precisely to keep
  `.gitignore`'d paths unreadable (`worktree_files.rs`, `resolve_listed_worktree_file`), and
  loosening it would serve every session's `.env` over LiveKit. Mirroring an ignored file needs a
  deliberate, separately-authorized opt-in, not a wider read.

### Models & Agents — open items at wrap (source: models-and-assistants changeset, 2026-08-16)

- **A missing provider credential is not a distinct error.** `credential_for` returns
  `Option<String>` and the client simply omits `Authorization`, so a provider that needs a key but
  has none fails as a generic `Provider(...)` 401 carrying the provider's words rather than ours. Add
  a `MissingCredential` variant (`packages/tddy-daemon/src/model_registry/error.rs:10-36`), refused
  before the round trip, with its own web surface. This is the one PRD requirement that shipped
  partial.
- **No `UpdateProvider` RPC.** A provider's key or base URL can only be changed by deleting and
  recreating it — and because `retired_provider_id` never reuses an id, the recreated row is a
  different provider. Assistants must be rebuilt against it.
- **Service registration is untested.** `packages/tddy-daemon/src/main.rs:622-653` — deleting either
  `rpc_entries.push` leaves all 6750 tests green while the Models screen goes dead. Note also that
  the LiveKit binding is conditional on all four of `livekit.url`/`api_key`/`api_secret`/`common_room`
  being set, and both pushes sit inside the `if let Some(user_resolver)` block, so a daemon without a
  user resolver registers neither.
- **Ownership refusal is under-tested.** `LoadModel`, `UnloadModel` and the ACP chat path all resolve
  the credential through the owner check, but only `DeleteProvider` and `RefreshProviderModels` are
  pinned (`model_registry_service_acceptance.rs:668,705`).
- **No service-level test for `UpdateAssistant` / `DeleteAssistant`** (store- and web-level only), no
  `ListAgents` RPC test with a registry wired (only the `agent_list_mapping` mapper), and no
  service-level `LoadModel`-on-cloud `FAILED_PRECONDITION` assertion (only `UnloadModel`).
- **Per-daemon scoping has no Rust test.** It is structural — a per-daemon DB plus a
  `daemon_instance_id` stamp, correctly with no query filter — but two daemons with two DBs are
  exercised only web-side (`ModelsCrossHostAcceptance.cy.tsx:301`).
- **`ProviderAcpAgent::initialize` / `authenticate` are dead in production** — the daemon hand-rolls
  the same reply at `acp_service.rs:546-561`, so the two can diverge silently.
- **`ModelRegistryStore::replace_models` is `pub` with no production caller** (`store.rs:365-374`);
  `record_refresh` is the real path.
- **`ModelSessionTarget.session_token` is unasserted.** The transport auth gate rewrites only
  *top-level* `sessionToken` fields, so the nested one in a stream frame is never touched by it.
  Production sets it (`ModelChatDialog.tsx:42-47`); nothing pins it.
- **A token refresh mid-conversation restarts the chat stream.** `ModelChatDialog`'s memo has
  `sessionToken` in its dependencies, so a refresh rebuilds the session and loses the transcript.
- **Assistant tools run as the daemon uid.** Confinement is path-based — the ACP `cwd` is
  canonicalised and must resolve inside the caller's own sessions base or their own `projects.yaml`
  repo paths — but unlike every other `execute_tool` caller this runs in-process, not in a session
  process or the sandbox under the caller's uid. Uid separation is the open half.
- **OpenAI models offer no Chat.** `/v1/models` reports no capability information, so they carry no
  `llm` label. Fireworks reports per-model flags and Ollama derives from `/api/show`; closing OpenAI
  needs a different endpoint or an operator-set flag on the model row.

### Models & Agents — adjacent findings (source: models-and-assistants changeset, 2026-08-16)

- **`ClaudeAcpBackend::default()` hardcodes `bunx claude-agent-acp`.**
  `packages/tddy-core/src/backend/acp.rs:360` spawns that command with no env var, no CLI flag and no
  coder-config override — unlike `CodexAcpBackend` (`codex_acp.rs:472`), which resolves through
  `TDDY_CODEX_ACP_CLI` → sibling of the `codex` binary → PATH, plus `--codex-acp-cli-path` and
  `codex_acp_cli_path`. An operator with `claude-agent-acp` installed anywhere other than where `bunx`
  finds it cannot use the backend at all. Give it the same resolution chain.
- **`acp.rs` and `codex_acp.rs` are ~80% duplicated.** Identical `acp::Client` impls, identical
  progress accumulators, identical `run_acp_worker` thread/`LocalSet` loops. `packages/tddy-acp`
  exists as the extraction target and says so in its `lib.rs` doc comment, but currently holds only
  `mapping.rs`. The models changeset adds a *third* ACP implementation there rather than unifying;
  the unification is still owed.
- **`ListAgentModels` has no peer forwarding.** `connection_service.rs:6247` — the
  `daemon_instance_id` field participates only in the cache key, so the session-creation model
  dropdown cannot enumerate a remote host's catalog even though `ListEligibleDaemons` +
  `forward_to_peer` exist and would supply it.
- **Two model catalogs will coexist.** `ListAgentModels` (per coding backend, via
  `tddy-tools list-models`) and the new `ModelRegistryService` (per provider, from SQLite) answer
  overlapping questions by different means. Unifying them — most likely by making the registry a
  provider *behind* `ListAgentModels` — is deferred.
- **Assistant definitions do not replicate between daemons.** Per-daemon SQLite was chosen so
  credentials stay on the host that uses them, but it means an assistant created on the laptop is
  invisible on the workstation. A common-room sync for the `assistant` table (not `provider`) is the
  natural follow-up.
- **Provider API keys are plaintext at rest.** `<tddy-data-dir>/models.db` (plus its `-wal`/`-shm`
  siblings) is created `0600`, but its **parent is the shared `tddy-data-dir`, mode 0755** — not the
  `0700` auth-storage directory `github_token_store.rs` uses. The data dir is deliberately readable
  by session processes running as other uids (`projects/`, per-user session bases), so `0700` there
  would break them; a `0600` file inside it is unreadable by those accounts, but the filename is
  visible. Moving to `<tddy-data-dir>/model-registry/models.db` would hide that too. Encryption is
  not used at all, so a host backup captures every key. The schema reserves a nullable
  `credential_ref` column for an env-var-reference mode that this changeset does not build.

### Remote git repo over LiveKit — deliberate gaps (source: remote-git-repo-over-livekit changeset, 2026-08-15)

- **No CLI login flow.** `tddy-remote-git-repo` takes a refresh token (`--refresh-token` /
  `TDDY_REFRESH_TOKEN`) and exchanges it via `auth.AuthService/RefreshSession`, but the only way to
  obtain one is to read `localStorage.tddy_refresh_token` out of the web UI. A device-code flow
  (`tddy-tools auth login`) would make the feature self-serve. The 5-minute access-token TTL
  (`docs/ft/daemon/session-auth.md`) is why a refresh token is accepted at all.
- **A clone runs at ~2.5 MiB/s, and the wire is the reason.** Measured, not estimated: 150 MiB in
  59 s, byte-identical, by `clones_a_large_repository_with_every_byte_intact`
  (`packages/tddy-daemon/tests/remote_git_livekit_acceptance.rs`, `#[ignore]`d; size it with
  `TDDY_REMOTE_GIT_THROUGHPUT_BYTES`). The earlier worry — that a large clone would outrun SCTP
  buffering because `BidiStreamSender::send` (`packages/tddy-livekit/src/client.rs:357`) has no
  application-level windowing — did **not** materialise: nothing stalls, and raising
  `GIT_FRAME_CHANNEL_CAPACITY` eightfold (8 → 64) changes the rate not at all (59.1 s vs 59.0 s).
  So an ACK/window field on `GitServerFrame` would buy nothing; a materially faster transfer needs a
  different carrier, not a bigger buffer. Worth revisiting only if the LiveKit data channel itself
  gets faster, or if a multi-gigabyte repository makes tens of minutes unacceptable.
- **`token.TokenService/GenerateToken` still lets an *authenticated* caller name any room.**
  The unauthenticated-mint hole is closed: the request carries a `session_token`
  (`packages/tddy-service/proto/token.proto`), the daemon's registration
  (`tddy_daemon::auth::build_token_service_entry`) verifies it with the same resolver that gates
  every other daemon RPC, and the service refuses any `daemon-*` identity on *every* registration
  (`tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX`) so no caller can be admitted as a daemon's
  RPC participant and be handed other participants' calls.
  **What remains:** an authenticated operator may still ask for a JWT for *any room name*, and the
  two `--web-port` registrations plus the LiveKit-surface one in `tddy-coder`
  (`packages/tddy-coder/src/run.rs`) stay unauthenticated — a session coder holds no session-token
  signer (`build_auth_service_entry` builds an *unsigned* `AuthServiceImpl`), so an authenticator
  there would refuse every caller and leave `--web-port` unable to open its own terminal. A caller
  already inside a session's room can therefore mint admission to a *different* room from that
  coder. Deliberately not fixed here: authorizing *which* rooms a given user may join needs a
  room-ownership model that does not exist today (rooms are named ad hoc by the daemon, by session
  spawn and by the presenter path), and inventing one to close this would be a much larger change
  than the impersonation vector warranted. Closing it properly means either giving the coder the
  fleet's verifier, or replacing `token.TokenService` on the web with per-purpose mints that derive
  the room server-side the way `auth.LiveKitTokenService/MintLiveKitToken` does.
- **The concurrent-stream cap is global, not per-user.** `MAX_CONCURRENT_GIT_STREAMS = 16`
  (`packages/tddy-daemon/src/remote_git_service.rs`) bounds the host's exposure but lets one busy
  user starve every other user's clone slot. Per-user would need a keyed map with eviction.
- **A `Serve` stream has no idle deadline.** Deliberate: a legitimate `git-upload-pack` negotiation
  can idle arbitrarily long waiting on the client, so any threshold would be a guess that kills real
  clones. The slot cap above and connection-scoped teardown bound the damage instead. Revisit only
  with evidence of a real stuck-stream class.
- **No peer forwarding.** The client addresses one daemon directly by `daemon_instance_id`. A
  project hosted on a *peer* daemon needs the caller to know which peer; there is no
  `StartSession`-style forwarding hop on this path (see `docs/ft/daemon/livekit-peer-discovery.md`).
- **Git protocol v2 is not negotiated.** Git derives its SSH variant from the command basename;
  `tddy-remote-git-repo` is unknown to it, so git selects variant `simple`, which never passes
  `-o SendEnv=GIT_PROTOCOL`. Sessions run v0/v1 — functionally complete, less efficient on ref
  advertisement. Supporting v2 means implementing the `ssh` variant's option contract.
- **`CliSessionManager::start_terminal` still passes `os_user: None`**
  (`packages/tddy-daemon/src/cli_session_manager.rs:744`), so a started Bash terminal runs as the
  daemon's own identity rather than the session owner's — unlike the main claude terminal, which
  passes `Some(os_user)` (`connection_service.rs:5313`). This changeset surfaces the gap (its own
  git children *do* impersonate, via `wrap_argv_for_privilege_drop`) but does not fix it; on a
  multi-user host the two paths currently disagree about who a spawned process is.

### LiveKit connect/streamer duplication still outstanding (source: remote-git-repo-over-livekit changeset, 2026-08-15)

The remote-git changeset deduplicated **one** LiveKit sequence — connect a room, wait for the
serving participant, vend a client through `LiveKitRpcClientFactory` — into
`tddy_livekit::client_connect::connect_client` (`packages/tddy-livekit/src/client_connect.rs`).
Three call sites now share it: `tddy-tools/src/pty_relay.rs:375`,
`tddy-tools/src/session_tool_client.rs:475`, and `tddy-remote-git-repo/src/relay.rs:49`. Two
adjacent duplications were left in place, both out of scope for that changeset:

- **`tddy-screenshare` and `tddy-livekit-screen-capture` are near-duplicate crates.**
  `packages/tddy-screenshare/src/streamer.rs` (183 lines) and
  `packages/tddy-livekit-screen-capture/src/streamer.rs` (176 lines) each implement the same
  shape — `start()` doing `Room::connect` plus a room-event loop, `push_rgba_frame`, `stop` —
  against the same LiveKit API. They cannot use `connect_client` as-is (they publish a track
  rather than address an RPC peer, so they need no target identity), but the connect-and-watch-
  events half is common, and having two crates own it means a fix to one silently misses the
  other. Decide whether one crate supersedes the other, or extract the shared half; do **not**
  add a third.
- **Ten test files under `packages/*/tests` roll their own connect + `ParticipantConnected`
  wait.** These are the copies most likely to carry the silent-degrade bug `connect_client`
  fixed: a closed event channel is indistinguishable from the participant arriving unless the
  wait reports *why* it stopped (`client_connect.rs:63-91`). A test helper wrapping
  `connect_client` would retire them. Note that two of these suites
  (`coder_serves_connection_service_from_participant`, `common_room_set_metadata_handshake_repro`)
  are already the workspace run's timing-flakiest, failing only under parallel load.

### VM image chaining and testkit — deliberate gaps (source: vm-cgroups-testkit changeset, 2026-08-14)

- **BLOCKER: binaries built in the builder guest cannot execute on the test host.** They are
  compiled inside the Nix dev shell, so their ELF interpreter is an exact store path —
  `/nix/store/nmq81hidzwij3c7vyiazwg2l74vnxkar-glibc-2.42-51/lib/ld-linux-aarch64.so.1`. The test
  host inherits `/nix` from `tddy-nix-base` but not *that* glibc closure, which was only ever
  realized in the builder's dev shell, so `execve` fails `ENOENT` on the interpreter and systemd
  reports `203/EXEC`. Observed as `./install --systemd` succeeding completely and then
  `tddy-supervisor is 'activating', not active`. Four ways out, none free: build against **musl**
  statically (cleanest "deployable" story, but a real change to how the workspace builds and
  `libwebrtc` is the risk); **`nix copy`** the runtime closure to the test host (faithful to how a
  Nix artifact really deploys, but the test host stops resembling a plain production host); build
  with **Debian's own toolchain** in the guest (links against system glibc, but the builder stops
  exercising the real `./release` path); or **warm the dev shell on the test host** (smallest
  change, deliberately reintroduces a toolchain into the guest the tests treat as production-like).
  This blocks all five cgroups e2e tests.
- **`builds_deployable_linux_binaries_on_a_host_that_cannot_compile_them` asserts too little.** It
  checks the ELF header's `e_machine` and stops, so it passed while producing binaries that cannot
  actually run on the guest they are built for — "deployable" is exactly the property it does not
  test. It should assert the binary *executes* in the test host (e.g. `--version`), which is the
  only check the interpreter problem could not have survived.
- **AppArmor profile does not load on Debian 12** (non-fatal): `apparmor_parser` fails with
  `Could not open 'abi/4.0'` — the profile targets a newer abi than bookworm ships. `./install`
  warns and continues, but on a host with `kernel.apparmor_restrict_unprivileged_userns=1` the
  daemon's own sandbox jails would fail to create a user namespace.

- **`tddy-vm-build cloud-init` can only build the *first* layer of a chain** (found by running it,
  2026-08-14). `run_cloud_init_build` unconditionally calls `import_base_image`, which now rejects a
  qcow2 that names a backing file — correctly, since importing a delta into `01-base/` would strand
  it. So passing an already-prepared layer as `--base-image` fails with *"is a qcow2 delta with a
  backing file; import the whole image it ultimately derives from instead"*. Multi-level chaining
  works only through `tddy-vm-testkit`'s `bake.rs`, which hands the parent straight to
  `build_cloud_init_image` without importing. Closing it means letting the CLI distinguish "import
  this pristine image, then chain onto it" from "chain onto this existing layer" — probably a
  `--parent-layer` flag alongside `--base-image`, mutually exclusive.

- **`VmManager::start` still boots with `seed_iso: None`** (`packages/tddy-vm/src/registry.rs:286`).
  `create_vm` now writes a per-VM NoCloud seed authorizing the keypair it generates, and both the
  testkit and `packages/tddy-vm/tests/common/mod.rs` attach it — but a library VM started **through
  the daemon** does not, so SSH into it cannot authenticate (`BatchMode=yes` + `IdentitiesOnly=yes`
  leave no fallback). Closing it means distinguishing library-created VMs, which have a seed, from
  spec-only VMs pointed at an arbitrary `image_path`, which have none and would fail to boot on a
  missing `-cdrom`. That is an RPC-surface decision, not a one-liner.
- **No layer records its parent's identity.** `import_base_image` unconditionally removes and
  re-copies `images/01-base/<name>.qcow2` on every bake, and qcow2 stores no parent hash — so a
  re-imported base silently changes the bytes under every existing child and nothing detects it.
  This is the makers-lt gap the changeset set out to close by recording each layer's parent in the
  manifest; it is not implemented and no test covers it.
- **The five VM production tests have never been run end to end.** Their gating is verified (all
  report `ignored` in a default run) but the bake chain itself is unexercised — the first real run
  should expect corrections around the guest-side `./install` invocation and the `tddy.slice` path
  the delegation assertions read.
- **A non-qcow2 supplied base image is not normalised.** `import_base_image` rejects a *chained*
  qcow2 but copies a raw/VMDK source verbatim, which then fails later at `qemu-img create -F qcow2`
  with a confusing error. If normalisation is added, its argv cannot live in `library.rs`: the
  `no_disk_flattening_acceptance` guard matches per-file on `"convert"` + `"-f"` + `"qcow2"`, and
  `library.rs` already contains `"-f"` from `ssh-keygen`.
- **The anti-flattening guard matches source text**, so it passes for a renamed reintroduction of
  the same behaviour and can false-positive on an unrelated `"convert"` literal. A tripwire, not a
  proof.
- **`dist/linux-aarch64` is hardcoded** (`packages/tddy-vm-testkit/src/layout.rs`) while
  `VmArch::host()` can be x86_64, and `vm_cgroups_acceptance.rs` asserts `EM_AARCH64`
  unconditionally. No arch guard.
- **~150 lines duplicated** between `packages/tddy-vm-testkit/src/guest.rs` and
  `packages/tddy-vm/tests/common/mod.rs` (`force_kill` and `wait_for_port_release` are byte-identical;
  `boot_library_vm` is `BootedGuest::boot` with no shares), already drifting in their env-var
  constants. `tddy-vm` could take `tddy-vm-testkit` as a dev-dependency and keep only
  `TestGuestBuilder`, which has no testkit equivalent.
- **`tddy-vm-testkit` is a plain workspace lib**, so nothing structurally stops production code
  depending on it and picking up `SESSION_TOKEN_SECRET` / `GUEST_PASSWORD`. Consider
  `publish = false` plus a crate-level test-only marker.
### Session rooms are not re-opened when the daemon restarts (source: session-room changeset, 2026-08-14)

`SessionRoomRegistry` is built empty in `ConnectionServiceImpl::new`, and a room is only opened by
`start_workspace_session`. A daemon restart therefore leaves every surviving workspace session's
checkout without its host: the room may still exist on the LiveKit server, but no `daemon-{instance_id}`
is in it, so a split agent resumed against that session finds nothing to address and its
`connect_livekit_client` wait times out after 10 s (`packages/tddy-tools/src/session_tool_client.rs:448`).

The fix is a startup sweep that re-opens a room for each session whose `.session.yaml` has a
`repo_path` and a live worktree — the same shape as the existing startup reconciliation in
`packages/tddy-daemon/src/startup.rs`. Marked in code at
`packages/tddy-daemon/src/session_room.rs:259`.

### A claude-cli split agent has no route to its own attachments (source: session-room changeset, 2026-08-14)

The session-room changeset puts a copy of a session's attachments on the facilitating daemon and serves
them to session-room participants over `ReadHostDocument` / `StreamReadHostDocument`
(`scope = SESSION_ARTIFACT`, `relative_path = "attachments/{basename}"`). A browser or a second agent
that speaks the RPC surface can fetch them.

A **claude-cli** split agent still cannot. It runs with every native filesystem tool disallowed and
`--strict-mcp-config` (`packages/tddy-daemon/src/split_session.rs:180-193`), so its only route out is
`mcp__tddy-tools__*` → `ExecuteTool`, whose tools are rooted at the worktree with traversal rejected
(`connection_service.rs:4462`, `:8056`). Attachments live under the *session* dir, outside that root.

Closing it means a new exec tool that deliberately reads outside the worktree, which widens the boundary
the split placement rests on — it needs its own changeset and its own review, not a quiet addition to
the dispatch table at `packages/tddy-tool-engine/src/lib.rs:217`.

### `tddy-rust-typescript-tests/gen/` is badly stale and nothing detects it (source: remote-managed-worktree changeset, 2026-08-14)

Running `bun run generate` in that package produces **12 files that were never checked in**
(`actions_pb`, `bsp_pb`, `tasks_pb`, `vm_pb`, `vnc_pb`, `sandbox_pb`, the `grpc/reflection` and
`tddy/acp` trees, …) and rewrites three that were, including a 5182-line diff to `connection_pb.ts`.
So the committed set is a curated subset frozen at some past point, and the checked-in files have
drifted behind the proto they are generated from — `auth_pb.ts` is missing `RefreshSession`, added
some time ago.

Nothing catches this: no CI step regenerates and diffs, and the package's own `bun test` needs a
built web bundle, so it does not run in an ordinary check either. A proto change can therefore land
with this package silently describing a different wire contract than the daemon serves — which is
precisely what an interop test package exists to prevent.

Left untouched by the remote-managed-worktree changeset deliberately: regenerating it there would
have added ~5 000 lines of unrelated churn to a feature PR. Worth either regenerating and committing
the whole set in a change of its own, adding a CI drift check, or deleting the directory if the
package is no longer exercised.

### A split agent's join token carries `can_update_own_metadata` it never uses (source: remote-managed-worktree changeset, 2026-08-14)

`tddy_livekit::TokenGenerator` (`packages/tddy-livekit/src/token.rs:50-65`) grants the same set to every
participant it mints for, including `can_update_own_metadata: true`. That grant is what a daemon needs
to publish its advertisement; a split session's agent process never calls `set_metadata` and has no use
for it.

It matters because participant metadata is exactly how peer eligibility is decided
(`eligible_daemon_from_participant_fields`), so the grant is the mechanism by which an agent could
advertise itself as a daemon. That path is now closed by reserving the `split-agent-` identity prefix
in discovery — the robust half — but narrowing the grant would remove the capability rather than filter
its one known use.

Not done here because `TokenGenerator` is shared by every LiveKit participant in the repo and a
narrowed variant belongs in `tddy-livekit`, not in a daemon-side feature. Cheap and worth doing: add a
grants parameter (or a `TokenGenerator::for_agent`) and mint the split token without it.

Related, and larger: the same session-token export means the agent process holds the *user's* full
session token in `TDDY_REMOTE_SESSION_TOKEN`, which authenticates every `ConnectionService` RPC on both
daemons — not just `ExecuteTool` on its own worktree. A session-scoped tool token (audience = this
session, exec-tool methods only) would bound that. Recorded in the PRD's trust model as a known
property rather than an oversight.

### No LiveKit RPC call has a client-side deadline (source: remote-managed-worktree changeset, 2026-08-14)

Neither `tddy_livekit::RpcClient` nor `tddy_rpc`'s `ClientEngine` bounds how long a call may wait for
a response. A request published to a participant that is not listening — a daemon restarted since the
room was joined, say — never completes and never errors. The caller hangs.

This surfaced while caching the LiveKit room in `tddy-tools`: holding one connection moves the 10 s
participant wait from every call to first connect, so a cached client can outlive the peer it
addresses. That case is mitigated by re-checking participant presence per call
(`LiveKitSession::peer_present`), but presence can lapse between the check and the publish, and
nothing bounds a call already in flight.

The same missing deadline is what makes the chunking hazard silent: `packages/tddy-livekit/src/chunking.rs`
documents that reassembly is best-effort and index-keyed, so a lost frame wedges a call permanently —
"deadlines are the only escape", and there are none on the client side. `forward_to_peer` added one
for the *daemon→daemon* hop (`PEER_FORWARD_TIMEOUT`) after exactly this bug; the client side never got
the equivalent.

A deadline on `RpcClient` would cover both. It is a policy decision affecting every LiveKit RPC in the
repo — including long-lived streams, which must not inherit a unary timeout — so it needs its own
change rather than riding along with a feature.

### cursor-cli cannot enforce managed-codebase mode (source: remote-managed-worktree changeset, 2026-08-13)

`cursor-agent` has no `--allowedTools` / `--disallowedTools` equivalent anywhere in this codebase, so a
managed-codebase cursor session can only be *guided* — via `REMOTE_APPENDIX` and a
`.cursor/rules/*.mdc` entry — never *prevented* from attempting native filesystem access. claude-cli
gets hard enforcement through `build_claude_allowlist` + `--disallowedTools`.

This is why split placement (`codebase_daemon_instance_id`) is restricted to `claude-cli` in v1. Adding
cursor-cli would additionally require, all of which the non-sandboxed cursor path lacks today:

- a read-only context dir as cwd (`prepare_context_dir_with_subagent` + `copy_dir_all`) instead of the
  worktree — `cursor_cli_spawn.rs:302` discards `managed_codebase` outright (`let _ = (…)`);
- an MCP registration (`write_cursor_mcp_config`, `packages/tddy-sandbox-recipes/src/cursor_cli.rs:213`)
  written to the cursor `$HOME` or cwd — this path writes none;
- `--force --trust --approve-mcps` in argv, which `write_cursor_mcp_config` deliberately does not inject;
- tool-relay env in `session_env`, **and** on resume — `resume_cursor_cli_session` passes
  `Vec::new()` (`cli_session_manager.rs:346-364`), so any start-time env is silently lost.

Worth revisiting if cursor-agent gains a tool-allowlist or MCP-only mode.

### `session_deletion` leaks the worktree for every session type except claude-cli (source: remote-managed-worktree changeset, 2026-08-13)

`packages/tddy-daemon/src/session_deletion.rs:166-169` gates worktree removal on
`session_type == "claude-cli"`:

```rust
let claude_cli_worktree = metadata
    .as_ref()
    .filter(|m| m.session_type.as_deref() == Some("claude-cli"))
    .and_then(|m| m.repo_path.clone());
```

So deleting a `cursor-cli` or `workspace` session removes the session directory but leaves both the
directory and the `git worktree` registration behind. The remote-managed-worktree changeset widens this
to include `"workspace"` because split sessions would otherwise leak a worktree on the codebase host on
every delete. **`cursor-cli` is deliberately left leaking** — fixing it changes behaviour for sessions
that changeset does not touch, and deserves its own change with its own tests.

Note `docs/ft/daemon/remote-codebase-mode.md` criterion 3 asserted that `DeleteSession` for a workspace
session "removes the session directory and the worktree". It did not. That line is corrected by the same
changeset; the rest of that document's criteria are worth re-verifying against the code rather than
trusted, since at least one was aspirational.

### `tddy-coder --remote` never completes a session bootstrap (source: remote-managed-worktree changeset, 2026-08-13)

`packages/tddy-coder/src/run.rs:4000-4003` contacts the relay daemon successfully and then bails:

```rust
// TODO: implement full session bootstrap (start-session → connect-session → run_goal)
anyhow::bail!("remote mode: successfully contacted relay at {} but full session bootstrap is not yet implemented", daemon_url)
```

`RemoteContextDir` (`packages/tddy-coder/src/remote.rs:27-59`) is referenced only from tests. So the
CLI entry point for remote-codebase mode has never worked end to end, despite
`docs/ft/daemon/remote-codebase-mode.md` criteria 23–28 describing it as shipped. The
remote-managed-worktree changeset delivers the daemon/UI path instead and leaves this alone; either
implement the bootstrap or retire the flag, but the current state advertises a capability that does not
exist.

### `docs/ft/daemon/background-tasks.md` claims TaskService unary methods peer-forward (source: remote-managed-worktree changeset, 2026-08-13)

`docs/ft/daemon/background-tasks.md:153-155` states TaskService's unary methods "forward via
`livekit_peer_discovery::forward_to_peer`". They do not — `packages/tddy-daemon/src/task_service.rs`
contains no `forward_to_peer` call and no `classify_peer_route`, so `daemon_instance_id` on those RPCs
is silently ignored rather than routed or rejected. Either implement the forwarding or correct the doc
and reject a non-local id the way `WatchTask` already does (`task_service.rs:151-155`).

### `TokenGenerator::generate_for` performs no authorization (source: remote-managed-worktree changeset, 2026-08-13)

`packages/tddy-livekit/src/token.rs:50-65` mints a JWT for whatever `(room, identity)` pair it is
handed, with no check that the caller may join that room or claim that identity. Authorization now
lives one layer up, in `token.TokenService` itself: the daemon's registration demands a verified
`session_token`, and no registration will mint a `daemon-*` identity. A *session coder's*
`token.TokenService` HTTP endpoint is still a room-agnostic minting oracle for anything that can
reach it, and even on the daemon an authenticated caller may name any room — see the
remote-git-repo-over-livekit entry above for why, and what closing it would take. Default TTL is 6 h
(`DEFAULT_LIVEKIT_JWT_TTL_SECS`). Separately, `spawner.rs:886-902` passes the raw
`--livekit-api-secret` on the spawned child's command line, where `/proc/<pid>/cmdline` exposes it to
the spawning user.

### Deterministic test suite — deliberate gaps (source: deterministic-test-suite changeset, 2026-08-13)

- **`tddy-sandbox-app` keeps `WarmupOptions::default()`** (`src/main.rs`) while the daemon's budget
  moved into `DaemonConfig.agent_warmup`. It has its own config schema, so a daemon-hosted and a
  standalone session on the same host can warm up with different budgets. Give it the same three
  keys, or have it read the daemon's.
- **`pick_free_loopback_port` / `allocate_verified_grpc_listen_port` share a production TOCTOU
  shape** (`sandbox_session.rs`) — bind, note the port, close, hand the *number* to something else,
  which binds it again. **Observed, not theoretical:** the test-side instance of this failed with
  `AddrInUse` on the third of three loaded workspace runs, on the caller's own re-bind, even after
  the search had been moved below the ephemeral range (`spawner.rs`, run 3 of the
  deterministic-suite measurement). Moving the band only removes the *kernel* as a competitor; the
  window between close and re-bind stays open to anything on the host. The test fixture was then
  fixed by never releasing ownership — it returns the held listener. `pick_free_loopback_port` is
  the worse of the two production cases: it binds `127.0.0.1:0`, i.e. draws from the range the
  kernel actively re-issues, then hands the number to a child.
  Two ways out, in order of preference:
  1. **Pass the bound listener across the fork** rather than the number — `FD_CLOEXEC` cleared,
     `LISTEN_PID`/`LISTEN_FDS`/`SD_LISTEN_FDS_START` set. There is in-tree precedent: the
     `handover` field in `packages/tddy-supervisor/src/spawn_broker.rs` already hands the daemon
     its listening socket this way. This closes the window rather than narrowing it.
  2. **Retry on `AddrInUse`** — currently a *fallback* in the CLAUDE.md sense, and not yet
     permissible: the child at `packages/tddy-coder/src/run.rs` does `TcpListener::bind(addr).await?`
     and then `.expect("gRPC server failed")`, so it panics, and the daemon's startup watch cannot
     tell `AddrInUse` from a bad argument, a missing binary, or a real crash. Retrying on that
     signal would mask genuine breakage. It becomes an option only once the child exits with a
     distinguishable status for "the port was taken".
- **The supervisor's unread stderr pipe can deadlock under `RUST_LOG=debug`** — a child that fills
  the pipe buffer blocks on write while nothing is reading.
- **`spawn_startup_poll_interval_ms > spawn_startup_grace_period_ms` is unvalidated.** The `.max(1)`
  clamp makes it harmless (one poll, then the deadline), but a config that says something impossible
  should be refused at load like the rest of `DaemonConfig`.
- **`packages/tddy-daemon/tests/worktree_files_rpc.rs:188` fails `cargo fmt --check`** — pre-existing
  from `5bd24ad1` (#375), left untouched as unrelated. Anyone running `cargo fmt --all` will
  incidentally fix it.

### tddy-web — a failed `ListSessions` is indistinguishable from an empty result in the new-session form (source: pr-stack-base-session changeset, 2026-08-13)

`CreateSessionPane`'s mount effect fetches sessions best-effort and swallows the failure. That was
tolerable when the list only fed the optional "PR stack parent" picker; it now also decides whether the
"Base the stack on" picker has any options, so an operator who came specifically to seed a stack sees
only "None (agent plans the stack)" and cannot tell "no eligible sessions" from "the fetch failed" —
the likely outcome being an unseeded orchestrator created by accident. Keep the fetch non-fatal, but
record the failure and say so in the picker's help text.

The same effect also never refetches when the in-form daemon selector changes (its dependency list
omits the host), so the offered sessions can belong to a different host than the one that will run the
session.

### Wrapping a changeset leaves dangling `1-WIP` pointers in code comments (source: pr-stack-base-session changeset, 2026-08-13)

`/wrap-context-docs` deletes the WIP PRD and changeset, but code and test comments that cite them by
path are not part of its sweep, so every wrap silently leaves broken pointers behind. The
pr-stack-base-session wrap had six (a proto field comment, its generated TS copy, and four acceptance
suite headers) and they were repointed by hand at
`pr-stacking.md#seeding-the-stack-from-an-existing-session-added-2026-08-13`.

One is still outstanding from an earlier wrap: `packages/tddy-service/proto/connection.proto:449` and
its generated copy in `packages/tddy-web/src/gen/connection_pb.ts` cite
`docs/ft/coder/1-WIP/PRD-2026-07-25-branch-query-and-remote-branch.md`, which exists nowhere — not even
under `1-WIP/archived/`. Left alone here because it belongs to a different changeset and repointing it
churns a generated file.

Fix the process, not just the instances: the wrap step should grep the tree for the paths it is about to
delete and refuse to finish while any code reference survives.

### pr-stack — an externally-located worktree is refused as a stack base (source: pr-stack-base-session changeset, 2026-08-13)

`session_repo_is_in_project` accepts a base session whose canonical `Changeset.repo_path` is at or under
the project's `main_repo_path`. That covers every worktree this system creates —
`worktree::worktrees_dir` is always `<repo_root>/.worktrees` — but git allows a worktree anywhere, and
`worktree.rs`'s resolver explicitly tolerates registered worktrees outside `.worktrees/`. Such a session
is refused as a stack base even though its branch belongs to the right repository.

Conservative in the safe direction (a false refusal, not a false accept) and consistent with treating
"could not tell" as "not the same repository". The real fix is to compare the resolved **git common
dir** rather than a path prefix.

### tddy-daemon — `connection_service.rs` repeats a trim-to-`Option<String>` block six times (source: pr-stack-base-session changeset, 2026-08-13)

`connection_service.rs` has the same eight-line "trim, empty means unset" block at five pre-existing
sites plus the one this changeset added, and the file already contains exactly that helper nested
inside `resume_agent_and_recipe`. Hoist it to module scope and collapse all six. The new site was left
consistent with its five siblings rather than fixed in isolation.

Related, in the same file: `validate_stack_seed_base_session` and `require_pr_stack_orchestrator` are
pure free functions with no `&self`, and belong in a `connection_service/pr_stack.rs` whenever that
13k-line file is finally split.

### tddy-daemon — generalize `pr_stack_spawn_args` to all optional spawn flags (source: pr-stack-base-session changeset, 2026-08-13)

`spawner::pr_stack_spawn_args` exists because an argument vector can be asserted on where a `Command`
cannot. That instinct applies to the four hand-rolled trim/skip-if-empty/`cmd.arg` blocks immediately
above it (agent, recipe, model, project id): `spawn_as_user` now has two mechanisms for one job.
Renaming it to an `optional_flag_args(&[(&str, Option<&str>)])` and routing those four through it
collapses their per-flag `log::debug!` lines into one and makes them testable too.

### pr-stack — a managed claude-cli/cursor-cli session may record a goal id as its state (source: pr-stack-base-session changeset, 2026-08-13)

Surfaced while implementing stack seeding; **not** fixed there, because fixing it changes how existing
on-disk sessions resume and that deserves its own review.

`PrStackRecipe::start_goal()` returns the goal id `"analyze-stack"`, and the claude-cli / cursor-cli
spawn paths in `tddy-daemon`'s `connection_service.rs` seed a managed session's position with
`update_state(&mut cs, WorkflowState::new(recipe.start_goal().as_str()))`
(`spawn_claude_cli_session_inner`, `start_sandboxed_claude_cli_session`,
`start_sandboxed_cursor_cli_session`). So `"analyze-stack"` can be persisted as a *state*, while
`PrStackRecipe::next_goal_for_state` matches only the `"AnalyzeStack"` / `"WriteStackPlan"` spellings —
the goal-id spellings fall into the `_ => orchestrate` catch-all, which would read a session that has
done nothing yet as mid-flight and skip planning.

Tool sessions are unaffected, which is why the orchestrator this changeset creates is fine: a tool
session's `changeset.yaml` is written by its own `tddy-coder` process via `ensure_changeset_recipe`,
leaving `Changeset::default()`'s `Init` — and `Init` is in the table.

Before fixing: confirm a managed **claude-cli** session can actually carry the `pr-stack` recipe in
practice (the web only offers the recipe select for tool sessions, but `managed_codebase` claude-cli
spawns do send `recipe`). If it can, the fix is to accept both spellings per state — and it must be
weighed against sessions already on disk in that state, which resume into `orchestrate` today.

### pr-stack — seeding a stack from several existing sessions (source: pr-stack-base-session changeset, 2026-08-13)

- **Only one base session can seed a stack.** The picker is single-select and
  `seed_stack_with_base_session` refuses a second node. Seeding a chain over *several* pre-existing
  branches would declare dependencies their git history does not have: making the chain real means
  rebasing branches an operator may be actively working in, and leaving it unreal means every node
  below the first reports itself behind its base from the moment the stack exists. Neither was worth
  shipping to get an ordering control.
- **With multi-select would come ordering.** The original request asked for drag-handle ordering over
  the selected sessions, with the linear order becoming the `parents` chain. It was dropped because a
  single base node has nothing to order. The panel's persisted `display_order` and
  `move_planned_pr_node` are the reorder primitives to build it on; note they move *rows*, while this
  would have to move `parents`, which is `pr_set_parents`' job.
- **claude-cli / cursor-cli sessions cannot seed a stack.** The orchestrator is a tool session, and
  those forms hold no agent + tool-path + model triple valid for spawning one.

### tddy-web — activities tail-first / autoscroll follow-ups (source: activities-tail-first-autoscroll changeset, 2026-08-02)

- **The live, interactive chat surfaces still never scroll.** `AgentChatView` gains sticky-bottom
  follow only in `readOnly` mode; `AgentChat`, `WorkflowChatScreen` and the PR-Stack chat render the
  same unmanaged `overflow-y-auto` list and stay pinned to the top. The same helpers
  (`src/lib/scrollFollow.ts`) apply, but the live surfaces add composer focus, the elicitation
  composer and the send round-trip to reason about, so they were deliberately left out of scope.
- **The loaded range is not virtualized.** Paging bounds what is *fetched*; once an operator has
  paged back several thousand entries, every one of them is still mounted. Windowing the rendered
  range is the next cost to pay, and it interacts with the prepend scroll anchor.
- **The read position is not persisted across a session switch.** Switching away and back re-opens
  at the tail even though the registry still holds the loaded range and its cursor. Persisting the
  offset per session in `agentActivityRegistry` would make switch-back resume where the operator
  left off.
- **`StreamAcpReplay` still cannot peer-forward.** `TAIL_THEN_LIVE` inherits the existing
  `TODO(acp-replay)` limitation (the forwarding primitive's idle deadline is sized for a
  short-lived stream). `GetAcpReplayPage` is unary and forwards, so a cross-host session can page
  its history but cannot open the tail feed — the asymmetry is worth closing.
- **Importing `src/index.css` into the component harness is still deferred.** This changeset works
  around it by declaring the transcript's flex/overflow inline (see the changeset's Decisions). That
  duplicated declaration can be deleted once the harness loads the stylesheet — see the entry below.
### VM — daemon-spawned tddy host VM follow-ups (source: daemon-spawned-tddy-host-vm changeset, 2026-08-02)

- **`SerialConsole` should be able to quiesce the guest kernel console.** `ttyAMA0` is shared
  between the login shell and the kernel log, so a `printk` landing mid-command is captured as
  another line of that command's output (observed: a `mount` returning
  `[ 7.790602] 9p: Installing v9fs 9p2000 file system support`). The acceptance tests now run
  `dmesg -n 1` after login to make exact-output assertions deterministic, but any production
  consumer driving a guest over UART faces the same interleaving — a `quiesce_kernel_console`
  on the driver is the natural home.
- ~~**A possible ordering bug in the cloud-init completion script (found statically,
  unverified).**~~ **Confirmed by a real bake and fixed (2026-08-14).** `scripts_per_boot` does
  run before `scripts_user`, so the completion script halted the guest before `runcmd` ever ran
  and the host sealed a half-baked image as a success; `cloud-init status --wait` did not block
  because the seed's `cloud-init clean --logs --seed` `bootcmd` had wiped the status it waits on.
  The completion signal now lives in `runcmd` itself — a preamble step arming an EXIT trap that
  emits `<token>_FAILED`, and a final step that emits the success token — and both paths dump
  the guest's `/var/log/cloud-init.log` and `/var/log/cloud-init-output.log` to the console,
  framed by `TDDY_GUEST_LOG_BEGIN`/`TDDY_GUEST_LOG_END`, into a boot log now written with its
  terminal escapes stripped.
- **Guest console log-level control.** The bake streams every serial line as RPC progress —
  ~713 lines in the first 17 seconds, almost all kernel and systemd chatter. `cloud_init_boot_argv`
  and `QemuVmArgs::build` emit no kernel cmdline at all, so there is no `loglevel=`/`quiet` knob.
  There is no prior art to copy: `~/Code/makers-lt` has no guest-side loglevel control either
  (no `printk`, `dmesg -n`, or `console=`, and its `qemu-vm-builder` exposes no `-append`); it
  handles noise host-side via sentinel matching and a `debug`-namespace gate. Deferred until the
  bake's real signal-to-noise is known.
- **`VmManager::start` still rejects `build_target` specs** (`registry.rs:177-180`) — only
  `image_path` works. Untouched by this changeset.
- **The bake pays a kernel swap it may not need.** Every `genericcloud` base costs an extra
  ~3 min, a ~100 MB download and a reboot to get a 9p-capable kernel. Supplying a Debian
  *generic* base image skips it entirely (the step is guarded on `uname -r`), so documenting
  or defaulting to a generic base would remove the cost. Alternatively, shipping the working
  copy as a second ISO9660 disk needs no 9p at all — iso9660 and virtio-blk are in the cloud
  kernel, as the seed ISO already proves.
- **`QemuVm` reports no real guest exit code from `deploy`.** Now that `serial_shell` can capture
  an exit code over UART, the SSH path's error reporting could be brought up to the same standard.
- **Reuse `serial_shell` for the cloud-init bake's completion detection.** `build_cloud_init_image`
  still uses the single-token `classify_serial_line` matcher; with a console driver available it
  could log into a guest that failed and interrogate it, instead of only reporting that the token
  never arrived.
- **A `ListPreparedBases` RPC.** `CreateVmFromPreparedBase` takes a prepared-base name with no way
  to discover what has been baked — the same gap `ListVmImages` filled for built images.

### `start_session_work_on_selected_branch_shares_the_owning_sessions_worktree` leaks a fixed-name directory (found 2026-08-03, pre-existing)

`packages/tddy-daemon/tests/session_branch_conflict_acceptance.rs:477` builds the owner worktree as
`world.repo.parent().unwrap().join("feat-auth-owner")` — a **sibling** of the temp repo, at a fixed
name. When the temp repo lands directly under `/tmp`, that sibling is `/tmp/feat-auth-owner`, outside
the `TempDir` and so never cleaned up. The next run then fails with

```
git worktree add -b feat/auth /tmp/feat-auth-owner main failed:
fatal: '/tmp/feat-auth-owner' already exists
```

and keeps failing until somebody removes it by hand — which is how it surfaced: a run showed it as a
new failure that was really the residue of an earlier one. Costly because it looks exactly like a
regression in the session-start path.

Fix: put the owner worktree *inside* the test's `TempDir` (a child of `world.repo`'s tempdir root
rather than a sibling of `world.repo`), so it dies with the fixture.

### `verify_rejects_a_token_with_a_tampered_signature` is flaky ~1-in-64 (found 2026-08-02, pre-existing)

`packages/tddy-github/src/session_token.rs` — the test asserts `InvalidSignature` but intermittently
gets `Malformed`. Diagnosed exactly:

```rust
fn with_tampered_signature(token: &str) -> String {   // :232
    chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
```

An HMAC-SHA256 tag is 32 bytes → 43 base64url characters, and 43 mod 4 == 3, so the final character
carries 2 significant bits plus 4 bits that **must be zero** for the encoding to be canonical. `'A'`
is 0 and always decodes; `'B'` is 1 and leaves a non-zero trailing bit, which a strict decoder
rejects. So whenever the untampered tag happens to end in `'A'` — about 1 in 64 runs — the helper
substitutes `'B'` and the token fails to *decode*, never reaching the signature check.

Deterministic per token, probabilistic across runs, because the tag depends on the expiry timestamp
baked into each freshly minted token. Fix: tamper a character that keeps the encoding canonical (flip
one in the middle of the tag), or assert on the tampered *bytes* rather than a re-encoded string.

Verified pre-existing: fails on this branch only inside a full run, passes 11/11 in isolation, and
`session_token`'s 10 tests pass at `master` (2851a1b3) — `tddy-github` is untouched by the
tddy-supervisor changeset.

### `./test` hides every target after the first failure (found 2026-08-02, pre-existing)

`./test` runs bare `cargo test`, which **aborts remaining test targets on the first failing one**
unless `--no-fail-fast` is passed. On any host where an earlier target fails, every later package
silently never runs — and the summary still looks like a complete run, because the passed-count is
simply the truncated total.

This bit for real: the pre-existing `action_sandbox_acceptance` failure (see below) meant
`packages/tddy-supervisor` — alphabetically later — was never executed by `./test`, while the printed
total was indistinguishable from a full pass. `./test --no-fail-fast` works today (the script forwards
`"$@"`), so the fix is to make that the default, or to say so loudly in the usage comment. Given
`./test`'s stated purpose is agent-readable verification evidence, a summary that can silently omit
whole packages is the wrong default.

### `unprivileged_userns_available()` under-approximates what the jail needs (found 2026-08-02, pre-existing — not caused by the tddy-supervisor changeset)

`packages/tddy-daemon/tests/action_sandbox_acceptance.rs` →
`sandboxed_bash_action_writes_to_output_dir` **fails instead of skipping** on a host with
`kernel.apparmor_restrict_unprivileged_userns=1`:

```
sandbox I/O error: spawn sandbox runner in cgroups jail failed:
Operation not permitted (os error 1) (the host may forbid unprivileged user namespaces)
```

The test has the correct self-skip guard, and the guard *passes* — `unprivileged_userns_available()`
returns true. The probe (`probe_unprivileged_userns`) only performs `unshare(CLONE_NEWUSER)` plus the
uid/gid-map writes, whereas `enter_rootless_jail` additionally does
`unshare(CLONE_NEWNS|CLONE_NEWNET)`, `mount(/, MS_REC|MS_PRIVATE)` and the cgroup scope write. So the
probe answers a strictly easier question than the one the caller is asking, and the self-skip
contract silently fails to fire.

Two things to fix, and they are separable:
1. The probe should exercise the same steps the jail does (or the jail's extra steps need their own
   probe), so "available" means available.
2. The error message's parenthetical is a guess appended by the caller; it named userns when userns
   was fine. It should report which syscall actually returned `EPERM`.

Verified pre-existing by inspection: the whole `tddy-daemon` diff on this branch is 20 added lines
(one `pub mod`, one `Option` config field defaulting to `None`) with zero deletions, and
`tddy-sandbox-cgroups`, `tddy-sandbox`, `tddy-actions`, `sandbox_session.rs`, `spawner.rs` and
`spawn_worker.rs` are untouched.

### tddy-supervisor — VM-backed acceptance test (deferred to its own PR, 2026-08-03)

> **Implemented 2026-08-14** by the `tddy-vm-testkit` changeset — see
> [docs/dev/1-WIP/vm-cgroups-testkit.md](1-WIP/vm-cgroups-testkit.md) and
> [plans/vm-cgroups-testkit.md](../../plans/vm-cgroups-testkit.md). Steps 1-6 below are all
> in place, with two deliberate departures: step 1's **download** was dropped (the base
> image is supplied on disk via `TDDY_CLOUDINIT_BASE_IMAGE`, nothing is ever fetched), and
> the single bake of step 2 became a **three-image chain** sharing one Nix-prepared parent,
> so the builder and the guest under test derive from the same base without paying for Nix
> twice. Step 6's gRPC assertions are the remaining gap: the cross-user session and
> `PR_SET_PDEATHSIG` properties still need a tonic client over `ssh -L`. Keep this section
> until that lands.

The supervisor's 33 acceptance tests run the real binary but declare the *invoking* user as the service
user, so `privilege_to_drop` returns `None` and no drop happens; the cgroup base is a temp directory.
Three properties therefore have no automated coverage anywhere, and they are the feature's headline
claims:

- a session for OS user `alice` actually running as `alice` while the daemon runs as `tddy`;
- real cgroup v2 delegation with **enforced** limits (`rmdir` of an emptied scope succeeding, a
  populated one returning `EBUSY` — a plain directory returns `ENOTEMPTY` forever, so the retry path
  and the success path only execute on cgroupfs);
- `PR_SET_PDEATHSIG` surviving a real privilege drop. This one hid a live bug once already:
  `commit_creds()` zeroes `pdeath_signal`, and the property held in tests only *because* no drop was
  planned.

Design settled during reconnaissance, so this is implementation rather than open design:

1. **Base image.** Fetch a public Debian *genericcloud* qcow2, verify its checksum, cache it via
   `VmLibrary::import_base_image` into `images/01-base/`. The download step is genuinely absent from
   `tddy-vm` by explicit design decision (`docs/ft/vm/tddy-vm.md` lists it as out of scope) — it is the
   first thing to write.
2. **Bake once.** cloud-init it into `images/02-prepared-base/` as a single delta chained onto
   `01-base/`, sealed `0444`. (Updated: 2026-08-14 — was a flattened-base + overlay pair promoted
   by `promote_prepared_base_pair`; both are gone.) Bake **OS packages and the account
   only** — do *not* reuse `build_tddy_host_image`, whose recipe mounts the repo over 9p and runs a
   cold `./release` including `libwebrtc` inside the guest (`TDDY_HOST_BAKE_TIMEOUT` is 6 hours, and
   its comment says hours is expected). Baking without 9p also avoids the ~3 min / ~100 MB kernel swap
   that Debian's *cloud* kernel forces, since it ships no 9p modules at all.
3. **Per boot.** A fresh qcow2 overlay off the prepared base via `library::vm_overlay_create_argv`
   (absolute backing), so base images are never mutated. Note `VmLibrary::create_vm` writes to a fixed
   `vm/<name>/<name>.qcow2` and `qemu-img create` fails if it exists, so a per-boot path scheme is
   needed.
4. **Reusable VM.** Mirror the LiveKit pattern exactly — a `run-tddy-vm-testkit` script plus an env var
   carrying the forwarded port, with the testkit skipping teardown when the var was supplied
   externally (`packages/tddy-livekit-testkit/src/livekit_testkit.rs` and
   `run-livekit-testkit-server`). Nothing analogous exists for VMs today: every VM acceptance test
   boots and shuts down its own guest.
5. **Provisioning.** `scp` the host-built `tddy-supervisor`/`tddy-daemon`/`tddy-tools` over the
   always-present `tcp::<port>-:22` forward and run `./install --systemd`. Re-testing a code change is
   then an scp, not a re-bake.
6. **Assertions over gRPC.** The daemon's local socket speaks tonic gRPC and `tddy-service` already
   generates `ConnectionServiceClient`, so `ssh -L <port>:/run/tddy-daemon.sock` plus a tonic `Channel`
   needs no new client code — unlike the Connect surface on the web port, for which the repo has no
   Rust client at all. This also reaches the daemon *through the socket the supervisor creates and hands
   over as fd 3*, giving that handoff end-to-end coverage it cannot get in CI. `tddy-tools --mcp`
   (configured by `TDDY_REMOTE_DAEMON_URL`/`TDDY_REMOTE_SESSION_ID`, not a `--proxy` flag) is the
   guest-side tool path.

Two traps found while surveying, worth carrying:

- **`livekit.api_secret` *is* the session-token HMAC secret.** Setting `livekit: None` is not the clean
  escape it looks like: with no secret the guest daemon returns `Unauthenticated` for every RPC with no
  fallback. A secret must be configured even if LiveKit is never used — and since the harness chooses
  it, the host can mint its own access tokens with `SessionTokenSigner`, or use the `github: { stub:
  true }` provider.
- **`daemon_config_yaml` in `tddy_host.rs` emits no `github:`, `users:` or `supervisor:` block**, so
  guest config emission has to be extended or written directly.

Live where it creates no cycle: `packages/tddy-e2e/tests/` (it already holds `install_supervisor.rs`;
`tddy-vm` depends on neither the daemon nor the supervisor). **Not** `packages/tddy-supervisor/tests/`,
which would need `tddy-daemon` for the client side and that is a cycle. Follow the existing production
test conventions — `#[ignore]` + `#[serial]` + env-gate + early return — so `./test` stays unaffected.

Prerequisite on any machine that runs it: `/dev/kvm` must be *openable*, not merely present. Under TCG
the bake takes hours. `VmAccel::host_default` now tests openability rather than existence, so a host
without access correctly reports `Tcg` instead of producing a manifest QEMU refuses to start.

### tddy-supervisor — deliberate gaps and follow-ups (source: tddy-supervisor changeset, wrapped 2026-08-03)

**Session types that still spawn from the daemon**, and therefore run as the daemon user on a supervised
host:

- **Sandbox sessions.** `SpawnSandbox` exists and builds a real jail, but `sandbox_session.rs` still
  calls `tddy_sandbox_cgroups::spawn_plan` in-process. Routing it needs the daemon's session bridge to
  stop depending on the child's *piped stdio* (`bridge_sandbox_stdio` needs `take_stdio()`), which is
  why the wire contract chose a `--grpc-uds` path over fd-passing.
- **claude-cli, cursor-cli and PTY sessions.** `pty_runtime.rs` drops privilege by shelling out to
  `setpriv --reuid`, which an unprivileged daemon cannot do. This is the one place the "paths, not fds"
  decision does not stretch: routing them needs the pty master fd over `SCM_RIGHTS`.

**`SignalSession` will fail with `EPERM` on a supervised host.** `connection_service.rs` calls
`libc::kill(pid, sig)` directly, which an unprivileged daemon cannot do to a session running as another
user. `SIGTERM`/`SIGKILL` map onto the supervisor's `stop_session`; **`SIGINT` has no equivalent**, so
closing this needs a protocol decision (add a `SignalSession` rpc, or accept TERM/KILL only) rather than
a patch.

**A session handle on the wire.** `SessionRef` names a session by pid, so when the kernel reissues a pid
the displaced session's retained exit status becomes unreachable even though it is the answer a poller
wants. The supervisor already carries a generation counter internally and logs a `WARN` when this
happens; closing it means putting that generation in `SpawnedProcess`/`SessionRef` — a wire change worth
making only when Milestone 6's remaining paths need it. `TODO(supervisor)` in `supervisor.rs`.

**The AppArmor userns grant has to move, not disappear.** The PRD originally claimed it became
unnecessary "because the supervisor is root". That does not follow: the supervisor drops to the target
uid *before* `unshare(CLONE_NEWUSER)`, so at that moment the process is unprivileged and the label in
force is the one attached at exec of `tddy-supervisor`. On a host with
`kernel.apparmor_restrict_unprivileged_userns=1` the grant must therefore exist for the **supervisor**
binary — a `packages/tddy-supervisor/apparmor/tddy-supervisor` profile, with a test pinning which binary
carries it. `install` still renders and loads the existing `tddy-daemon` profile, correctly: it is
path-attached, and the daemon keeps its own non-brokered jail path.

**`Supervisor::shutdown` signals sessions by pid, not by process group.** Every child now leads its own
group, so group-signalling there would also reach a session's own descendants — the same argument that
justifies it in `stop_session`. Those surviving descendants are exactly what makes a cgroup scope
`EBUSY` at the worst moment.

**Unvalidated inputs, both wanting a decision rather than a patch:** a sandbox mount's `target` is not
checked absolute or traversal-free (it names a path inside the jail's own namespace, which is why no
test pins it), and a session spawn's `working_dir` is `chdir`'d after the privilege drop — so it is
traversed with the target user's authority, the important part — but is not matched against any
allowlist the way `tool_path` is.

**Recursive read-only bind mounts.** A read-only `BindMount` remounts only the top mount; submounts
beneath it stay writable. Closing it wants `mount_setattr(AT_RECURSIVE)`. `TODO(supervisor/jail)` in
`spawn_broker.rs`.

**Duplication worth folding, all three deliberate for now:** `apply_socket_ownership` and the
create-dir/unlink/bind/chown sequence exist in both `server.rs` and `supervisor.rs` and want to move to
`socket.rs` (whose header currently advertises itself as pure, so that claim needs amending); the
single-path-component check exists as both `policy::scope_dir` and `cgroup_broker::names_one_directory`
and must keep its two different error types (opaque `Denied` for a caller's scope name, `Invalid` naming
the key for root's own config); and `spawner.rs`'s `clone_as_user`/`run_capture_as_user` still carry
their own `getpwnam_r` copies now that `resolve_target_account` exists.

**Packaging and docs:**

- **`./publish.sh` does not ship the supervisor.** It builds a `.deb` installing binaries plus a systemd
  unit into `/lib/systemd/system` and knows nothing about `tddy-supervisor` or `supervisor.yaml`, so a
  `.deb`-installed host gets the old daemon-only deployment.
- **`docs/ft/daemon/systemd-install.md` documents neither `--user` nor `--headless`** (neither side of
  the rebase that introduced `--user` added them), so "`--user` means no supervisor" currently lives
  only in the `install` header.
- **`INSTALL_NO_SYSTEMCTL=1` does not gate the file writes** — binaries, both configs and both units are
  still written to the four `INSTALL_*_DIR` destinations, which must therefore also be overridden for a
  test install. Gating them was rejected because it would hollow out
  `install_fails_when_config_lists_codex_acp_without_native` and its sibling, which assert only a
  non-zero exit and would then pass for a different reason. The header's promise was narrowed instead.
- **The socket-path drift check warns rather than fails.** Install cannot repair a preserved operator
  config without overwriting it, and a mismatch can be benign since the daemon adopts the passed fd
  regardless of `local.socket_path`.
- **Two codex-acp install tests pass for the wrong reason.** They do not override `INSTALL_BIN_DIR`, so
  the mode-hardening step aborts them at the real `/usr/local/bin` before the codex-acp check they are
  named for. They assert only a non-zero exit, so they still pass. The fix is to give them the four
  `INSTALL_*_DIR` overrides every other install test uses.

**`detect_and_prepare_base`'s process-global `OnceLock`** (in `tddy-sandbox-cgroups`, the no-supervisor
path) means a cgroup topology change requires a restart. The supervisor's own preparation deliberately
has no such cache.

### tddy-supervisor follow-ups (source: tddy-supervisor changeset, 2026-08-02)

- **PTY spawning still drops privilege by shelling out to `setpriv`.**
  `packages/tddy-daemon/src/pty_runtime.rs` (`pty_requires_privilege_drop`,
  `wrap_argv_for_privilege_drop`) prefixes terminal argv with
  `setpriv --reuid --regid --init-groups --`, a second, unrelated privilege-drop mechanism next to
  the supervisor's `setuid`. It should route through `SpawnSession` so there is exactly one path
  and one allowlist. Out of scope here because PTY spawning also carries the pty master fd, which
  has to cross the socket via `SCM_RIGHTS` — its own design problem.
- **`spawn_worker.rs`'s fork-before-tokio machinery becomes dead weight on supervised hosts.**
  `fork_spawn_worker()` exists only because `fork()` from a multi-threaded process can deadlock;
  with the supervisor as *parent*, the daemon never needs to fork at all. Keep it while the
  no-supervisor deployment is supported, then delete it (along with the JSON-over-pipes
  `WorkerRequest`/`WorkerResponse` protocol and the `spawn_worker_request_timeout_secs` setting).
- **`tddy-sandbox-app` on Linux can stop routing through the daemon.**
  `packages/tddy-sandbox/docs/architecture.md` explains it delegates to the daemon purely because
  cgroup v2 delegation containment stops an unprivileged app placing its own child in a limited
  scope. A supervisor that owns the delegated subtree removes that reason — the app could hold a
  supervisor client directly and skip the daemon hop entirely.
- **`supervisor.proto` is outside `buf lint` coverage.**
  `packages/tddy-service/buf.yaml` lints that crate's whole `proto/` directory; moving
  `supervisor.proto` into `packages/tddy-supervisor/proto/` to keep the uid-0 binary's dependency
  tree small took it out of lint scope. `tddy-terminal-rpc` has the same gap, so this is a
  workspace-wide pattern rather than a new regression — but the *privileged surface's* proto is the
  one most worth linting. A 4-line `buf.yaml` per proto-owning crate closes it.
- **`cpu_max_ceiling` cannot express the kernel's `"max <period>"` form.**
  `policy::CpuMax::from_str` accepts exactly two integers, because that is all the tests pin. But
  the kernel writes `cpu.max` as `"max 100000"` when a cgroup is uncapped, so an operator who
  copies that value into `cgroup.cpu_max_ceiling` gets a `SupervisorError::Invalid` on *every*
  scope creation — at runtime, not at config load. Fixing it needs an explicit `CpuMax::Max`
  variant with its own tests, plus load-time validation of the ceiling, not a quiet parse
  fallback. Discovered implementing Milestone 1.
- **`detect_and_prepare_base`'s process-global `OnceLock` means a cgroup topology change needs a
  supervisor restart.** Acceptable today (the base is stable for a boot), but worth revisiting if
  the supervisor ever has to survive a re-delegation.

### tddy-web — inactive session activities follow-ups (source: inactive-session-activities changeset, 2026-08-01)

- **The component harness renders without any CSS, so no Cypress component test can assert layout.**
  `cypress/support/component-index.html` loads no stylesheet and `cypress/support/component.ts`
  imports none, so every Tailwind class is inert and every element measures the full viewport width.
  This was discovered attempting to pin "the inspector is a ~360px overlay, not the full pane" after
  `data-docked` was removed — the assertion failed with `expected 1280 to be below 1280`. **Do not
  re-attempt geometry assertions in component specs** without first importing the app stylesheet into
  the harness; until then, the *removal* of inspector docking has no direct test pinning it (the
  specs prove the adjacent fact that the base view stays mounted behind an open drawer). Importing
  the stylesheet would make layout testable but risks perturbing the ~163 existing specs, so it is
  its own changeset.
- **Resume has up to a ~2s dead time before the view changes.** The base view is derived from
  session liveness, and liveness for the selected session only refreshes on the drawer's 2s
  `ListSessions` poll (`sessionManager.ts` `REFRESH_INTERVAL_MS`), so after clicking Resume the pane
  keeps showing the recorded transcript until the next poll reports the session live. Not a
  regression — it falls straight out of "the view is derived, not navigated" — but it is a visible
  lag on this feature's primary action. An optimistic local liveness hint on a successful
  `ResumeSession` would close it without reintroducing view state.

### tddy-web — dead session surfaces (source: inactive-session-activities changeset, 2026-08-01)

- **`SessionDetailPane.tsx` has no importers.** It carries its own Resume/Delete buttons
  (`sessions-detail-resume-*`, `sessions-detail-delete-*`) and a full metadata block, all reachable
  from nothing — `SessionMainPane` superseded it. The ids still live in `cypress/support/testIds.ts`
  (`sessionsDetailResumeBtn`, `sessionsDetailDeleteBtn`), so a spec could be written against a
  component the app never mounts. Delete the component and its ids together, or wire it back in;
  leaving it is what let a second Resume affordance drift out of sync with the real one.
- **`useSessionActivity.ts` is callerless.** It consumes `StreamSessionActivity`
  (`AgentActivityRecord` frames, coalesced by `call_id`) and nothing calls it — both the Agent
  Activity overlay and the new inactive-session Activities view read the ACP replay path
  (`useAcpReplay` over `StreamAcpReplay`) instead. The daemon still serves the RPC. Decide whether
  `StreamSessionActivity` has a remaining consumer before treating it as load-bearing; note it also
  hardcodes `daemonInstanceId: ""`, so it would not peer-forward for a cross-host session as written.

### Session attach UI — follow-ups (source: session-attach-ui changeset, 2026-08-01)

- **The browser→daemon leg has never run.** Nothing in the attach feature has been exercised against a
  real daemon: the daemon suites drive `ConnectionServiceImpl` directly, and every web Cypress spec
  stubs the RPCs with the in-memory backend. Real chunk uploads over the LiveKit data channel, a real
  streamed `StartSession`, and a real cross-host fetch are all unverified end to end. **The manual
  check:** bring up two daemons via `./web-dev`, then (1) in **New session** with the Host selector on
  the connected daemon, attach one local file and one host document — confirm per-row progress advances
  from streamed events and both land in `{session_dir}/artifacts/attachments/` before the agent's first
  turn; (2) repeat with the Host selector on the **second** daemon, which exercises the cross-host
  staged fetch and the streamed forward — the two paths most likely to hang silently rather than fail;
  (3) restart the staging host and confirm the staging root is gone.
- **Two files sharing a `File.name` in one batch fail opaquely.** They stage under the same
  `(staging_id, file_name)`; uploads are sequential, so the first writes its `.staged-complete` marker
  and the daemon refuses the second with *"staged file already exists in this batch"* — an error, not
  truncation or corruption. But it surfaces as an opaque daemon failure late in the submit instead of a
  form-level refusal beside the offending row. The duplicate-basename check catches the default case
  (a row's basename starts as its file name) but not a batch where the operator renamed one of two
  same-named files. The fix is a **unique staged file name per row**, not a second refusal.
  `TODO` at `packages/tddy-web/src/hooks/useStagedAttachmentUpload.ts`.
- **No consumed-batch staging GC.** The restart-cleared staging root
  (`std::env::temp_dir()/tddy-staging`) bounds *abandoned* batches, but a batch that a `StartSession`
  successfully consumed is still left on disk until the next host restart.
- **`RpcRequest.abort` is never read by `ServerEngine`.** Dropping a stream receiver does not stop the
  producer, so a peer keeps producing frames nobody drains and a client that disconnects mid-creation
  still gets its session created (an orphan the UI never showed). Unary `StartSession` has the same
  property — this is parity, not a regression — but streaming plus a multi-megabyte upload widens the
  window from seconds to minutes. A real fix needs an abort frame honoured server-side plus
  peer-disconnect teardown.
- **`StreamSessionActivity` / `StreamAcpReplay` / `WatchTask` / `WatchTaskList` still refuse
  `PeerRoute::Forward`.** The streaming-forward primitive they were blocked on now exists
  (`livekit_peer_discovery::forward_server_stream_to_peer`), but its idle deadline is sized for a
  short-lived stream and these four are open-ended, so migrating them needs a keepalive frame first.
  Their `TODO`s in `connection_service.rs` state this.
- **Do not "fix" the relay channel by bounding it.** `forward_server_stream_to_peer` uses an
  *unbounded* channel on purpose. The transport's bounded `mpsc::channel(32)` beneath it is filled by
  the room's **shared** response loop via an awaited send, so a relay that stopped draining would block
  that single loop and head-of-line-block *every other in-flight forwarded RPC on the daemon's
  common-room connection*. Buffering one stream is strictly better than stalling all of them. What
  bounds the buffer today is that both callers cap what they accept and both streams are short-lived; a
  real fix is per-stream flow control in the transport.
- **`cypress/support/livekit/fakeCommonRoom.ts` does not serialize `max_attachment_bytes`.** A Cypress
  `DaemonHost` fixture driven through the fake room therefore advertises no cap. No current spec needs
  it — the attachment specs inject hosts directly into `SelectedDaemonProvider` — but a future spec
  routing through the fake room would find the cap mysteriously absent.
- **`attachment_size_bytes` reports 0 when `metadata` fails** on a file it just wrote successfully
  (`connection_service.rs`). Display-only: it feeds the progress event's `bytes_total`, not a
  correctness gate.
- **`on_disk_size_bytes` reports 0 for a tracked-but-deleted file** (`worktree_files.rs`), logging a
  warning rather than skipping the entry — skipping would silently drop the file from the Code pane
  tree. It opens no cap-enforcement hole, since `stat` only fails when there are no bytes to attach, so
  a large file can never be understated. If the listing should instead drop unstattable paths, that is
  a small follow-up.
- **The three attachment Cypress specs each rebuild a near-identical baseline backend.** Real
  duplication, but 12 pre-existing `CreateSession*.cy.tsx` files duplicate it the same way, so the new
  specs followed the house pattern rather than inventing a second one. A shared
  `aCreateSessionBackend()` fixture is a suite-wide follow-up, not this feature's debt.
- **`CreateSessionPane.cy.tsx` fails spuriously on its first batched Cypress load.** It is the only
  `CreateSession*` spec still using `cy.intercept` against a real ConnectRPC transport (the rest use
  the in-memory backend); observed failing as a single 275 ms failure inside a 6-spec batch, then
  passing 29/29 alone and 63/63 on an identical re-run. Migrating it to `anInMemoryRpcBackend` would
  remove the flake.
### Web URL state routing — follow-ups (source: web-url-state-routing changeset, 2026-08-01)

- **Two worktrees cannot run `cypress:component` at once** — `vite.config.ts` sets no `server.port`,
  so every checkout's Cypress component dev server takes Vite's default 5173. A second concurrent
  run makes one checkout fetch the other's `cypress/support/component.ts`, and every spec after that
  dies with `Failed to fetch dynamically imported module` — which reads like a test failure but is a
  port collision. A `server.port: Number(process.env.VITE_PORT) || undefined` in `vite.config.ts`
  (mirroring the `dev` script's existing `VITE_PORT` convention) would let each worktree pick its
  own; left out of this changeset as unrelated shared-config scope.
- **`src/rpc/**` is in no `bun test` path** — `package.json`'s `test:unit` covers
  `src/components/connection src/components/sessions src/lib src/hooks src/utils`, and
  `bun test src/routing` covers routing. `src/rpc/selectedDaemon.test.ts` therefore never runs, and
  when this changeset touched it, it turned out it *cannot* run: it imports a `.tsx` module, so bun
  fails on `react/jsx-dev-runtime`. This changeset relocates those five cases to
  `src/routing/selectedHost.test.ts`, but the general gap stands — adding `src/rpc` to `test:unit`
  needs a bun-side JSX runtime (or every `src/rpc` unit test kept to pure `.ts` modules) first.
- **Migrate the hash router (`#/sessions/:id`) to real `history.pushState` paths** — deliberately
  out of scope here: real paths need server rewrite rules on the daemon's static bundle handler, and
  the hash grammar deep-links fine without them. Worth revisiting if the app ever wants distinct
  server-rendered entry points.
- **`WorktreesAppPage`'s host `<select>` (`daemonId`) is not in the URL** — it is a create-worktree
  form field rather than a destination, so it was left as local state. If the worktrees screen ever
  starts *listing* per host (today the list is local-daemon-only, per the note in that file), it
  becomes a navigable selection and should join the URL grammar.
- **The RPC Playground's `participant` param has no acceptance test** — `RpcPlaygroundAppPage`'s
  harness is `cy.intercept`-driven around LiveKit reflection; standing it up was disproportionate to
  the one param. Service/method are covered at the `RpcPlaygroundScreen` level.

### PR-Stack — full control follow-ups (source: pr-stack-full-control changeset, 2026-07-30)

- **Split `pr_stack/mod.rs` (2434 lines) into `mod.rs` + `mutations.rs`** — the recipe definition (`PR_STACK_TOOL_NAMES`, the orchestrate prompt, `PrStackRecipe`, the two trait impls) and the 816-line stack-mutation API (11 functions + 4 input/output structs) are two modules in everything but name, and `pr_stack/` already has submodules (`bridge`, `hooks`). The production-code split costs **nothing**: all public functions keep their paths through a `pub use`, and the three private helpers are used only by their own group so they need no visibility change. The only real work is partitioning the single flat 1252-line `mod tests` — already banner-sectioned by subject, but ~1250 lines of churn, which is why it hasn't happened. It gets more expensive every changeset: the test module grew 547 lines in `pr-stack-full-control` alone. Bonus: 7 of the 9 mutation functions open with a function-local `use tddy_core::changeset::{read_changeset, update_stack_atomic};` that a module-level `use` would delete.
- **Split `orchestrate_pr_stack/github.rs` (1298 lines) into `github/{mod,lifecycle,insight}.rs`** — this is the file `pr-stack-full-control` is responsible for (549 → 1298, +546 production lines). The boundary is unusually clean: the two trait impls share **no** code beyond `resolve_token`/`require_token`, which would become `pub(super)`. `owner_repo_from_remote_url` is genuinely misfiled and belongs with the git helpers. The obstacle is that `mod tests` straddles the boundary — it holds `MockGithubPrApi` + `GithubPrApi` object-safety tests *and* the 14 `search_qualifiers` cases under one `use super::*`, so it has to be split in two and repointed, plus `mod real_impl_tests` moves to `lifecycle.rs`. No macro constraint blocks it.
- **`server.rs` (2527 lines): lift `server/subagent.rs` out first** — the subagent group (~350 lines: `subagent_enabled` … `subagent_tool_router`, the session table, the accounting file) is self-contained, needs no visibility changes, and has no coupling to the tool router. The larger win is a `server/pr_stack_tools.rs` holding the 15 PR-stack tool methods behind a second `#[tool_router(router = pr_stack_tool_router)]` — `ToolRouter` implements `Add` and `PermissionServer::new` already merges four routers, so the macro does **not** force one file. But it needs all 15 methods to become `pub(crate)` for `call_tool_by_name`, and both `advertised_tool_defs()` and `PermissionServer::new` to merge the second router — two edits where a miss is silent (the tool vanishes from the web Inspector but still dispatches by name, or the reverse).
- **A `PrSearchState` enum** — the search-state vocabulary (`open`/`closed`/`merged`/`all`) is written out by hand in five places: the `search_qualifiers` match arms and their `is:` counterparts, the `PrSearchQuery` doc, the `pr_search` schema description, and the tool description. `PrState` cannot serve (it has no `All`). An enum with `FromStr` + `as_qualifier()` would collapse the match, the validation error and the schema text into one definition. Related: `pr_search`'s default state is the bare literal `"open"` in the tool layer while the same tool's limit defaults route through the named `DEFAULT_SEARCH_LIMIT`/`MAX_SEARCH_LIMIT`.
- **The candidate-stack clone-push-wrap is still duplicated between the two appenders** — `add_planned_pr_node` and `adopt_pr_as_stack_node` each build `Stack { version, nodes: candidate_nodes }` inline before calling the shared `reject_if_cyclic`. A `stack_with(&existing, node)` helper would fold it; deliberately left out of the clean-code pass to keep that refactor scoped to the `topo_order` tail.

- **A testable transport seam for `RealGithubPrApi`** — `github_api_url` (`github_rest_common.rs:36`)
  hardcodes `https://api.github.com` and the transport is `Command::new("curl")`, so no HTTP mock can
  intercept it. Every GitHub request/response body in the repo therefore ships with zero automated
  coverage, and `pr-stack-full-control` adds eight more (`GET /pulls/{n}`, `/files`, `/reviews`,
  `/comments`, `/issues/{n}/comments`, `/commits/{sha}/check-runs`, `/search/issues`, and a title/body
  `PATCH`). `wiremock` is already a dev-dependency of both `tddy-tools` and `tddy-workflow-recipes`.
  Adding a base-URL override plus a real HTTP client would make the request shapes and the JSON parsing
  testable in one move — deliberately kept out of the feature changeset because it is a transport
  migration touching every existing call path.
- **Review-thread resolution state needs GraphQL** — `pr_comments` returns threads without a `resolved`
  flag because the REST API does not expose one (it is `reviewThreads.isResolved` on the GraphQL v4
  schema only). No field is emitted rather than guessing. Adding it means the first GraphQL call in the
  repository.
- **`pr_search` returns no branch names and does not paginate** — `GET /search/issues` omits a PR's head
  and base, so `base:` works as a query qualifier but the agent must follow up with `pr_read` to learn
  the branches; and `search_prs` fetches a single page (limit capped at 100). Following `Link` headers,
  or resolving each hit through `GET /pulls/{n}`, would close both gaps at a cost in API calls.
- **No gRPC/web surface for update, delete, set-parents or adopt** — `pr-stack-full-control` is
  agent-only by decision, so the web keeps `AddPlannedPr` / `RepointPlannedPr` / `GetPrStatus` /
  `QueryBranch` while the agent now has strictly more. Bringing the four new operations to
  `connection.proto` and to `PlannedPrRow`'s action set would remove the asymmetry — and `PrStackScreen`
  is where an operator most naturally wants "delete this row" and "rename this row".
- **`pr_delete_planned` leaves the branch, the worktree and the child session behind** — deletion is a
  plan operation by decision; it reports the orphaned `branch` and `session_id` and stops. An opt-in
  cleanup (close the PR, delete the remote branch, remove the worktree, delete the child session) would
  make "abandon this node" one step instead of four. Related: *dangling `session_id` links are never
  scrubbed*, below.
- **`AddPlannedPrInput.child_recipe` is still inert** — accepted by the Rust struct and present in
  `connection.proto:1122`, but `StackNode` has no field to carry it, so it is discarded on every path
  (`pr_stack/mod.rs:377-381`), and the MCP schema does not even expose it. Either give `StackNode` the
  field and honour it when spawning the child, or delete the parameter from both the struct and the
  proto. Untouched by `pr-stack-full-control`.
- **`Stack::topo_order` still treats an unknown parent id as a no-op** — in-degree is counted only over
  parents that resolve to a node (`changeset.rs:105`), so a dangling parent reference is silently
  ignored and no validation ever rejects a persisted `Stack` that holds one. Only `validate_stack_plan`
  rejects dangling parents, and it runs on plan *input*, never on what is on disk.
  `pr-stack-full-control` works around this by validating the candidate stack in every new writer and by
  making delete reparent rather than orphan, but the underlying model still cannot represent
  "this stack is invalid".
- **`update_stack_atomic` takes no lock** — it is read-modify-write plus an atomic rename, so two
  concurrent writers are last-writer-wins on the whole changeset. Every writer works around it
  individually by computing inside the closure. With seven more mutating tools calling it, an advisory
  file lock (or a single serialized stack-writer) becomes worth the change.

### Terminal lazy scroll-up — LiveKit transport & unified surface (source: terminal-replay-viewport changeset, 2026-07-28)

- **LiveKit transport does not carry offset metadata.** `GhosttyTerminalGrpc` now owns the
  overlay double-buffer scroll-up paging via `GetTerminalHistory`, but `GhosttyTerminalLiveKit`
  still receives raw bytes only. Carry `end_offset`/`at_oldest` on the LiveKit terminal frames (or
  a side channel) so the LiveKit-backed terminal can use the same overlay paging flow.
- **Paged forward-fill.** The page terminal is filled with the entire retained capture
  (`0 → anchor`), which transfers all bytes even though the terminal retains only the last
  `scrollback` lines. Page the forward-fill to fill the scrollback budget only (skipping bytes
  that would be discarded).
- **Unified single-terminal surface.** The viewport integration uses two interchangeable,
  overlaid ghostty-web terminals (live at `scrollback: 0`, page at `scrollback > 0`) to avoid
  resetting the live terminal (which would reintroduce the duplicate-pane bug). A unified
  single-terminal surface is infeasible today because ghostty-web has no "insert at top of
  scrollback" API and a live reset is unacceptable; revisit if a future ghostty-web release adds a
  prepend API that does not require a live-terminal reset.
- **Persisted scroll position across reconnects** is out of scope; the forward fill populates the
  page terminal from offset `0` toward the anchor.

### Terminal native scrolling model (source: terminal-native-scrolling changeset, 2026-07-28)

Adopts the native ghostty desktop scrolling model in the web (live `scrollback > 0`, native
`Scrollbar {total, offset, len}` on the page terminal, native scroll-to-bottom policy,
mouse-tracking gating) — see `docs/dev/1-WIP/2026-07-28-terminal-native-scrolling.md`. Future
enhancements beyond that changeset:

- **Persisted scroll position across reconnects** — the live terminal lands at the live tip on
  reconnect and the page terminal fills from offset `0`; a future option can persist and restore
  the user's viewport position across sessions. (Also tracked above; kept here as the
  native-scrolling-scoped reference.)
- **Daemon-side PageList emulator** — the overlay double-buffer exists only because ghostty-web
  has no "insert at top of scrollback" API (a single terminal cannot lazily prepend older
  history). If a future ghostty-web release does not add a prepend API, a daemon-side PageList
  emulator that holds the terminal state and renders the visible window over RPC would give a
  true single-terminal surface (no overlay, no second instance).
- **Paged forward-fill** — the page terminal is filled with the entire retained capture
  (`0 → anchor`), which transfers all bytes even though the terminal retains only the last
  `scrollback` lines. Page the forward-fill to fill the scrollback budget only (skipping bytes
  that would be discarded). (Already noted above; kept as the native-scrolling-scoped reference.)

### PR-Stack — status polling and stack hygiene (source: pr-stack-ux-recovery changeset, 2026-07-26)

- **Manual verification against the live stack was never done** — orchestrator session
  `019f9dd5-716d-7071-96ac-464ff7b98c2a` on `uppin/tddy-coder`: confirm node `attach-store` recovers
  (its recorded session is gone, so the row must offer Start session pre-filled to resume
  `feature/session-attach-docs/attach-store`) and that PR #351 shows on
  `feature/session-attach-docs/attach-proto`. It needs the daemon **rebuilt and installed**,
  `auth_storage` (`/var/lib/tddy/auth`) **created and chowned to the daemon user**, and a **re-login**
  (the widened `read:user repo` scope). Every automated suite is green; this is the one unverified path,
  and it is also the end-to-end check that the token store and the `remote` leg work against a real
  repo rather than a fixture.
- **The repoint recovery was never run against a live stack** — deferred to after merge by the
  developer (source: `pr-stack-repoint-dead-end`, 2026-07-26). Exercise a genuinely stranded node — a
  predecessor whose PR merged on GitHub and whose branch was deleted, with the plan still recording
  `open` — and confirm the row offers "Repoint to `<default>`", that taking it drops the dead parent and
  leaves the node startable, and that a refusal shows its reason. Do it on **two** projects: one that
  stores `main_branch_ref` and one that does not. The second matters most: sending an empty target used
  to select a different rule server-side and silently do nothing, and no automated test covers that path
  end to end.
- **GitHub poll volume is one lookup per rendered branch per 5s** — `useQueryBranch` polls every branch
  a row renders. Adding each node's *base* branch to the poll set costs nothing extra: a base is by
  definition some node's own `branch` and was already in the set (`resolvedBranches` is deduplicated).
  The rate-limit problem was entirely the **two hooks polling the same fact** — `usePrStatus` and
  `useQueryBranch` both reaching the same authenticated `GET /pulls` — which is fixed by removing
  `usePrStatus`. What remains is a fixed 5s interval per branch: a ten-node stack is ~2 calls/second,
  which is comfortable against a 5000/hour user limit but still linear in stack size and unaffected by
  nothing having changed. Batch the resolution into one call per stack, cache per branch with ETags, or
  back off when the response is unchanged.
- **Dangling `session_id` links are never scrubbed** — `DeleteSession` leaves the deleted session's id
  on the orchestrator's stack node; the orphan state is derived at render instead. A periodic (or
  delete-time) reconciliation would make the stored stack match reality, which matters for anything
  reading the changeset directly rather than through the web.
- **`origin/<branch>` freshness depends on the last fetch** — the start-blocked warning reads the
  local remote-tracking ref, so a branch pushed from another machine reads as missing until this host
  fetches. A periodic background fetch, or a fetch-on-demand from the row, would close the gap. Since
  `pr-stack-repoint-dead-end` this also has a **destructive** consequence, not just a delaying one: the
  row will offer "Repoint to `<default>`" for a base that is actually alive, and taking it drops the
  parent edge from the plan for good.
- **Two checked-in generations of `connection.proto` disagree** — `packages/tddy-rust-typescript-tests/gen/connection_pb.ts` predates `RepointPlannedPr` entirely, while
  `packages/tddy-web/src/gen/connection_pb.ts` is kept current. Regenerating the former is a large diff
  unrelated to any one changeset, which is why it keeps being skipped. Source: `pr-stack-repoint-dead-end`.
- **A repoint names its target but not what it drops** — the control reads "Repoint to `<target>`" and
  collapses the node onto that single parent, dropping every other edge (intended, D18). The operator is
  never shown *which* predecessors that removes, and there is no undo. Naming them in the button's
  tooltip, or a confirm step when more than one edge would go, would make the cost visible.
  Source: `pr-stack-repoint-dead-end`.
- **`repoint_planned_pr_node` has three pre-existing silent-failure paths** — a failed `git rev-parse`
  collapses to an empty `expected_sha`, which git reads as "the remote ref must not exist" and turns a
  `--force-with-lease` into a guaranteed rejection; `merge_base` failure invents `effective_base`; and a
  force-push failure is only `log::warn!`, so the RPC returns success while `origin/<branch>` still points
  at the old base and the PR was re-targeted anyway. Untouched by `pr-stack-repoint-dead-end`, which only
  moved them inside a branch guard. Source: `pr-stack-repoint-dead-end`.
- **A `tddy-coder`-embedded web server retains no GitHub token** — `packages/tddy-coder/src/run.rs`
  builds its own `AuthServiceImpl` without a `GitHubTokenStore`, so PR status there reads
  *unavailable* even after a real login. The daemon path is wired; this one is not.
- **`start_sandboxed_cursor_cli_session` takes no `stack_parent`** and calls neither
  `resolve_chain_base_ref` nor the node link, so a cursor-cli session cannot be a PR-stack child at
  all. Wiring it is a larger gap than this changeset, not an oversight in it.
- **Nothing tests that `qualified_head` is *applied* at the two call sites** — only that the function
  itself is correct. `get_open_pr` / `get_pr_by_head` reach `api.github.com` through free `curl`
  helpers with no injection seam, so pinning the outgoing `head` value would need either a network
  call or an HTTP-transport abstraction. Worth adding the seam if that module grows.

### tddy-web — Agent Activity tool-detail dialog (source: acp-tool-detail-explicit-states changeset, 2026-07-26)

- **No retry affordance on a failed body lookup** — `AgentActivityDetailDialog` reports a failure inline
  and nothing is cached, so a retry *is* possible but only by closing and reopening the row. A "Retry"
  button in the error block would make that discoverable.
- **The tool-detail cache has no per-session entry cap** — `AgentActivityRegistry`'s
  `MAX_SESSIONS = 100` LRU is the only bound, so a session in which the operator opens very many tool
  rows retains every fetched body for the page's lifetime. Bound it per session if a heavy transcript
  ever shows growth (related: the persisted-log size caps tracked below).
- **The `animate-pulse` skeleton is inline in the dialog** rather than a shared UI primitive — it is the
  only skeleton in tddy-web today. Promote it to `components/ui/` when a second surface needs one.

### tddy-sandbox-recipes — host-independent sandbox path tests (source: acp-tool-detail-explicit-states changeset, 2026-07-26)

- **`cursor_agent_prerequisite_reads` asserts a machine-specific path** —
  `packages/tddy-sandbox-recipes/src/cursor_cli.rs:509` asserts `/Users` appears among the path-traversal
  ancestor grants. That only holds where `HOME` sits under `/Users` (macOS); on Linux `HOME=/var/tddy`, the
  ancestors end at `/var`, and the test fails permanently. Introduced by c018a176 (#303) and already on
  `master`, so **no Linux developer can get a clean `cargo test --workspace`** — which trains everyone to
  ignore workspace failures.
  - **Tests should stub their environment** rather than read the ambient one: have
    `cursor_agent_prerequisite_reads` take the home/share roots (or resolve them through an injectable
    provider) so a test can pass a `tempfile::tempdir()` root and assert the *shape* of the ancestor chain —
    "every ancestor from the install dir up to the filesystem root is granted" — instead of a literal
    prefix belonging to one OS.
  - **Production code should be testable by design**: the same seam removes the hidden `HOME` dependency
    from the recipe, which is the actual reason the assertion had to name a real directory.
  - Audit sibling recipes for the same pattern before fixing just this one assertion.

### tddy-service / tddy-coder / tddy-build (source: bsp-build-server changeset, 2026-07-22)

- **Literal JSON-RPC 2.0 BSP transport** — the `bsp.BspService` is BSP-*shaped* over the workspace's
  protobuf/Connect + LiveKit RPC, so external BSP clients (Metals, IntelliJ-BSP) cannot connect. A real
  JSON-RPC 2.0 BSP server (`build/initialize`, `workspace/buildTargets`, …) over stdio/TCP with a `.bsp/`
  connection file would require a new transport.
- **Structured build diagnostics** — build ops return exit code + raw stdout/stderr only. Parse compiler
  output into `{file, line, severity, message}` diagnostics for `BuildTargetCompile`/`Test` responses.
- **Remaining BSP methods** — `buildTarget/inverseSources`, `dependencySources`, `dependencyModules`,
  `resources`, `debugSession/start`.
- **Unify shared `catalog` build_target rows with `build_targets`** — the populate task currently writes
  build targets into both the shared `catalog` table (lightweight, for the unified `list`) and the new
  `build_targets` table (rich, for BSP). Collapse the duplication via a SQL view once the read paths agree.
- **Streaming compile/test progress** — the initial compile/test/run methods are unary; add BSP-style task
  progress notifications (server-streaming) for long builds.
- **BSP per-request session construction (no cache)** — the daemon's `DaemonBspService` builds a fresh
  `BspServiceImpl` per request (each triggers a catalog open+populate). Fine for correctness; add a
  per-session cache (keyed by resolved `session_dir`, with eviction) if the read path gets hot.
- **Silent provider lowering failures** — `tddy-bsp`'s provider swallows a target's lowering error and lists
  it with empty sources/outputs (no `log` dependency on `tddy-bsp`). Add `log` and a `debug!` on failure if
  that observability is wanted.

### tddy-core / tddy-coder / tddy-daemon (source: session-catalog changeset, 2026-07-22)

- **`list_action_summaries` read-path cutover** — make the per-session catalog the sole read source: replace the query-time YAML glob in `packages/tddy-core/src/session_actions/list.rs` with a read from `SessionCatalog`. The producer (`PopulateCatalogTask`) and the consumers (`list-actions` listener / `tddy-tools` CLI / `tddy-sandbox-app` host) run in **different processes**, so this needs cross-process `catalog.db` reads via the durable `meta['populated_at']` marker + `CatalogError::PopulateTimeout` bounded wait, sync→async at the 3 call sites, and a lazy-populate-on-read model for the owner-less standalone CLI fallback.
- **Daemon populate trigger** — spawn `SessionCatalog::open_and_populate` in `tddy-daemon` `spawn_claude_cli_session_inner` on worktree-open (threads the shared `TaskRegistry` through the `self`-less free function; 3 call sites). The coder already triggers populate; the daemon-managed flow does not yet.
- **`SessionCatalog`/`CATALOG` lifecycle** — the process-global `DashMap` of per-session catalogs has no eviction (each holds a `SqlitePool`); add eviction on session-close, and add a bounded wait to `SessionCatalog::list` so a panicked populate task cannot hang a reader indefinitely.

### tddy-workflow-recipes / tddy-discovery (source: exploration-artifact changeset, 2026-07-21)

- **Prime `exploration.md` from the FastContext discovery subagent** — the discovery agent already returns `path:line-start-line-end` citations (`docs/ft/coder/discovery-agent.md`); seed the exploration artifact from those citations before the plan agent starts so plan-time exploration begins pre-warmed.
- **Structured exploration entries in `changeset.yaml` discovery** — extend `DiscoveryData.relevant_code` with line/col-aware references sourced from the exploration document, keeping a machine-readable mirror of the markdown.
- **Staleness detection for exploration line references** — flag `exploration.md` code references invalidated by later diffs (e.g. compare against `git diff` ranges in post-green steps) so downstream agents know which references to re-verify.

### tddy-github / tddy-daemon (source: cross-daemon-session-token changeset, 2026-07-04)

- **Refactor `TelegramOAuthStateSigner` to reuse the generic HMAC signer** — `packages/tddy-daemon/src/telegram_github_link.rs:48-135` hand-rolls the same HMAC-SHA256 sign/verify pattern that the new `SessionTokenSigner` (`packages/tddy-github/src/session_token.rs`) generalizes. Once the session-token signer lands, collapse the telegram state signer onto it.
- **Server-side session-token revocation / denylist** — signed tokens are only bounded by their 5-minute TTL; there is no way to revoke a leaked token before it expires. Add a shared (room-propagated) denylist only if leaked-token containment becomes a requirement.

### tddy-sandbox-app / tddy-sandbox-runner (source: claude-sandbox-launcher changeset, 2026-07-03)

- **Integration/acceptance test for `./claude-sandbox` full launch with an inline Ollama def** — the launcher was verified by a manual full-launch smoke test (config loads → `codebase_mode=managed` → inline `fastcontext` activated, end-to-end through a real macOS Seatbelt jail), but the interactive terminal-attach path was not exercised in CI and no automated regression test drives a full sandboxed launch with an inline Ollama `fastcontext` def. The launcher script, `tddy-sandbox-app --config`, the egress shim's plain-HTTP forward proxy, persisted `tddy-tools.mcp.log` + `latest` symlink, and the `--disallowedTools` + server-side replaced-tool enforcement are all landed; what's missing is a CI-runnable test that asserts the whole stack comes up and a subagent turn completes against a stubbed Ollama. Knowledge transferred to `docs/ft/coder/managed-codebase-subagents.md` § Standalone launcher; source changeset `docs/dev/1-WIP/claude-sandbox-launcher.md` removed after wrap.
- **Split the Standalone launcher section into its own `docs/ft/coder/claude-sandbox-launcher.md`** — the launcher/config/egress/observability/enforcement knowledge currently lives as a section of `managed-codebase-subagents.md`. If it outgrows that file, lift it into a dedicated feature doc.

### tddy-coder (source: subagent-tool-replacement changeset, 2026-07-02)

- **Extend subagent tool-replacement to the `--remote` path** — `packages/tddy-coder/src/remote.rs`'s `RemoteContextDir`/`REMOTE_APPENDIX` and `build_remote_allowlist` have no subagent concept at all today (no `SUBAGENT_TOOLS`, no `subagent_*` wiring in `run_remote`). Extending the tool-replacement mechanism there requires wiring subagent support into that path first — deferred to keep this changeset scoped to the sandbox/daemon managed path where subagents already exist.
- **Per-tool replacement policies** — the replaced-tool set is a flat list today (e.g. all of `Grep`, unconditionally). A future refinement could scope replacement (e.g. only for certain file types or path prefixes) rather than an all-or-nothing per-tool switch.

### tddy-vm / tddy-daemon (discovered while verifying the subagent-tool-replacement changeset, 2026-07-02)

- **`vm_service_acceptance.rs`'s `BuildVmImage` tests hang in the standard nix dev shell** — `build_vm_image_adapter_still_delivers_progress_messages`, `build_vm_image_streams_progress_messages`, and `vm_build_task_appears_in_registry_after_build_call` (`packages/tddy-daemon/tests/vm_service_acceptance.rs`) all assume `BUILDROOT_DIR` is unset so `run_buildroot_pipeline` (`packages/tddy-vm/src/build.rs:968-976`) takes the fast `STAGE_ERROR` path. `./dev`'s nix shell unconditionally exports `BUILDROOT_DIR` to a real Buildroot source tree, so these tests instead fall through to a real `make olddefconfig`/`make -j<nproc>` build (via Docker on macOS) — effectively hanging (or taking a very long time) rather than failing fast. Pre-existing, unrelated to any code in this changeset; not fixed here because the right fix (mock/stub the pipeline at a lower level, or have the dev shell not export `BUILDROOT_DIR` for test runs) needs a decision from whoever owns the VM-build feature. Workaround used during this changeset's verification: `cargo test -- --skip build_vm_image_adapter_still_delivers_progress_messages --skip build_vm_image_streams_progress_messages --skip vm_build_task_appears_in_registry_after_build_call`.

### tddy-sandbox-cgroups (source: finish-stdio-ipc-migration changeset, 2026-07-02)

- **Verify `--stdio` jail-spawn piping through a real Linux jail** — `spawn_plan` now pipes
  stdin/stdout (instead of leaving stdout on its prior default) when `--stdio` is in the command,
  mirroring `tddy-sandbox-darwin::spawn_plan`. Compile-checked only (the crate is
  `#[cfg(target_os = "linux")]`-gated and the dev environment that made this change has no Linux
  box); needs a real-jail run in Linux CI to confirm the daemon's now-stdio-only session control
  channel (`docs/dev/1-WIP/finish-stdio-ipc-migration.md`) actually works cross-platform.

### tddy-sandbox-app (source: specialized-subagents changeset, 2026-07-02)

- ~~`--specialized-agent` CLI flag + deprecated aliases~~ — done (2026-07-02, multi-agent tool-replacement changeset; overrides and the deprecated alias fully removed 2026-07-02 in a follow-up cleanup — see below). `tddy-sandbox-app` takes repeatable `--specialized-agent <name>` + `--agents-dir`, resolves them via `spawn::resolve_specialized_agents`, and threads the resolved array into the jail as `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` via `spawn::subagent_env_overlay`. There is no `--discovery-subagent` alias and no `--fastcontext-*`/`--subagent-replaces` override flags — all configuration comes exclusively from the resolved agent's YAML def. See `docs/ft/coder/managed-codebase-subagents.md` and `docs/ft/coder/specialized-subagents.md`.
- ~~**`--agent` CLI validation for custom specialized-agent names (`tddy-coder`)**~~ — **done** (models-and-assistants changeset, 2026-08-16). The clap `value_parser` allowlist on `--agent` is removed; validation moved into `create_backend`, whose former `_ => Claude` catch-all is now an error naming the known agents. A registry assistant or a `<tddyhome>/agents` def is accepted by name.

### tddy-core (source: stdio-transport-for-grpc-binaries changeset, 2026-07-01)

- **Migrate the toolcall listener to tddy-rpc/tddy-stdio** — `tddy-core/src/toolcall/listener.rs` is a third bespoke newline-delimited-JSON protocol (`submit`/`ask`/`approve`/`list-actions`/`build`) between `tddy-coder` and the Claude Code CLI subprocess it spawns, distinct from the sandbox tool-IPC and gRPC-over-UDS relay this changeset migrates. Same category of problem, same fix would apply, deferred to keep this changeset scoped.

### tddy-daemon (source: stdio-transport-for-grpc-binaries changeset, 2026-07-01)

- **Switch `tddy-daemon`'s real session lifecycle onto the stdio transport** — `connection_service.rs`'s spawn/dial orchestration and `sandbox_session.rs`'s `dial_and_bridge` still spawn `tddy-sandbox-runner` with `--grpc-uds`/`--grpc-listen-port` and dial the tonic `SandboxServiceClient`, for every real sandboxed session. All the primitives to switch this over are built and proven end-to-end through a real Seatbelt jail (`bridge_sandbox_stdio`, `StdioSandboxClient`, transport-agnostic `run_host_relay`) — what remains is purely wiring the daemon's own spawn/dial call sites in `connection_service.rs`, deferred because that file's orchestration is large (1300+ lines) and the switch changes live transport behavior for every real session. Once done, `--grpc-socket`/`--grpc-uds`/`--grpc-listen-port` and their port/path-allocation code (`pick_free_loopback_port`, the `ready_marker` polling handshake) can be deleted outright — no dual-path fallback, per this repo's convention.
- **Linux (`tddy-sandbox-cgroups`) jail-spawn stdio piping** — `tddy-sandbox-darwin::spawn_plan` was updated to pipe stdin/stdout (instead of redirecting stdout to an egress log) when `--stdio` is in the command; `tddy-sandbox-cgroups` needs the equivalent change on Linux. Not attempted in the original changeset because that crate is `#[cfg(target_os = "linux")]`-gated and couldn't even be compile-checked on the macOS dev environment that did the work, let alone verified through a real jail.

### tddy-sandbox-cgroups (source: sandbox-builder changeset, 2026-06-28)

- **Minimal RO-root `pivot_root`** — the sandbox-builder changeset lands read-only bind-mounts of each declared `ReadSpec` inside the rootless jail, but the jail still shares the host filesystem root. Build a minimal tmpfs root, bind only the plan's reads + writable project/scratch/egress, then `pivot_root` into it for full filesystem write-confinement.

### tddy-build (source: tddy-build-bazel-system changeset, 2026-06-16)

- **Distributed cache / parent-fallback** — remote shared cache layer (maker-build pattern). Deferred to v2.
- **Hermetic sandboxing** — isolate action execution; v1 uses PATH + cwd discipline only.
- **Full remote build execution** — `TDDY_SOCKET` relay covers co-located sessions; true remote/distributed build deferred.
- **Watch mode** — incremental rebuild on file change.
- **Output-publication convention** — finalize the published-artifact layout (maker-build publishes to `dist/{name}/`); v1 stages under `.tddy-build/out/{target_id}/` only.
- **Cross-compilation architecture filter** — port `ensure_action_architecture()` from `session_actions` for ToolTargets that ship per-arch binaries.
