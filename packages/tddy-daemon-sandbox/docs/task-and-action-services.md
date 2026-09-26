# The tasks and actions services (tddy-daemon-sandbox)

`tddy-daemon-sandbox` is the daemon's sandbox orchestration: starting a jailed session
(`sandbox_session`), provisioning the workspace tool sandbox (`workspace_tool_sandbox`), and the
plans and runtime behind both (`sandbox_plan_builder`, `sandbox_runtime`, `sandbox_action`). The
crate's own rustdoc (`src/lib.rs`) records why it is a crate apart from `tddy-spawn`. This page covers
the two RPC services it also serves, because they run tasks through that runtime.

## `task_service` and `action_service`

| Module | Serves | Over |
|---|---|---|
| `task_service` | `tasks.TaskService` (`TaskServiceImpl`): list, get, watch, cancel and send input to the tasks in the daemon's `TaskRegistry` | the shared `tddy_task::TaskRegistry` |
| `action_service` | `actions.ActionService` (`ActionServiceImpl`): list the action kinds, start one, get its task | `tddy_actions`' catalog and runtimes, attaching a sandbox request through `sandbox_runtime` when the action asks for one |

Both authenticate every call by resolving its session token with the daemon's
`SessionUserResolver`; an unknown or expired token is `UNAUTHENTICATED`.

The registry they share is created by `tddy-session-lifecycle`'s `CliSessionManager`, which is why the
services take it as a constructor argument rather than owning it. `tddy-daemon`'s `runtime.rs` builds
both, and names them `tddy_session_lifecycle::task_service::…` and `…::action_service::…`: lifecycle
re-exports this crate (`pub use tddy_daemon_sandbox::*;`), so those paths resolve here.

Neither module needed a dependency the crate did not already have: `tddy-task` and `tddy-actions`
could not host them without a cycle, and `tddy-daemon-rpc` sits above lifecycle.

## Tests

| Suite | What it pins |
|---|---|
| `tests/action_service_acceptance.rs` | a started bash action is visible as a task; the kinds list carries bash with its input schema |
| `tests/action_sandbox_acceptance.rs` | sandboxed actions: writing to the output directory, refusing one without it, `tddy-coder` and a build action under a read-only mount, a write outside egress denied, PTY output streamed, and the refusal off Darwin and Linux |
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
