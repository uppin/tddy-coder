# Which test binaries live here

`tddy-daemon` is the daemon's **composition root**: `runtime.rs` wires roughly twenty services
together and `main.rs` starts them. `tests/` holds **22 suites** — the ones that exercise that
composition, plus `test_placement.rs`, which asserts the rule below.

Every other suite lives in the crate whose code it exercises.

## The rule

A suite belongs here when **either** holds:

1. **It names a module this crate defines** — `runtime`, `server`, `startup`, `config`,
   `daemon_settings`, `daemon_config_service`, `local_socket_server`, `index_daemon`. A suite that
   reaches a module this crate merely re-exported does not belong here, and an import alone proves
   nothing about which of the two it is.
2. **It asserts about this package's own `src/` or `Cargo.toml`.** This kind cannot move at all:
   `CARGO_MANIFEST_DIR` would name whichever crate it landed in, so the assertion would go on
   passing while silently being about something else. `tddy-workflow-recipes`'
   `proto_workflow_contracts.rs` is the same shape and stays put for the same reason.

`test_placement.rs` enforces (1) against a named list, and also asserts that `src/lib.rs` carries no
re-export facade and that this crate declares no `tddy-*` crate in `[dependencies]` that no file in
`src/` names. Adding a suite here means adding it to that list, with a sentence saying which of the
two clauses it satisfies.

## Why the rule needs stating at all

`src/lib.rs` once carried a facade re-exporting 82 `tddy-session-lifecycle` modules under the
comment *"Legacy paths for integration suites"*, and `tddy-session-lifecycle` re-exported 49 of
those from ten further crates. A test's import therefore said nothing about what it exercised:
`tddy_daemon::host_registry` is `tddy-host-service`'s, `tddy_daemon::session_room` is
`tddy-daemon-livekit`'s. Under that facade 119 of 140 suites sat here without reaching this crate's
production code, and they carried 17 `tddy-*` crates into `[dependencies]` — not
`[dev-dependencies]` — so every consumer of the daemon rebuilt them.

`lib.rs` now declares only what this crate defines, and there are no re-export shims. `src/config.rs`
is the one forwarding module that stays: it forwards to `tddy-daemon-kernel`, which owns the
configuration this crate loads.

## Where the rest went

| Destination | Suites |
|---|---:|
| [`tddy-session-lifecycle`](../../tddy-session-lifecycle/docs/test-suites.md) | 95 |
| `tddy-worktree-service` | 7 |
| `tddy-daemon-livekit` | 6 |
| `tddy-projects`, `tddy-tool-engine`, `tddy-vm` | 2 each |
| `tddy-core`, `tddy-daemon-auth`, `tddy-daemon-kernel`, `tddy-daemon-sandbox`, `tddy-session-agents` | 1 each |

They were relocated with `move_test_binary_to_crate` — see
[test-binary-moves.md](../../tddy-code-restructuring/docs/test-binary-moves.md).
