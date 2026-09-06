# 2026-08-13 — Deterministic test suite — deliberate gaps

**Category:** Future enhancement
**Source:** deterministic-test-suite changeset, 2026-08-13

- **`tddy-sandbox-app` keeps `WarmupOptions::default()`** (`src/main.rs`) while the daemon's budget
  moved into `DaemonConfig.agent_warmup`. It has its own config schema, so a daemon-hosted and a
  standalone session on the same host can warm up with different budgets. Give it the same three
  keys, or have it read the daemon's.
- **`pick_free_loopback_port` / `allocate_verified_grpc_listen_port` share a production TOCTOU
  shape** (`sandbox_session.rs`) — bind, note the port, close, hand the *number* to something else,
  which binds it again. **Observed, not theoretical:** the test-side instance of this failed with
  `AddrInUse` on the third of three loaded workspace runs, on the caller's own re-bind, even after
  the search had been moved below the ephemeral range (`spawner.rs`, run 3 of the
  deterministic-suite measurement). Moving the band only removes the *kernel* as a competitor; the
  window between close and re-bind stays open to anything on the host. The test fixture was then
  fixed by never releasing ownership — it returns the held listener. `pick_free_loopback_port` is
  the worse of the two production cases: it binds `127.0.0.1:0`, i.e. draws from the range the
  kernel actively re-issues, then hands the number to a child.
  Two ways out, in order of preference:
  1. **Pass the bound listener across the fork** rather than the number — `FD_CLOEXEC` cleared,
     `LISTEN_PID`/`LISTEN_FDS`/`SD_LISTEN_FDS_START` set. There is in-tree precedent: the
     `handover` field in `packages/tddy-supervisor/src/spawn_broker.rs` already hands the daemon
     its listening socket this way. This closes the window rather than narrowing it.
  2. **Retry on `AddrInUse`** — currently a *fallback* in the CLAUDE.md sense, and not yet
     permissible: the child at `packages/tddy-coder/src/run.rs` does `TcpListener::bind(addr).await?`
     and then `.expect("gRPC server failed")`, so it panics, and the daemon's startup watch cannot
     tell `AddrInUse` from a bad argument, a missing binary, or a real crash. Retrying on that
     signal would mask genuine breakage. It becomes an option only once the child exits with a
     distinguishable status for "the port was taken".
- **The supervisor's unread stderr pipe can deadlock under `RUST_LOG=debug`** — a child that fills
  the pipe buffer blocks on write while nothing is reading.
- **`spawn_startup_poll_interval_ms > spawn_startup_grace_period_ms` is unvalidated.** The `.max(1)`
  clamp makes it harmless (one poll, then the deadline), but a config that says something impossible
  should be refused at load like the rest of `DaemonConfig`.
- **`packages/tddy-daemon/tests/worktree_files_rpc.rs:188` fails `cargo fmt --check`** — pre-existing
  from `5bd24ad1` (#375), left untouched as unrelated. Anyone running `cargo fmt --all` will
  incidentally fix it.
