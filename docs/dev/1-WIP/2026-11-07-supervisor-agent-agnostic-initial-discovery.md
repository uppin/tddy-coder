# Initial discovery — supervisor-agent-agnostic

Exploration 1 is the whole-work discovery for the #agent-tools leftovers stack (21–28), copied in
full. Exploration 2 is the orchestrator's read-only discovery of this node at #836's tip
`ddef5cc6b`: its 11-item inventory, re-verified by line. Exploration 3 lists what that check
corrected or added.

## Exploration 1 — whole-work discovery (copied)

### Initial discovery — #agent-tools leftovers (whole work, temporary)

**Base:** `yolo-pr-stack-agent-tools-6465b5be` (PR #828, `#agent-tools 20`) at `e6ff7012e`.
n1–n19 (#790–#825) are merged to master; #828 is the YAML-only sweep. This stack's
three nodes stack on #828 and close the stack TODOs #828 left open.

Not staged on any branch. Copied into each node's companion as Exploration 1.

### Conclusions

_(filled once the explorations return)_

### Decomposition (user-approved 2026-11-06)

1. **Bounded waits.** ACP launch waits, the generic session start, supervisor
   `call_unary`, the `clone_askpass` host-key test hang.
2. **Env and session sockets.** Split agents get `TDDY_REMOTE_*`, the headless
   conversation tool-server env, the Codex OAuth env, agents start from a clean
   env, session-pointer hazards, SSH pointer exclude/removal, jail tool-IPC socket
   collision, server-daemon agent tool socket per OS user.
3. **Registry plumbing.** `DaemonSessionHost.coding_agents` no longer an `Option`,
   Telegram loads the registry once, a test pins the catalog probe to host agents,
   choose-agent offers ACP agents to PTY sandbox, split sessions render the
   embedded spec, vestigial runner model flag, `relaunch_jail_*` trio,
   cli-spec-features leftovers, ACP hand-built launch path, OpenCode ACP route
   file owner.

### Exploration 1 — bounded waits (Explore, sonnet · medium, read-only, no execution)

1. **ACP launch waits** (`2026-11-04-acp-launch-waits-have-no-deadline.md`): HALF STALE.
   - open_session FIXED: `SESSION_OPEN_DEADLINE` 120 s, `acp_driver.rs:227`, `open_session_within` :420, test `tests/spec_backend_integration.rs:24,95` (`that_never_answers`). Record needs narrowing only.
   - Supervisor STILL TRUE: `tddy-supervisor/src/client.rs:291-292` `call()` → `transport.call_unary(...)` unbounded; `spawn_relayed` (:150) reaches it. NOT in record: `clone_repository` (:198-246) `call_server_stream(...).await` + first `frames.recv().await` unbounded.
   - Attach: `SupervisorClient` (connect :58) has no config; `SupervisorError::Unavailable { message }` (error.rs) is the named error; crate style is module `const X: Duration` (clone.rs:35 `STDERR_DRAIN_AFTER_EXIT`). Daemon side already wraps some calls with `spawn_worker_request_timeout()` (daemon-kernel config/daemon_methods.rs:138; session-split service_util.rs:195,227).
   - Seam: `tddy-supervisor/tests/support/mod.rs`, pattern `a_git_that_never_finishes` (supervisor_clone_progress.rs:184); silent peer = raw UnixListener that accepts and never replies (transport is concrete `Arc<StdioRpcClient>`, no fake). Size S (M with clone stream).
2. **Generic session start** (`2026-11-06-generic-session-start-has-no-deadline-of-its-own.md`): STILL TRUE. `operator_start.rs:54` passes `CancellationToken::new()` into `start_here` (svc_start_session_core.rs:144); `start_session_core` same. Only outer bound is Telegram `START_DEADLINE` 600 s (cli_spawns.rs:12,93). Attach: wrap `start_here` body, cancel token on expiry; knob = new `DaemonConfig` method next to `spawn_worker_request_timeout` (clamp 0→1 s; test config.rs:541). Seam: agent-launch start tests (`host_session_socket_tests.rs`, `LaunchSessions` fakes) — a never-returning start step fake must be built. Packages: tddy-agent-launch, tddy-daemon-kernel (opt. telegram-control). Size M.
3. **clone_askpass host-key**: NOT CONFIRMED from code. Test `a_host_key_confirmation_request_is_refused_and_the_clone_fails_in_seconds` (agent-launch tests/clone_askpass_acceptance.rs:172), real sshd fixture (tests/support/sshd.rs), `StrictHostKeyChecking=ask`; guard `A_CLONE_THAT_HANGS` 60 s (tests/support/rig.rs:25,101). Planner sets `BatchMode=yes` only without askpass (clone_planner.rs:305,361); askpass branch :345 (`SSH_ASKPASS_REQUIRE=force`). Needs a manual run before planning.
4. Other todo records: only the ssh-session-pointer record mentions a deadline (belongs to the env/sockets node).

Overlap: item 2 edits `svc_start_session_core.rs`, which also holds the `coding_agents` model check — keep it a thin wrapper. Supervisor item isolated.

### Exploration 2 — env and session sockets (Explore, sonnet · medium, read-only, no execution)

All six records still true at `e6ff7012e`.

1. **Headless conversation tool-server env** (`2026-11-05-headless-conversation-tool-server-env.md`), M. `tddy-session-lifecycle/src/coding_agent_launcher.rs:317-337` sets `forwards_server_env` for `McpSlot::Inline` → `env.extend(server.env…)`; `HeadlessCommand::env` carries `TODO(headless-driver)` (`tddy-agent-backend/src/backend/headless_driver.rs:53-59`). Fix: per-conversation `.tddy/session.json` (socket, conversation_id, minted token); inline slot names no env; drop `forwards_server_env`. Needs new `SessionPointer` fields. Packages: session-lifecycle, agent-backend (`session_pointer.rs`, `turn_files.rs`), tddy-tools (`session_source.rs`). Seams: `tddy-session-lifecycle/tests/headless_subagents_from_spec_acceptance.rs`, `tddy-agent-backend/tests/headless_renderer_port.rs`, `remote_env_acceptance.rs`.
2. **Codex OAuth `TDDY_CODEX_OAUTH_OUT`** (`2026-11-05-codex-oauth-out-env-in-headless-agent.md`): belongs to the Codex OAuth follow-up stack, OUT OF SCOPE. `codex.yaml:82-87`, `codex.rs:64`; second leak not in the record: `codex.rs:67` sets `TDDY_SESSION_DIR`.
3. **Pointer lifecycle hazards** (`2026-11-05-session-pointer-lifecycle-hazards.md`), M. `tddy-agent-backend/src/turn_files.rs:41` replace semantics, `Drop` :101 removes; pointer `{socket, repo_dir, session_dir, remote, notify}` (`cli_session_pointer.rs:60-66`) has no owner. `tddy-tools/src/session_source.rs` `UnreadablePointer` :52,:93, never checks socket/owner. Fix: `owner_pid`; pointer-source resolution fails with a named cause on dead pid / missing socket (never for `--socket`); optionally refuse a second pointer for another socket. Seams: `tddy-tools/tests/session_source_unit.rs` (:91, :161), `tddy-tools-cli/tests/session_pointer_submit.rs`, `tddy-agent-launch/src/cli_session_pointer/cli_session_pointer_unit_tests.rs`. Sequence: with/before item 1 (both change the pointer format).
4. **ssh pointer exclude and removal** (`2026-11-06-ssh-session-pointer-exclude-and-removal.md`), M. `tddy-agent-launch/src/cli_session_pointer.rs:84-86` `TODO(cli-dispatch-collapse)`; remote via `write_slot_file_over_ssh` (no removal); local uses `exclude_from_git` (:97), `0o600` (:31). Fix: one bounded ssh command (exclude + 0600 write, same deadline/keep-alives as slot write) and a bounded removal on exit and delete (`svc_signal_delete_session.rs`). Seam: `cli_session_pointer_unit_tests.rs` + ssh stub as in `tests/clone_ssh_env.rs`.
5. **Jail tool-IPC socket collision** (`2026-11-06-jail-tool-ipc-socket-collides-across-concurrent-sessions.md`), S. `tddy-sandbox/src/spec.rs:37-47` takes first 10 alphanumerics (= UUIDv7 timestamp prefix) + pid. Fix: random tail or FNV hash of the whole id, within SUN_LEN. Callers: `tddy-agent-launch/src/jail_launch.rs:89`, `tddy-sandbox-app/src/spawn.rs:392`. Seam: unit test in `spec.rs` — two UUIDv7 ids within 256 ms.
6. **Server-daemon agent tool socket per OS user** (`2026-11-03-server-daemon-agent-tool-socket-per-os-user.md`), L, BLOCKED on a developer design decision (A needs a supervisor protocol change; WIP ref `wip/agent-tools-cursor-cli-server-socket`). `tddy-daemon/src/runtime/build_context.rs:148-153` binds only for `RuntimeHost::Embedded`.

**`TDDY_REMOTE_*` to split agents:** built in `tddy-session-split/src/split_session.rs` (`split_remote_tool_env` :211, `_from_join` :183, sandboxed variant :226-250, called :351); delivered via `coding_agent_wiring.rs:30-55`, `tddy-agent-launch/src/cli_agent_steps.rs:644-652` (routed correctly: TDDY_* to MCP env), `jail_launch.rs:123`, headless Codex (item 1), the pointer `remote` map (`cli_session_pointer.rs:57`). Asserted in `claude_cli_spawn_steps_tests.rs:286-317`, `tddy-agent-backend/tests/split_session_env_acceptance.rs`.

**Agent-child env construction sites:**
| Site | Kind |
|---|---|
| agent-backend `backend/turn_process.rs:35-62` (`strip_tddy_env` + `.envs(turn.env)`) | deny-list |
| agent-backend `backend/spec_headless.rs:416` (model listing) | deny-list |
| agent-backend `backend/acp_driver/worker.rs:281` | inherit-all, no strip |
| agent-backend `backend/codex.rs:64,67` | adds TDDY_* |
| agent-backend `adapters/conversation_id.rs:97` | inherit-all |
| agent-launch `spec_launch.rs:329-345` (`env -u`) and `:358-375` (tmux unset) | deny-list |
| agent-launch `jail_launch.rs:346` | deny-list |
| tddy-spawn `spawner.rs:1054-1061` | inherit-all |
| sandbox-runner `exec_plan.rs:129` | deny-list (runner prefix) |
| sandbox-runner `runner.rs:1107`, sandbox-cgroups `lib.rs:135,291` | allow-list |
| tool-engine `contained_shell.rs:110` (pinned by `tests/exec_shell_no_tddy_env.rs`) | — |

Overlap: the ACP worker spawn (`worker.rs:281`) is where both an env allow-list and a deadline would land; the ssh pointer fix reuses `SLOT_FILE_OVER_SSH_DEADLINE` unchanged. The allow-list conversion is L and risky (PATH, HOME, provider keys) — its own decision.

### Exploration 3 — registry plumbing (Explore, sonnet · medium, read-only, no execution)

1. **Telegram loads the registry again** — STILL TRUE, 3 loads (record says 4): `tddy-daemon/src/runtime/telegram_wiring.rs:90-105` (`TODO(sweep)`, called :203), `mount_host.rs:63` (reserved ids), `mount_host.rs:269-270` (`build_session_host`). Fix: load once, fallibly, in/before `Registries::open`; share the `Arc`; start refuses on error; drop Telegram's empty-registry branch. tddy-daemon only. S–M.
2. **`choose_agent` offers ACP agents to PTY sandbox** — STILL TRUE, needs product decision. `tddy-sandbox-app/src/agent_spec.rs:113`; pinned by `tests/agent_from_spec_acceptance.rs:248-273` (`known` includes claude-acp, codex-acp). S.
3. **Split sessions render the embedded spec** — STILL TRUE. `tddy-coding-agent-specs/src/lib.rs:105` `TODO(jail-agnostic)`; callers `tddy-session-split/src/agent_argv.rs:7,182`, `split_session.rs:816`. Fix: thread the host registry into `split_claude_extra_args` (split start, `SplitAgentWiring` resume, sandboxed-codebase start). M. Second half — `TDDY_REMOTE_*` off the split agent's process into pointer + MCP env — L, overlaps env/pointer.
4. **Runner `--model` vestigial** — STILL TRUE. `tddy-sandbox-runner/src/runner.rs:290-296`; writers `tddy-daemon-sandbox/src/sandbox_plan_builder.rs:216`, `tddy-sandbox-app/src/spawn.rs:459`; tests `runner.rs:2484-2566`, `spawn.rs:813`, `sandbox_session.rs:934`. S. #828's tail touches sandbox-app daemon_client — rebase first.
5. **cli-spec-features leftovers** — MOSTLY FIXED; record stale. Open: ssh pointer exclude (`cli_session_pointer.rs:84`, tracked by the ssh-pointer record); a managed agent with no file-placed `mcp` slot gets no `TDDY_*` (untraced).
6. **ACP hand-built launch path** — LARGELY FIXED (`coding_agent_launcher.rs:159` non-optional `Arc`). Open in `tddy-coding-subagent`: `withdrawal_session_meta` default of `session_meta_for` (`launcher.rs:230-240`, `TODO(acp-agents)`); `opencode_withdrawal_config`, `CLAUDE_NATIVE_TOOLS`, `ACP_HARNESSES` in `confinement.rs` (also used by catalog.rs, transports.rs, lib.rs); exercised by `tddy-coding-agent-specs/tests/acp_slots_unit.rs`. M.
7. **OpenCode ACP route file has no session owner** — STILL TRUE. `coding_agent_launcher.rs:385-387`; inline `OPENCODE_CONFIG_CONTENT` because `CodingAgentSession::start` runs `NoHooks`. Fix: hooks owner with session lifetime. Seam `tests/acp_agents_from_spec_acceptance.rs`. M.
8. **Legacy CLI harness** — FIXED by #828 except the dead `relaunch_jail_*` trio in `tddy-agent-launch/src/relaunch_jail_steps.rs` (:61, :94, :156, `#[allow(dead_code)]`, `TODO(harness-clis)`) + structs in `svc_relaunch_sandboxed_runner.rs` (:8, :19, :35). `relaunch_jail_dirs` is LIVE (`svc_resume_sandboxed_cli_harness_session.rs:26`). Also `hooks: unsupported` TODOs in `specs/{opencode,muse,warp}.yaml:41-44`, `slots/hooks.rs:59`. S.
9. **Callerless bespoke launch helpers** — STILL OPEN (#828 did not close): `jail_launch_steps.rs:74,165`; `jail_worktree.rs:18,42,90`; `claude_cli_spawn.rs:29` (`create_routed_session_dir`, test-only caller `session_provider_route_tests.rs`); `chat.rs` `cursor_notify_env`, `resumed_cursor_notify_env`, `cursor_hook_daemon_url`. Deletions need approval. S–M.
10. **Model listing + bespoke ACP backends** — PARTLY FIXED. `CodexAcpBackend` still in `tddy-agent-backend/src/backend/codex_acp.rs` (+ `codex_acp/oauth.rs`, `adapters/codex_acp_login.rs`); `ClaudeAcpBackend` named by `tddy-integration-tests/tests/acp_backend_acceptance.rs`, `tddy-acp-stub/src/main.rs`; `tddy-coder/src/acp_from_spec.rs:79-110` `*_from_spec` builders; integration tests import `resolve_agent_models`. M.

**(a) `DaemonSessionHost.coding_agents`** still `Option` (`connection_service.rs:240`, default None `svc_host_builders.rs:125`, wiring `svc_coding_agents_wiring.rs:9-50`). None→empty fallbacks: `daemon_rpc_handler.rs:288-290`, `svc_resolve_os_user.rs:76-80`, `svc_activity_ports.rs:87-90`; Option pass-through `svc_terminal_ports.rs:123,175`, `mount_accounts.rs:69` → `account_linking.rs:266`. Pinned refusal test `stack_child_spawn_agent_tests.rs:383`. ~10 test files use `with_coding_agents_if_none`. M; pairs with item 1.
**(b) Model probe** NOT pinned to the host: `daemon-rpc/src/catalog/ports.rs:114-123,201-213` uses host registry only for the binary; `tddy-tools-cli/src/main.rs:300` lists from `builtin_cached()`, so an operator `tools/<agent>.yaml` model override is ignored. No test covers it. S–M (changes `tddy-tools` CLI surface).

## Exploration 2 — what in the supervisor knows about agents, env or git (read-only, at `ddef5cc6b`)

All paths are under `packages/tddy-supervisor/` unless they say otherwise. Every reference was re-read at `ddef5cc6b`.

1. **`allowed_relay_tool_paths`.**
   - `src/config/spawn_policy.rs:14-18` documents it as "coding-agent harness binaries … relayed".
   - `src/config.rs:352-354` checks that it is absolute.
   - `src/policy.rs:114-131`: `resolve_relay_tool_path`, a verbatim match.
   - `src/server.rs:94-99`: `spawn_relayed` checks it first.
2. **Agent names in the templates.**
   - `supervisor.yaml.production:103-116` names `codex-acp`, `opencode` and `claude-agent-acp`, plus `TDDY_CLAUDE_AGENT_ACP_CLI` and the bunx warning.
   - `dev.supervisor.yaml:68-74` does the same.
3. **`allowed_env_keys`.**
   - `src/config/spawn_policy.rs:22-29` defines it.
   - `src/config.rs:358-367` rejects loader keys in it.
   - `src/policy.rs:145-169` (`resolve_env`) refuses a key it does not list.
   - `src/server.rs:200-203` applies it to every spawn through `resolve_spawn_of` (:190).
4. **Key lists in the templates.**
   - `supervisor.yaml.production:117-140` covers the clone SSH keys, the commit identity and `TDDY_GITHUB_SOURCE`.
   - `dev.supervisor.yaml:76-87` is the same, with a live `allowed_env_keys` example at :87.
5. **#836's surface.** `policy::resolve_relayed_env` (`src/policy.rs:171` and on) and `EnvironmentBase::Exactly`. At `ddef5cc6b` these are contract stubs: `server.rs:100-101` still carries `TODO(env-allow-list)`.
6. **The `SESSION_PATH` doc.** `src/spawn_broker.rs:150-160`: a fixed `PATH` `/usr/local/bin:/usr/bin:/bin`, "a host whose binaries live somewhere else grants `PATH` through `allowed_env_keys`", and `FORWARDED_VARIABLES = [LANG, TZ]`.
7. **`CloneRepository` knows git.**
   - `src/clone.rs:1-12` is the module doc.
   - `src/clone.rs:57-71`: `git_on_supervisor_path`.
   - `src/clone.rs:222-248`: `move_into_place`, an `std::fs::rename` done by the root supervisor next to a destination the session user can write.
   - `src/clone.rs:250-261`: `remove_partial`.
   - `src/clone.rs:291-295`: `is_progress_redraw` (the `%` heuristic).
   - The server side is `src/server.rs:232-288`, which builds `git clone --progress -- <url> <partial>`.
   - In the proto: `proto/supervisor.proto:44-48` (the rpc) and `:146-181` (`CloneRepositoryRequest`, `CloneEvent`, `CloneStarted`, `CloneProgressLine`, `CloneExited`).
8. **The relay is described as a coding-agent harness** in `proto/supervisor.proto:27` and in the module doc `src/relay.rs:3`.
9. **The sockets and `allowed_session_users`** belong to #859 (`#agent-tools` 31), not this node.
10. **No cgroup scope.**
    - `src/server.rs:102-108`: `spawn_relayed` passes `None` as the scope (:106).
    - `src/server.rs:244-250`: `clone_repository` passes `None, None` (:247-248).
    - `SpawnSession` and `SpawnSandbox` pass `request.scope` (:75, :146).
11. **`allowed_tool_paths`.** `src/config/spawn_policy.rs:11-13`; `src/policy.rs:98-112` (`resolve_tool_path`, verbatim).

**Daemon side:**
- `packages/tddy-spawn/src/clone_env.rs:1-37`: `CloneSshEnv::ALLOWED_ENV_KEYS`, seven keys.
- `packages/tddy-spawn/src/supervisor_spawn.rs`: the module doc :1-20, and `clone_repo_via_supervisor_with_progress` from :254, whose doc at :250 says git "must be in `allowed_tool_paths`".
- `packages/tddy-spawn/src/git_progress.rs` already parses git's progress.
- `packages/tddy-session-lifecycle/src/coding_agent_relay.rs`:
  - the module doc :9-11 and `relay_refusal_error` :101-111 name `allowed_relay_tool_paths`, `allowed_session_users` and `allowed_env_keys`;
  - the tests assert them at :322, :331 and :340.
- `packages/tddy-daemon-rpc/tests/supervisor_clone_delegation.rs`.
- `packages/tddy-supervisor/tests/supervisor_call_deadline.rs:119` uses a `/usr/bin/claude` fixture.

**Feature docs:**
- `docs/ft/supervisor/tddy-supervisor.md`;
- `docs/ft/daemon/systemd-install.md:45`;
- `docs/ft/daemon/session-agent-roster.md:272-276`;
- `docs/ft/daemon/clone-ssh-passphrase.md:67,122,140`;
- `packages/tddy-supervisor/docs/architecture.md`.

**Reusable code:**
- trust checks `require_trustworthy_config_file` and `require_trustworthy_config_directory` (`src/config.rs:380-447`);
- `open_directory_reference` and `open_resolved` (`O_PATH`, `openat2`; `src/spawn_broker.rs:520-572`, Linux only, with `only_on_linux` elsewhere).

## Exploration 3 — corrections and additions found while checking

1. **"The env gate already admits PATH" holds only per host.** `spawn_broker.rs:150-155` says `PATH` is granted by listing it in `allowed_env_keys`, and neither shipped template lists it. `GIT_SSH_COMMAND` is admitted on every host configured for SSH clones, because install documents the clone keys and they are required. The defence-in-depth conclusion stands.
2. **git and many harnesses live outside the install bin dir.**
   - git comes from the supervisor's `PATH`, in practice `/usr/bin/git` (`clone.rs:57-71`).
   - The templates document `opencode` and `claude-agent-acp` under `/usr/local/bin`, and dev's `codex-acp` under `REPO_ROOT/node_modules/.bin`.

   With the default root (the install bin dir alone), a streamed `git clone` and these harnesses are refused. Pass 2 must decide.
3. **The directory trust check accepts sticky world-writable directories** (`config.rs:435-436`). That is right for a policy file's parents, and needs a deliberate decision for exec roots.
4. **The `allowed_session_users` refusal wording overlaps with #859.** `coding_agent_relay.rs:105-106` and the test at :331 also name `allowed_session_users`, which #859 replaces with `users.yaml`. Each node edits its own clause of one message, so expect a rebase conflict there.
5. **#836 is still a contract at `ddef5cc6b`.** `resolve_relayed_env` and `Exactly` have bodies marked `TODO(env-allow-list)`, so this node's env rule builds on #836's green.
