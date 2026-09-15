# 2026-09-15 — 122 of `tddy-daemon`'s 139 test suites belong to other crates

**Category:** Architecture / test placement
**Source:** test-placement audit during `#carve` planning

`tddy-daemon` is 58,693 lines, of which **2,377 are production code**. Its `src/` is twelve files —
`runtime.rs` (1,432), `daemon_settings.rs` (642), `local_socket_server.rs`, `daemon_config_service.rs`,
`main.rs`, `server.rs`, `lib.rs`, `startup.rs`, and four one-line re-export shims.

The other **55,682 lines are 139 integration-test binaries**, and most of them do not test this crate.

## What was measured

`tddy-daemon/src/lib.rs` is a 30-line facade re-exporting **82 modules** from `tddy-session-lifecycle`,
under the comment *"Legacy paths for integration suites (`tddy_daemon::connection_service`, …)"*.
`tddy-session-lifecycle` in turn re-exports **49** of those from ten further crates. So a test's
import says nothing about what it exercises: `tddy_daemon::host_registry` is `tddy-host-service`'s,
and `tddy_daemon::session_room` is `tddy-daemon-livekit`'s.

Resolving every `tddy_daemon::<module>` through both hops, and separating the daemon's own modules
from the `config` / user-path boilerplate a test uses only to stand a fixture up:

| | Suites | Lines |
|---|---:|---:|
| Exercise a **real** `tddy-daemon` module (`runtime`, `server`, `startup`, `daemon_settings`, `daemon_config_service`, `local_socket_server`, `relay_idle`) | **17** | 8,843 |
| Touch only `config` / `tddy_user_config` / `user_sessions_path` boilerplate | 60 | 27,095 |
| Touch **no** `tddy-daemon` module at all | 62 | 19,744 |

**122 suites — 46,839 lines, 84% of the test code — never reach this crate's production code.**

Counted independently: **0 of 139** test files name `tddy_session_lifecycle`, while **133** name
`tddy_daemon::`. The suites did not move when the code did; the facade is what has kept them
compiling through ten `#unbundle` nodes.

## Where they belong

| Destination crate | Suites | Lines |
|---|---:|---:|
| `tddy-session-lifecycle` | 97 | 38,629 |
| `tddy-daemon-livekit` | 6 | 2,318 |
| `tddy-session-files` | 2 | 1,376 |
| `tddy-rpc` | 2 | 902 |
| `tddy-worktree-service` | 4 | 860 |
| `tddy-daemon-auth` | 2 | 804 |
| `tddy-projects` | 3 | 610 |
| `tddy-session-agents` | 1 | 515 |
| `tddy-tool-engine` | 1 | 334 |
| `tddy-daemon-kernel` | 1 | 234 |
| `tddy-sandbox-runner` | 1 | 134 |
| `tddy-core` | 1 | 90 |
| `tddy-service` | 1 | 33 |
| **total** | **122** | **46,839** |

The largest single misplacements:

```
1835  session_room_acceptance.rs                  -> tddy-session-lifecycle
1495  telegram_session_control_integration.rs     -> tddy-session-lifecycle  (tddy-telegram-control after #carve 7/9)
 970  task_service_acceptance.rs                  -> tddy-session-lifecycle
 930  claude_cli_session_acceptance.rs            -> tddy-session-lifecycle
 766  session_activity_delta_acceptance.rs        -> tddy-daemon-livekit
 715  remote_git_livekit_acceptance.rs            -> tddy-daemon-auth
 707  context_sync_acceptance.rs                  -> tddy-session-files
 511  vm_service_acceptance.rs                    -> tddy-rpc
```

**`tddy-session-lifecycle` has no `tests/` directory at all.** Its 38,629 lines of acceptance
coverage live in another crate, which is why it looks untested and why it cannot be verified on its
own.

## What this costs today

- **17 of `tddy-daemon`'s `tddy-*` runtime dependencies are named by no file in its `src/`** —
  `tddy-workflow-recipes`, `tddy-livekit`, `tddy-telegram`, `tddy-session-files`,
  `tddy-session-activity`, `tddy-session-agents`, `tddy-daemon-sandbox`, `tddy-sandbox`,
  `tddy-sandbox-recipes`, `tddy-sandbox-runner`, `tddy-pty`, `tddy-task`, `tddy-stdio`,
  `tddy-semantic-index`, `tddy-session-sync`, `tddy-demo-runner`, `tddy-workflow`. They are declared
  in `[dependencies]`, not `[dev-dependencies]`, so **every consumer of `tddy-daemon` rebuilds them**.
- **139 of the workspace's 597 test binaries (23%)** each link a ~55-crate dependency graph to
  exercise a 2,377-line library.
- A scoped `./test -p tddy-session-lifecycle` proves almost nothing, because that crate's tests are
  not in it.

## What closing it would take

1. Rewrite `tddy_daemon::X` → the owning crate's path in the 122 suites. Mechanical: the
   module→crate map is derivable from the two `lib.rs` facades, and the audit above already resolved
   it per file.
2. `git mv` each suite into its destination crate's `tests/`.
3. **Delete the `lib.rs` facade.** Its only stated purpose is those suites.
4. Reclassify the now-unreferenced `[dependencies]` as `[dev-dependencies]`, or drop them.

Steps 1–3 are close to mechanical and should be one change; step 4 follows from it and is what
delivers the compile-time win.

**Do it after the `#carve` stack lands.** `#carve` 7/9 moves the Telegram cluster to
`tddy-telegram-control`, which retargets the 12 Telegram suites (4,901 lines) a second time — and
those suites reaching the cluster **unedited** through the facade is that node's AC6. Moving them
first would invalidate it.

## Not a defect, and deliberately not proposed

The 17 suites that mount `runtime` to stand a whole daemon up for a cross-host acceptance test
(`session_agent_remote_acceptance.rs`, `remote_managed_worktree_cross_host_acceptance.rs`,
`split_session_resume_acceptance.rs`, …) are **correctly placed**. The daemon is the composition
root; a test of the composition belongs with it. This entry is about the other 122.
