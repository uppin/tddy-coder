# The tasks and actions services (tddy-daemon-sandbox)

`tddy-daemon-sandbox` is the daemon's sandbox orchestration: starting a jailed session
(`sandbox_session`), provisioning the workspace tool sandbox (`workspace_tool_sandbox`), and the
plans and runtime behind both (`sandbox_plan_builder`, `sandbox_runtime`, `sandbox_action`). The
crate's own rustdoc (`src/lib.rs`) records why it is a crate apart from `tddy-spawn`. This page covers
the two RPC services it also serves, because they run tasks through that runtime.

## Recording a jailed `main`

`main_recording.rs` is the jailed counterpart of `tddy-cli-sessions`' recorder.
`SandboxSessionManager::record_main_capture(session_id, path)` registers the session's sandbox state's
capture (no state: logged, nothing registered); a `JailRecorder` rewrites it every
`tddy_task::RECORDING_INTERVAL` (`with_recording_interval` overrides) and writes it once more inside
`SandboxSessionManager::remove` — delete, or a resume replacing the state — before `remove` returns. A
jail that dies on its own is covered by the periodic write. Tests: `tests/jail_main_recording.rs`.

## `task_service` and `action_service`

| Module | Serves | Over |
|---|---|---|
| `task_service` | `tasks.TaskService` (`TaskServiceImpl`): list, get, watch, cancel and send input to the tasks in the daemon's `TaskRegistry` | the shared `tddy_task::TaskRegistry` |
| `action_service` | `actions.ActionService` (`ActionServiceImpl`): list the action kinds, start one, get its task | `tddy_actions`' catalog and runtimes, attaching a sandbox request through `sandbox_runtime` when the action asks for one |

Both authenticate every call by resolving its session token with the daemon's
`SessionUserResolver`; an unknown or expired token is `UNAUTHENTICATED`.

`TaskServiceImpl::forwarding_to_peers(room, local_instance_id)` makes `CancelTask` forward a request whose
`daemon_instance_id` names another daemon to that daemon over the `CommonRoom`
(`tddy_daemon_kernel::peer_calls::forward_cancel_task`) and return its answer; a request naming this
daemon, or none, cancels in the local registry. `tddy-daemon`'s `mount_entries.rs` calls it when the
daemon has a common room. Without it, every request is served locally.

The registry they share is created by `tddy-session-lifecycle`'s `CliSessionManager`, which is why the
services take it as a constructor argument rather than owning it. `tddy-daemon`'s `runtime.rs` builds
both, and names them `tddy_session_lifecycle::task_service::…` and `…::action_service::…`: lifecycle
re-exports this crate (`pub use tddy_daemon_sandbox::*;`), so those paths resolve here.

Neither module needed a dependency the crate did not already have: `tddy-task` and `tddy-actions`
could not host them without a cycle, and `tddy-daemon-rpc` sits above lifecycle.

## `build_action_sandbox`

`DaemonActionSandbox::new(work_dir)` implements `tddy_build::ActionSandbox` for `sandbox: true` /
`--hermetic` build actions ([tddy-build § Hermetic actions](../../../docs/ft/build/tddy-build.md#hermetic-actions)).
It lowers one action into a process-mode jail plan, one directory per action id under `work_dir`:
inputs are read-only grants, declared outputs the only writable paths, the tool closure is lowered
by `tddy_sandbox_recipes::lower_tools` (literal executable reads, linked from a jail-local `bin/`
first on `PATH`), and network is `lower_network` of the closure. A closure that declares egress hosts
is refused — there is no egress shim for build actions. It spawns through `spawn_sandbox_plan` (Seatbelt
on macOS, cgroups on Linux); the action's stdout/stderr are captured by a launcher shell into files in
its jail directory, because the backends' own logs are not the command's output.

Limits: on Linux writes and execs are not confined
([todo](../../../docs/dev/todo/2026-11-02-hermetic-build-actions-unconfined-on-linux.md)), so the two
confinement tests in `tests/hermetic_build_action_acceptance.rs` are macOS-only
(`FIXME(fs-confinement)`); outputs are executable
([todo](../../../docs/dev/todo/2026-11-02-hermetic-build-action-outputs-are-executable.md)); no egress
shim ([todo](../../../docs/dev/todo/2026-11-02-hermetic-build-actions-have-no-egress-shim.md)).

## `tool_jail`

The jail of a tool run from its definition — the sandboxed `claude-cli` session's, through
`tddy_agent_launch`'s `LaunchSessions::launch_jailed`. Nothing here names a tool.

- `lower_tool_jail(&ToolJailInputs { tool, closure, defs, tddy_home, host_home, scratch_dir, host, route })
  -> Result<ToolJailGrants, Status>` lowers the tool's `home:` (`lower_home`; both a legacy and a
  `homes/<key>` home existing is `FAILED_PRECONDITION`), every closure member's `files:`, `secrets:`
  (dropped by their route guard against `route`) and `policy:` (with Mach lookup of all services and a
  pseudo-terminal added), `lower_tools` of the closure into `<scratch>/bin`, the binary's own
  `process_exec_reads`, and `lower_network` of the closure. It is not pure: a persistent home is
  created or migrated and `seed-once` files are copied before the jail exists.
- `ToolJailGrants { home, env, bin_dir, mounts, reads, symlinks, secrets, policy, egress, oauth_inbound }`:
  `env` is the definition's overlays plus `HOME`; `oauth_inbound` is the declared one and only on an
  unrouted session.
- **The route's hosts.** `route_hosts(&HarnessRoute)` is the `host:port` of every `http(s)` URL in the
  route's env — found **inside** each value, so an inline JSON configuration's base URL counts
  (OpenCode's `OPENCODE_CONFIG_CONTENT`, `#agent-tools 29/32`) — and in its files, the
  scheme's port when none is named. They join a `hosts` policy, become the whole
  policy of a `none` one, and leave `any` as it is.
- `build_tool_jail_plan(SandboxRunnerSpawn, ToolJailGrants)` is `build_sandbox_plan` of the spawn
  (whose `mounts` already hold the grants') with the grants' reads and links added, and their policy,
  egress and OAuth inbound replacing what the runner's argv would imply; each secret goes on with
  `SandboxPlan::add_secret`.

`sandbox_session::dial_and_bridge_with_egress` is `dial_and_bridge` with the jail's `EgressGuard` on
the host relay (`HostRelayConfig::with_egress`), so a `TunnelOpen` to an undeclared host is refused on
the host as well as at the in-jail shim. `dial_and_bridge` is it with an unrestricted guard, for the
recipe-built jails.

A jailed agent's persistent home is its tool definition's `home:` section, resolved by
`tool_jail::lower_tool_jail`. `sandbox_session` seeds no agent home, and `build_allow_read_paths`
grants the toolchain paths plus the runner binary's own directory. What the process the runner execs
reads comes from its tool definition, never from the runner's argv.

## Tests

| Suite | What it pins |
|---|---|
| `tests/action_service_acceptance.rs` | a started bash action is visible as a task; the kinds list carries bash with its input schema |
| `tests/action_sandbox_acceptance.rs` | sandboxed actions: writing to the output directory, refusing one without it, `tddy-coder` and a build action under a read-only mount, a write outside egress denied, PTY output streamed, and the refusal off Darwin and Linux |
| `tool_jail::tests` | a route's base-URL host with its scheme's port; route hosts joining declared hosts and opening a `none` jail to themselves alone; no route keeps the declared policy |
| `tests/task_service_peer_cancel.rs` | `CancelTask` naming a peer daemon is forwarded to it (over a scripted `CommonRoom` peer) and its answer returned; one naming this daemon is not forwarded |
| `packages/tddy-session-lifecycle/tests/task_service_acceptance.rs` | `tasks.TaskService`. It stays in lifecycle because three of its tests drive lifecycle's `ClaudeCliSessionManager`, and moving it would make this crate dev-depend on lifecycle |

On a macOS developer host:

- `action_sandbox_acceptance::sandboxed_bash_pty_action_streams_output` does not finish, so a local
  run skips it (`--skip sandboxed_bash_pty_action_streams_output`).
- `tests/sandbox_stdio_seatbelt_acceptance.rs` (`#![cfg(target_os = "macos")]`) does not compile:
  three `E0425`, `SandboxHandle` not imported. Linux CI never builds it, so a macOS run excludes that
  binary.
- `sandbox_session_stdio_acceptance::real_daemon_session_drives_a_seatbelt_jailed_sandbox_runner_entirely_over_stdio`
  fails with `tool dispatch timed out`.

## Related

- [code-issues/](./code-issues/) — the open findings against `sandbox_session.rs` and
  `workspace_tool_sandbox.rs`
- [changesets/](./changesets/) — change history, one file per change
- [`tddy-session-lifecycle` module layout](../../tddy-session-lifecycle/docs/module-layout.md) — the
  facades over this crate's modules
