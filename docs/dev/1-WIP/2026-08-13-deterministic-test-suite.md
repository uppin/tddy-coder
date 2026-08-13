# WIP changeset — deterministic, host-independent test suite

**Status:** in progress (2026-08-13). Wrap into `docs/dev/changesets.md` + the per-package
`changesets.md` files when verification completes.

## Problem

Two back-to-back `cargo test --workspace` runs on the same machine produced **different failure
sets** — 12 failures in one, 15 in the other, only 6 in common. The suite reported how busy the
machine was, not whether the code worked. Five causes:

1. Fixtures comparing a raw `TempDir` path against a path production canonicalized (macOS
   `/tmp` → `/private/tmp`).
2. A fixture naming a session id it never wrote to disk.
3. Time budgets calibrated on an idle machine — and in two cases no wait at all — while cargo runs
   many test binaries concurrently (`--test-threads=1` is per binary, not per run).
4. Readiness checks watching the wrong signal: a Unix socket *inode* (which outlives the process
   that bound it) instead of a connect; and no readiness signal at all for a spawned stdio-RPC
   fixture, so its 900 ms call budget silently also covered fork/exec, dynamic linking and tokio
   start-up.
5. A hard dependency on a service that is not part of the repo — two tests needed an SGLang server
   on `localhost:30000` and burned **242 s of the 863 s suite** waiting out a 120 s readiness
   timeout before failing.

## Governing rule

**No test-only branch in production code.** Where a test needs different behaviour, production
grows a config knob whose default *is* today's production value, and the test supplies its value
through that same knob. Precedence: `defaults ← daemon.yaml ← TDDY_*`.

## Production delta

**tddy-daemon `config.rs`** — three new knobs, all defaulting to today's hardcoded values:

| Key | Default | Replaces |
|---|---|---|
| `spawn_startup_grace_period_ms` | 500 | `spawner::STARTUP_GRACE_PERIOD` |
| `spawn_startup_poll_interval_ms` | 25 | the spawner's hardcoded poll cadence |
| `agent_warmup.timeout_secs` | 120 | `WarmupOptions::default().timeout` |
| `agent_warmup.retry_interval_ms` | 1000 | `WarmupOptions::default().retry_interval` |
| `agent_warmup.request_timeout_secs` | 120 | `WarmupOptions::default().request_timeout` |

Accessors `spawn_startup_grace_period()`, `spawn_startup_poll_interval()` and
`agent_warmup_options()` clamp with `.max(1)`, per the existing duration convention. Env overrides
(`TDDY_SPAWN_STARTUP_GRACE_PERIOD_MS`, `TDDY_SPAWN_STARTUP_POLL_INTERVAL_MS`,
`TDDY_AGENT_WARMUP_TIMEOUT_SECS`, `TDDY_AGENT_WARMUP_RETRY_INTERVAL_MS`,
`TDDY_AGENT_WARMUP_REQUEST_TIMEOUT_SECS`) are applied by `apply_timing_env_overrides` in the
existing post-load mutating pass; an unparseable value warns and keeps the file value. A test pins
that `agent_warmup_options()` on a default config equals `WarmupOptions::default()` — the config
path changed no production behaviour.

**`spawner::StartupWatch { grace, poll }`** — a value type replacing the two consts, threaded
through `spawn_as_user`. `spawn_as_user` also runs inside the **forked spawn-worker**, which holds
no `DaemonConfig` (and is forked *before* env overrides are applied), so the two values travel per
request as `SpawnRequest.startup_grace_period_ms` / `startup_poll_interval_ms`, following the
`child_log_level` / `child_log_format` precedent (`#[serde(default = …)]`, so a legacy client's
JSON decodes to the production default). `supervisor_spawn::watch_session_startup` takes the same
value type; `wait_for_exit` keeps its own `CLONE_POLL_INTERVAL` const, since how often a *clone* is
polled is a different concern from how long a session is watched for an early exit.

**Resume now warms up.** `relaunch_sandboxed_runner` never called `warm_up_agents`, so a resumed
session handed the agent subagents whose first call could stall on a cold model — while both start
paths gated on exactly that. The gate is now applied there too, with the same `failed_precondition`
and no fallback.

## Docs correction owed to `packages/tddy-daemon/docs/connection-service.md`

That file's **§ Specialized-agent warm-up gate** (and § Lifecycle → `ResumeSession`) states:

> Resume reuses this start path, so it is gated too.

**This was never true** — `ResumeSession` goes through `relaunch_sandboxed_runner`, not
`start_sandboxed_claude_cli_session`. It is true *now*, but for a different reason, and the text
should say so:

> Resume does not reuse this start path — `relaunch_sandboxed_runner` runs the same gate before
> relaunching the jail, so a resumed session's subagents are as ready as a fresh one's.

The same paragraph should record that the daemon's `WarmupOptions` now come from the `agent_warmup`
config section rather than `WarmupOptions::default()` (`120 s` budget unchanged), and that the
budget is overridable per-process by the three `TDDY_AGENT_WARMUP_*` env vars.

`docs/ft/coder/specialized-subagents.md` § Start-time warm-up gate has already been corrected
directly (product docs are editable in place).

## Test-side delta

**`tddy-testing-commons`** gained three modules:

- `wait.rs` — `eventually` / `eventually_awaiting` / `eventually_blocking`, 25 ms cadence, panic
  message carrying the condition, the ceiling, the poll count and the **last observed state**.
- `stub_scripts.rs` — `a_stub_agent_script(dir, name)` builder. The recording variant writes to
  `"$f.tmp.$$"` then `mv -f`, and the appending variant emits a single pre-built line through one
  `printf`, so a reader can never observe a half-written argv record. (Measured: 526,770 torn
  observations for the old stub under injected preemption, 0 for the new one.)
- `stub_http.rs` — `a_stub_http_endpoint_answering_ok()`, a loopback listener on `:0` that drains
  request headers **and** the declared `Content-Length` body before replying. A stub that replies
  while the client is still writing gets that write reset; the warm-up gate classifies a connection
  error as *transient* and retries it until the budget elapses, so the drain is what makes the
  `200` dependable under load. Needs `"net"` + `"io-util"` on this crate's existing `tokio`
  dependency — a feature addition, no new crate.

**Per-failure fixes** — argv files read through `eventually_blocking` instead of not waiting at
all; one `PTY_STUB_OUTPUT: Duration = 10s` in a new `packages/tddy-daemon/tests/common/mod.rs`
replacing six near-identical copies and a 2000/10000 ms split; `a_port_no_one_is_listening_on()`
probing **outside** the kernel's ephemeral range so a concurrent test cannot be handed the port out
from under the fixture; the supervisor's `await_socket` replaced by `await_ready`, which polls
`SupervisorClient::connect` **and** `try_wait` for the whole window and drains stderr only on the
confirmed-exit path (a dead supervisor left its socket inode behind, so every later connect got
`ECONNREFUSED` with no diagnosis); and a reverse-RPC readiness handshake in the execute-tool stdio
fixture, splitting the old single 900 ms budget into `FIXTURE_READY: 5s` for start-up and
`CALL_TIMEOUT: 1s` for the call the budget was meant to measure.

**The two SGLang-dependent tests are now hermetic**: each writes a `fastcontext` agent def into the
session's `<tddyhome>/agents` pointing `base_url` at a `stub_http` endpoint (a user def fully
replaces the same-named builtin), and picks a 10 s warm-up budget through the new config section.
Both binaries went from **121 s each, failing** to **12.1 s and 6.8 s, passing** — nothing outside
the repo has to be running.

## Deliberately out of scope (flagged, not silently dropped)

- ⚠️ `packages/tddy-sandbox-app/src/main.rs` keeps `WarmupOptions::default()` — it has its own
  config schema. Consequence: a daemon-hosted and a standalone session on the same host can warm up
  with different budgets.
- `sandbox_session.rs::pick_free_loopback_port` and `allocate_verified_grpc_listen_port` share the
  same TOCTOU shape in **production**; not hardened here.
- The supervisor's unread stderr pipe can deadlock under `RUST_LOG=debug`.
- `spawn_startup_poll_interval_ms > spawn_startup_grace_period_ms` is left unvalidated (the clamp
  makes it harmless).
- ⚠️ `packages/tddy-daemon/tests/worktree_files_rpc.rs:188` has a **pre-existing** `cargo fmt
  --check` failure from `5bd24ad1` (#375); untouched.
