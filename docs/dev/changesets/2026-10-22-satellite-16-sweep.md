# 2026-10-22 — The #satellite sweep: the files the stack grew over budget are split, and the records it added are closed

**Type:** Refactor

`#satellite` 16/16 ([#695](https://github.com/uppin/tddy-coder/pull/695)), the stack's sweep node, after
nodes 1–15 merged. Scope, as the developer set it: the files the stack grew past or further beyond the
500-production-line budget, the code-issue records and backlog entries the stack itself added, and the
stack-shared files earlier nodes declined to split. Files the stack shrank or left unchanged keep their
records. Behaviour-preserving, apart from the two fixes named below.

Per-package entries: `tddy-daemon`, `tddy-service`, `tddy-session-sync`, `tddy-session-tool-client`,
`tddy-terminal-rpc`, `tddy-telegram-control`, `tddy-tools`, `tddy-livekit`, `tddy-host-service`,
`tddy-session-activity`, `tddy-daemon-auth`, `tddy-cli-sessions` — each
`packages/<pkg>/docs/changesets/2026-10-22-satellite-16-sweep.md`.

## Measurements

Production lines to the first `#[cfg(test)]`, on the merge-base with `master` and on the branch tip.

| File | Before → after | Into | Record |
|---|---|---|---|
| `tddy-daemon/src/runtime.rs` | 1,867 → 296 (`build` 1,080 → 14 lines) | `runtime/{tasks,env,telegram_wiring}.rs`; `build` by hand into `runtime/{build_context,identity,integrations,mount,mount_host,mount_entries,mount_accounts,mount_socket}.rs` | `oversized-file-runtime`, `complexity-runtime-build` deleted |
| `tddy-session-tool-client/src/lib.rs` | 1,114 → 435 | `room_transport.rs` (311), `rpc_transports.rs` (390) | `oversized-file-lib` deleted |
| `tddy-livekit/src/participant.rs` | 907 → 418 | `participant/{publisher,serve}.rs` (170, 348) | `oversized-file-participant` deleted |
| `tddy-tools/src/cli.rs` | 895 → 459 | `cli/{session_tools,changeset_commands,relay}.rs` (174, 78, 231) | `oversized-file-cli` deleted |
| `tddy-session-activity/src/service.rs` | 835 → 202 | `service/{ports,rpc,hook_reports}.rs` (146, 323, 215) | none at the base; `oversized-file-service-rpc` (509) opened and closed inside the change |
| `tddy-session-sync/src/sync.rs` | 788 → 454 | `sync/{git,delta}.rs` (141, 215) | `oversized-file-sync` deleted |
| `tddy-terminal-rpc/src/pty_relay.rs` | 784 → 264 | `pty_relay/{room,daemon_http,grpc_terminal}.rs` (205, 161, 200) | `oversized-file-pty-relay` deleted |
| `tddy-service/build.rs` | 743 → 93 (`main` 682 → 20) | `build/{core_services,unbundled_families,platform_services,descriptors}.rs` by hand | `oversized-file-build` deleted |
| `tddy-telegram-control/…/session_start.rs` | 719 → 295 | `cli_spawns.rs` (444) | `…-session-start` deleted |
| `tddy-host-service/src/host_registry.rs` | 596 → 456 | `host_registry/sightings.rs` (155) | `oversized-file-host-registry` deleted |
| `tddy-telegram-control/…/mod.rs` | 557 → 412 | `types.rs` (151) | `…-mod` deleted |
| `tddy-session-sync/src/attach.rs` | 555 → 371 | `attach/daemon_http.rs` (195) | `oversized-file-attach` deleted |
| `build_auth_entries_admitting` | 111 lines, 6 params → 58, 5 | helpers in `auth/entry_parts.rs` (105) | `complexity-auth-build-auth-entries-admitting` deleted; `oversized-file-auth` narrowed 616 → 569 |

Every new module is under 500. A brace-matching scan of every touched package, on the merge-base and
the tip (functions over 60 lines or 5 parameters), finds no function this change added or lengthened:
`run_livekit_session` (69, then 72 when its widened signature wrapped) is 50 lines, and
`spawn_telegram_claude_cli` (224, then 230 with the exposure fix) is 216. The moved complexity records
(`complexity-cli-run-call-tool`, `complexity-participant-run`, `complexity-service-stream-acp-replay`)
carry their new `Location` and unchanged numbers.

## By hand, with the developer's consent

Each carries a `TODO(restructure-engine)` marker and names the engine gap that forced it.

- **`runtime::build`**: a `BuildContext` of the locals its groups share, then stage methods, each body a
  verbatim slice bound to its old local names. A plain `extract_method` cut would have produced helpers
  with more than five parameters.
- **`tddy-service/build.rs`**: the engine refuses a `build.rs` (no module under `src/`). Four `#[path]`
  modules, byte-identical bodies; `OUT_DIR` identical before and after (`diff -r`).
- **`build_auth_entries_admitting`**: after three `extract_method`s (80 lines), a `let … else`, a
  `provider_auth_entry` helper and the web host and port folded into one value.
- **`cli.rs`'s relay families**: each a `#[cfg(unix)]` / `#[cfg(not(unix))]` pair of same-named
  functions, which the engine cannot anchor; a range `extract_module` over them (lines 738–866) was
  refused because rust-analyzer analyses one `cfg` arm only.
- **`service/rpc.rs`**: a trait impl cannot be split across files and `extract_method` is refused under
  `#[async_trait]`, so the two hook reports' bodies became inherent methods the trait delegates to.

## Fixes

- **`tddy-cli-sessions`**: the terminal-exposure lock no longer spans the join. Each recorded terminal
  carries its own gate; a re-record waits for a reach in flight and a reach writes back only to the record
  it was gated by.
- **`tddy-telegram-control`**: an unwired terminal exposure (`FailedPrecondition`) is not logged as a
  failure.

## Test-side change

`tddy-daemon/tests/unbundle_endpoint.rs` caps the daemon crate's non-blank lines and whitelists its
modules by path. The whitelist names the eleven `runtime/` modules; the cap moves 6,000 → 6,500, because
splitting the composition root adds about 630 non-blank lines of plumbing (5,707 → about 6,340 on top of
`#provider-route` 1/9). The whitelist is what keeps session logic out of the crate, and it is exact.

## Backlog

- **Deleted:** `2026-10-19-satellite-7-client-transports-left-files-over-the-500-line-budget` (all three
  rows and the `sync.rs` note closed), `2026-10-19-terminal-exposure-lock-spans-the-join` (fixed).
- **Narrowed:** `2026-10-17-satellite-1-left-files-over-the-500-production-line-budget` (to the three
  files the stack net-shrank; its Telegram `warn!` paragraph is fixed),
  `2026-09-12-the-two-new-service-rs-files-are-over-budget` (to `tddy-session-agents`).
- **Added:** `2026-10-22-restructure-cannot-anchor-cfg-twin-functions` (engine gap).
- **Extended:** `2026-10-21-restructure-cannot-reparent-modules-declared-in-a-binary-root` (the `build.rs`
  case), `2026-10-21-tddy-daemon-binary-cannot-be-built-without-the-livekit-plugin` (assessed; the feature
  needs a `not(feature)` arm in seven of the runtime stages and edits files #694 owns).
- **Deferred with the user's consent** (2026-10-22, "defer the remaining"; a consent line on each entry,
  files kept): the `tddy-daemon` `livekit` feature
  (`2026-10-21-tddy-daemon-binary-cannot-be-built-without-the-livekit-plugin`); the engine gaps
  `2026-10-19-restructure-extract-method-refused-under-async-trait`,
  `2026-10-21-restructure-cannot-reparent-modules-declared-in-a-binary-root`,
  `2026-10-21-restructure-cannot-move-a-binary-root-or-a-test-crate` and
  `2026-10-22-restructure-cannot-anchor-cfg-twin-functions`; the three LiveKit testkit flake entries
  from #651 (`2026-10-18-livekit-docker-testkit-can-stall-the-event-loop-with-no-deadline`,
  `2026-10-18-livekit-web-start-echo-server-intermittently-exceeds-its-task-timeout`,
  `2026-10-18-rust-tests-that-flaked-once-in-four-ci-attempts-of-651`); and
  `2026-10-21-session-delta-stores-adapter-lives-in-the-daemon`, which #694 closes.

## Verification

Scoped to the touched packages (`./test -p <pkg>`, one package at a time); whole-workspace health is CI's.

| Package | Baseline (passed / failed / ignored) | Tip |
|---|---|---|
| `tddy-session-activity` | 49 / 0 / 0 | 49 / 0 / 0 |
| `tddy-daemon-auth` | 159 / 0 / 0 | 159 / 0 / 0 |
| `tddy-host-service` | 199 / 0 / 0 | 199 / 0 / 0 |
| `tddy-livekit` | 97 / 0 / 2 (see below) | 82 / 0 / 1 |
| `tddy-terminal-rpc` | 69 / 0 / 0 | 69 / 0 / 0 |
| `tddy-session-tool-client` | 16 / 0 / 0 | 16 / 0 / 0 |
| `tddy-session-sync` | 70 / 0 / 0 | 70 / 0 / 0 |
| `tddy-telegram-control` | 158 / 0 / 0 | 160 / 0 / 0 (+2 exposure-refusal unit tests) |
| `tddy-service` | 157 / 0 / 0 | 157 / 0 / 0 |
| `tddy-tools` | 266 / 0 / 0 | 266 / 0 / 0 |
| `tddy-cli-sessions` | 15 / 0 / 0 | 17 / 0 / 0 (+2 exposure-gate tests, each red before its fix) |
| `tddy-daemon` | 284 / 0 / 1 (on `bcdbb08af`) | 285 / 2 / 1 in the full run, on top of `#provider-route` 1/9; both failures pass alone: `session_agent_remote_acceptance::restores_a_clone_that_diverged_and_says_so` (17 s; the baseline's known noise) and `session_attach_cross_host_acceptance::a_forwarded_rpc_to_a_peer_that_stopped_answering_fails_within_its_deadline` (Docker port-binding 500 at load average > 200; 12 s) |

`cargo clippy -p <the twelve> --all-targets -- -D warnings` and `cargo fmt --check` on the same packages: clean.

`tddy-livekit`'s planning baseline recorded 97 passed and 2 ignored, which does not reproduce and is not
a loss: `cargo test -p tddy-livekit -- --list` names the same 83 tests (82 run, 1 ignored) on the
merge-base and on the tip, compared by name. The extra 15 were counted from another package's binaries.
