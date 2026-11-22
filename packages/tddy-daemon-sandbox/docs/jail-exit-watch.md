# Main-jail exit watch

`jail_exit_watch.rs` watches a session's **main** jail for death. Tab jails keep their own poll
(`tab_jails.rs`).

| Item | Role |
|---|---|
| `watch_jail_exit(state)` | Waits for the runner with `waitid(P_PID, WEXITED \| WNOWAIT)` on a blocking task — no lock held, the state held weakly so a dropped state ends the wait. `Some(exit)` when the runner exited on its own, `None` when it was stopped by request, before or during the wait. |
| `SandboxSessionManager::insert` | Starts the watch. A death removes the jail when it is still the one registered (and finishes its recording) and broadcasts a `JailEnded { session_id, exit }` on `subscribe_ended`. |
| `JailProcess` | The runner behind `SandboxSessionState`: `Owned(Child)` today. `try_exit` reads with `WNOHANG \| WNOWAIT` so `is_running` / `runner_pid` never reap the status away; `kill_and_reap` ends it. A supervisor-spawned jail (jail-uid) is a later variant. |
| `SandboxSessionState::{exit, subscribe_exit}` | How the runner ended, once known — on its own or stopped. The terminal's `pty_done` follows it. |
| `JailBridge` / `with_bridge` | The stdio endpoint, relay and terminal-forwarder tasks `dial_and_bridge*` return; the state aborts them on stop. |

**Stop versus death.** `stop` sets `stopped` before it kills, and the watch checks it before recording, so a stop is
never announced as a death. The first exit recorded stands (`record_exit`).

`sandbox_session.rs` keeps the session state and registry; its former neighbours are `jail_host_bridge.rs` (the
host side of the stdio bridge, `dial_and_bridge*`), `runner_paths.rs` (binary and read-path discovery) and
`runner_spawn.rs` (`SandboxRunnerSpawn`, `build_sandbox_plan`, `spawn_sandbox_*`), all re-exported from
`sandbox_session`.

Feature: [session-liveness](../../../docs/ft/daemon/session-liveness.md#a-dead-main-jail).
