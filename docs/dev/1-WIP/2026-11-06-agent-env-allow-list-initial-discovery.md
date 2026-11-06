# Initial discovery — agent-env-allow-list

Exploration 1 is the whole-work discovery for the #agent-tools leftovers stack (21–28/28), copied in full. This node's items are named in its changeset's Prerequisites and State A.


**Base:** `yolo-pr-stack-agent-tools-6465b5be` (PR #828, `#agent-tools 20`) at `e6ff7012e`.
n1–n19 (#790–#825) are merged to master; #828 is the YAML-only sweep. This stack's
three nodes stack on #828 and close the stack TODOs #828 left open.

Not staged on any branch. Copied into each node's companion as Exploration 1.

## (whole work) Conclusions

_(filled once the explorations return)_

## (whole work) Decomposition (user-approved 2026-11-06)

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

## (whole work) Exploration 1 — bounded waits (Explore, sonnet · medium, read-only, no execution)

1. **ACP launch waits** (`2026-11-04-acp-launch-waits-have-no-deadline.md`): HALF STALE.
   - open_session FIXED: `SESSION_OPEN_DEADLINE` 120 s, `acp_driver.rs:227`, `open_session_within` :420, test `tests/spec_backend_integration.rs:24,95` (`that_never_answers`). Record needs narrowing only.
   - Supervisor STILL TRUE: `tddy-supervisor/src/client.rs:291-292` `call()` → `transport.call_unary(...)` unbounded; `spawn_relayed` (:150) reaches it. NOT in record: `clone_repository` (:198-246) `call_server_stream(...).await` + first `frames.recv().await` unbounded.
   - Attach: `SupervisorClient` (connect :58) has no config; `SupervisorError::Unavailable { message }` (error.rs) is the named error; crate style is module `const X: Duration` (clone.rs:35 `STDERR_DRAIN_AFTER_EXIT`). Daemon side already wraps some calls with `spawn_worker_request_timeout()` (daemon-kernel config/daemon_methods.rs:138; session-split service_util.rs:195,227).
   - Seam: `tddy-supervisor/tests/support/mod.rs`, pattern `a_git_that_never_finishes` (supervisor_clone_progress.rs:184); silent peer = raw UnixListener that accepts and never replies (transport is concrete `Arc<StdioRpcClient>`, no fake). Size S (M with clone stream).
2. **Generic session start** (`2026-11-06-generic-session-start-has-no-deadline-of-its-own.md`): STILL TRUE. `operator_start.rs:54` passes `CancellationToken::new()` into `start_here` (svc_start_session_core.rs:144); `start_session_core` same. Only outer bound is Telegram `START_DEADLINE` 600 s (cli_spawns.rs:12,93). Attach: wrap `start_here` body, cancel token on expiry; knob = new `DaemonConfig` method next to `spawn_worker_request_timeout` (clamp 0→1 s; test config.rs:541). Seam: agent-launch start tests (`host_session_socket_tests.rs`, `LaunchSessions` fakes) — a never-returning start step fake must be built. Packages: tddy-agent-launch, tddy-daemon-kernel (opt. telegram-control). Size M.
3. **clone_askpass host-key**: NOT CONFIRMED from code. Test `a_host_key_confirmation_request_is_refused_and_the_clone_fails_in_seconds` (agent-launch tests/clone_askpass_acceptance.rs:172), real sshd fixture (tests/support/sshd.rs), `StrictHostKeyChecking=ask`; guard `A_CLONE_THAT_HANGS` 60 s (tests/support/rig.rs:25,101). Planner sets `BatchMode=yes` only without askpass (clone_planner.rs:305,361); askpass branch :345 (`SSH_ASKPASS_REQUIRE=force`). Needs a manual run before planning.
4. Other todo records: only the ssh-session-pointer record mentions a deadline (belongs to the env/sockets node).

Overlap: item 2 edits `svc_start_session_core.rs`, which also holds the `coding_agents` model check — keep it a thin wrapper. Supervisor item isolated.

## (whole work) Exploration 2 — env and session sockets (Explore, sonnet · medium, read-only, no execution)

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

## (whole work) Exploration 3 — registry plumbing (Explore, sonnet · medium, read-only, no execution)

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
