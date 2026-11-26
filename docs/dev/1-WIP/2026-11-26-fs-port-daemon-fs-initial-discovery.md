# Initial discovery — `#fs-port` n3 (daemon-fs)

## Exploration 1 (whole-work dump, copied)

**Date:** 2026-11-26 · **Streak:** `#fs-port` (13 nodes) · **Lands after:** `#target-reconnect` (#1041, #1042; #1040 merged)

## Combined conclusions

- **No filesystem port exists.** No `trait Fs`/`FileSystem` anywhere in `packages/*/src`. Disk access is
  direct `std::fs` / `tokio::fs`: ~2,000 matching lines in ~85 crates (count of `fs::` / `tokio::fs` lines
  in `src/`, tests included). Heaviest: code-restructuring 180, agent-launch 177, session-lifecycle 140,
  zed-extensions 117, session-files 116, supervisor 106, session-activity 100, sandbox 73, vm 71,
  daemon-kernel 70, workflow-recipes 66, changeset 62, host-service 58, tddy-coder 57.
- **Four atomic-write implementations:** `tddy-session-store/src/atomic_file.rs` (`write_atomic`,
  `write_atomic_with_mode`, `write_atomic_labelled`; re-exported by `tddy-core`, used by ~20 crates),
  `tddy-credentials/src/atomic.rs` (`write_owner_only`, a copy), `tddy-agent-launch/src/log_follower.rs:176`
  (`write_atomically`), `tddy-task/src/capture_recording.rs:74` (`write_atomically`). The backlog entry
  [2026-09-23-atomic-file-leaf-crate.md](../todo/2026-09-23-atomic-file-leaf-crate.md) asks for a std-only
  leaf crate — the FS port crate is that crate.
- **Decision (operator, 2026-11-26):** new leaf crate **`tddy-fs`** — sync `Fs` trait (`Arc<dyn Fs>`),
  `DiskFs` + `MemFs` adapters, atomic writes as port operations, an `AsyncFs` facade behind a `tokio`
  feature for `tokio::fs` sites. `tddy-session-store::atomic_file` re-exports at its old path.
- **Daemon HTTP:** `tddy-daemon/src/server.rs::run_server` → `serve_web_bundle_with_shutdown(host, port, …)`
  on `listen.web_host`/`listen.web_port` (`tddy-daemon-kernel/src/config.rs::ListenConfig`, `deny_unknown_fields`).
  `tddy-daemon` is **wiring only** (`packages/tddy-daemon/CLAUDE.md`) → registry, collectors and the
  metrics listener live in a new crate **`tddy-metrics`**; the daemon only wires it.
- **No metrics crate in Cargo.lock.** Operator approved **`prometheus-client`** (new external dep).
  Config: `metrics: { listen: "127.0.0.1:9464" }` (absent ⇒ no listener; equal to web port ⇒ startup error).
  Initial metrics: `tddy_build_info`, `process_open_fds`, `tddy_sessions_active{kind}`,
  `tddy_session_starts_total{result}`, `tddy_rpc_requests_total{service,method,code}`,
  `tddy_rpc_request_duration_seconds`, `tddy_livekit_rooms_connected`.
- **Sequencing:** n1 and n3–n13 edit crates `#target-reconnect` 2/3 and 3/3 (#1041 agent-launch,
  cli-sessions; #1042 session-lifecycle) touch. **No node starts `/green` before #1041 and #1042 merge.**
- **Restructure engine:** the migration is call-site rewriting (not a move/extract), which the engine does
  not perform; the one mechanical move (atomic_file → tddy-fs) should be tried with the engine first per
  `code-restructuring`, and per memory "engine moves only", a refusal needs operator consent.
- **Test-only crates keep real disk:** tddy-testing-commons, tddy-e2e, tddy-vm-testkit, tddy-test-hosts.

## Deferred-work cross-check (Step 2b)

| Entry | Verdict | Node |
|---|---|---|
| [2026-09-23-atomic-file-leaf-crate.md](../todo/2026-09-23-atomic-file-leaf-crate.md) | answered — ✅ resolved by n1 | n1 |
| [2026-10-08-session-lifecycle-test-util-is-not-gated-behind-a-test-util-feature.md](../todo/2026-10-08-session-lifecycle-test-util-is-not-gated-behind-a-test-util-feature.md) | during — MemFs test helpers must not widen it | n4 (reference) |
| [2026-11-02-hermetic-build-action-outputs-are-executable.md](../todo/2026-11-02-hermetic-build-action-outputs-are-executable.md) | unrelated (mode semantics only) | — |

## Exploration 1

Commands: per-crate `grep -rcE "(std::)?fs::|tokio::fs" packages/*/src`; `grep "fn write_atomic|atomic_write"`;
`grep trait (Fs|FileSystem)` (none); `Cargo.lock` grep for prometheus/metrics (none); read
`tddy-session-store/src/atomic_file.rs`, `tddy-daemon/src/server.rs`, `tddy-daemon-kernel/src/config.rs::ListenConfig`,
`packages/tddy-daemon/CLAUDE.md`; `gh pr list` for #1040–#1042 (1040 MERGED, 1041/1042 OPEN).


## Exploration 2 — node n3

Crates and fs line counts: tddy-daemon-kernel 70, tddy-daemon-auth 34, tddy-daemon-sandbox 25, tddy-daemon-rpc 12, tddy-daemon 4, tddy-daemon-livekit 2.
