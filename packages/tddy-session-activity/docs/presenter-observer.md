# Presenter observation and notification publishing (`tddy_session_activity`)

The per-session presenter observer: one gRPC stream from a child `tddy-coder` feeding the notification bus
and, through the kernel's `OperatorChannel` port, whichever chat surface the daemon installed. About 3.0k
production lines in the crate.

| Module | Holds |
|---|---|
| `presenter_observer_task.rs` | the gRPC client that subscribes to a child `tddy-coder`'s `PresenterObserver` and hands each event to the daemon's `OperatorChannel`, then to the session-notification bus; spawned only when the channel `follows_presenter_events()` or a bus is present |
| `presenter_observer_spawn.rs` | `PresenterObserverDeps` (`tddy_data_dir`, `operator_channel` — a `SharedOperatorChannel` — and `session_notification_bus`, the last two shared) and `maybe_spawn_presenter_observer` |
| `presenter_intent_client.rs` | the gRPC client to a child `tddy-coder`'s `PresenterIntentClient` (submit feature text, answer clarifications) |
| `session_notification_publishing.rs` | `SessionNotificationPublishing` and `resolve_session_label`: the publish context built on a session's display label, which is read through `session_list_enrichment` |
| `remote_git_pack_execution.rs` | the `PackExecution` for `RemoteGitService.Serve`, resolved from the session metadata |

The daemon host builds the deps (`presenter_observer_deps()` in `tddy-session-lifecycle`); the launch
topic holds a copy. `session_notifications` stays a facade there over this crate's module and
`session_notification_publishing`.
