# Investigation: Seatbelt spawn (sandbox-runner never started in jail)

**Date:** 2026-06-27  
**Status:** ✅ **Resolved.** Full in-jail spawn, SessionChannel egress, daemon acceptance, lifecycle (delete/resume), and claude-cli integration tests are green.  
**The original diagnosis (loopback network rules cause the SIGABRT) was wrong.**  
**Related changeset:** [2026-06-27-darwin-sandbox-claude-cli.md](./2026-06-27-darwin-sandbox-claude-cli.md)  
**Feature PRD:** [docs/ft/1-WIP/PRD-2026-06-27-darwin-sandbox-claude-cli.md](../../ft/1-WIP/PRD-2026-06-27-darwin-sandbox-claude-cli.md)

---

## TL;DR

The "SIGABRT under loopback network rules" hypothesis was a **red herring**. The blocker was a
chain of **eight** distinct sandbox/path/test issues, none of them the network policy. Each was
peeled back by reproducing at the profile level (`sandbox-exec -f profile.sb …`), reading dyld
crash reports + runner boot logs, and a real-runner repro that toggled `/tmp` vs `/private/tmp`.

| # | Symptom | Real cause | Fix | State |
|---|---------|-----------|-----|-------|
| 1 | `/bin/echo` SIGABRTs in jail (abort trap 6 / exit 134), before `main()` | dyld `CacheFinder` reads the **root dir node `/`** to find the shared cache; read denied | add `(literal "/")` to `file-read*` | ✅ |
| 2 | runner: `bind egress shim … failed to lookup address information` | `getaddrinfo("localhost")` fails — **no resolver** in clean-env jail | bind/dial literal **`127.0.0.1`** at runtime | ✅ |
| 3 | profile invalid: `host must be * or localhost in network address` | Seatbelt TCP filter **rejects literal IPs**; only `*`/`localhost` allowed | keep `localhost` keyword in the **SBPL rule** (runtime still uses `127.0.0.1`) | ✅ |
| 4 | runner: `tool ipc bind failed: Operation not permitted` | `(deny network*)` also blocks the **AF_UNIX** tool-IPC socket bind | allow `(local/remote unix-socket)` for bind/in/out | ✅ |
| 5 | still `tool ipc bind … Operation not permitted` when project is under `/tmp` | Seatbelt checks the **symlink-resolved** path; profile had `/private/tmp/…` but the socket bound at `/tmp/…` | **canonicalize** sandbox paths (daemon + `render_profile`) | ✅ |
| 6 | runner: `openpty: failed to openpty: Operation not permitted` | openpty opens `/dev/ptmx` + `/dev/ttysN` **O_RDWR**; read missing | add PTY device **reads** to `file-read*` | ✅ |
| 7 | runner: `spawn claude in pty: Operation not permitted` | forking the PTY child is denied — macOS sandbox-exec does **not** allow `process-fork` by default | add `(allow process-fork)` | ✅ |
| 8 | runner: `tool ipc bind … path must be shorter than SUN_LEN` | canonical session path overflows the **104-byte AF_UNIX limit** | short out-of-tree socket path + explicit literal allow (`SandboxSpec::ipc_socket` / `short_ipc_socket_path`) | ✅ |
| 8b | `spawn claude in pty: … doesn't exist (EPERM)` for a `/tmp` binary | runner exec'd the **non-canonical** `--claude-binary` path; allow-list was canonical | canonicalize exec binary paths in the daemon argv | ✅ |

The in-jail runner now reaches, end to end:

```
boot: tool ipc server ready          ✅   pty: openpty …                ✅
boot: egress shim listening …        ✅   pty: claude spawned           ✅
boot: ready marker written …         ✅   EGRESS_PROBE: direct=denied   ✅
gRPC listening …                     ✅   EGRESS_PROBE: session_channel=ok  ✅ (relayed)
```

### Test results (2026-06-27, post-handoff)

```
tddy-sandbox-darwin                         ok
tddy-tools  sandbox_runner_acceptance         ok
tddy-tools  sandbox_runner_behavior_acceptance ok
tddy-daemon sandbox_runner_spawn_smoke        ok
tddy-daemon sandbox_behavior_acceptance       5/5 ok
tddy-daemon sandboxed_claude_cli_acceptance   4/4 ok
tddy-daemon sandboxed_session_lifecycle       2/2 ok
```

### Follow-up fixes (second agent pass)

| Issue | Fix |
|-------|-----|
| Integration tests used `target/debug/deps/tddy-tools` (exit 127) | `resolve_tddy_tools_path()` — config → `CARGO_BIN_EXE` → parent of `deps/` |
| Tool IPC test looked for socket under session dir | Use `SandboxSpec::short_ipc_socket_path(session_id)` |
| Read probe failed (uncommitted README) | Commit README before worktree spawn in acceptance test |
| `streams_demo_tui_dimensions` race (empty terminal replay) | PTY backlog in runner relay + host capture buffer on `StreamTerminalOutput` |
| Delete did not kill sandbox child | Keep `SandboxHandle` in `SandboxSessionState::stop()` |
| Resume called full `start_sandboxed` (worktree EPERM) | `relaunch_sandboxed_runner()` — respawn only; clear `context_dir` before copy |
| `SessionMetadata { sandbox }` missing in unit tests | Added `sandbox: None` to test initializers |

---

## How the diagnosis was done (so it can be repeated)

1. **Profile-level repro, not test-level.** `sandbox-exec -f profile.sb /bin/echo hi`
   reproduced the SIGABRT in isolation and proved it happened **with the empty
   `(deny network*)` policy too** — instantly killing the network hypothesis.
2. **Crash report read.** `~/Library/Logs/DiagnosticReports/echo-*.ips` gave the backtrace:
   `__abort_with_payload → … → dyld4::CacheFinder::CacheFinder(...) → dyld4::start`. That is
   dyld aborting while locating the shared cache → a missing **read**, not a network op.
3. **Template bisection.** Cutting the `file-read*` block down showed `(literal "/")` is the
   single line that flips `/bin/echo` from exit 134 → exit 0.
4. **Real-runner repro in a persistent dir.** A throwaway test rendered the *actual* profile
   via `render_profile` and spawned the *real* `tddy-tools sandbox-runner`, surfacing issues
   2→6 in sequence as each prior fix landed. Pointing the base at `/tmp/…` vs `/private/tmp/…`
   isolated the symlink bug (#5) precisely.

### dyld-cache abort proof (issue #1)

```
__abort_with_payload → abort_with_reason → ignition_halt → boot_boot → ignite
→ dyld4::CacheFinder::CacheFinder(...)
→ dyld4::ProcessConfig::DyldCache::DyldCache(...)
→ dyld4::ProcessConfig::ProcessConfig(...)
→ dyld4::start(...) → start
```
(EXC_CRASH / SIGABRT, macOS 15.5, `/usr/lib/dyld`.)

`(literal "/")` reproduction matrix (`/bin/echo hi` under the full template):

| Profile | Result |
|---------|--------|
| Full template, `(deny network*)` only, **no** `(literal "/")` | exit 134 (SIGABRT) |
| Full template, loopback rules, **no** `(literal "/")` | exit 134 (SIGABRT) |
| Full template, either policy, **+** `(literal "/")` | exit 0 ✅ |

---

## Changes made

1. **`profiles/sandbox-claude.sb.tmpl`**
   - Added `(literal "/")` to `file-read*` (issue #1) — the root *node* only, not its subtree.
   - Added `(literal "/dev/ptmx")` + `(regex #"^/dev/ttys[0-9]+$")` to `file-read*` (issue #6).
2. **`packages/tddy-sandbox-darwin/src/profile.rs`**
   - `canonical_rule_path()` — canonicalize project/scratch/egress/read paths so SBPL rules
     match the symlink-resolved paths Seatbelt evaluates (issue #5).
   - Network policy: always allow `(local/remote unix-socket)` bind/inbound/outbound (issue #4);
     loopback TCP re-allows use the `localhost` keyword (issue #3); `(deny network*)` otherwise.
3. **`packages/tddy-tools/src/sandbox_runner.rs`** — bind/dial **`127.0.0.1`** instead of
   `localhost` for the gRPC server and egress shim (issue #2).
4. **`packages/tddy-daemon/src/connection_service.rs`** — `std::fs::canonicalize` the sandbox
   root + egress dir after creating them, so argv paths (tool-IPC socket, ready marker, …)
   match the canonical profile (issue #5).
5. **`packages/tddy-sandbox/src/spec.rs`** — `SandboxHandle::try_exit_diagnostic()` decodes a
   dead child's termination (signal vs exit code) with a SIGABRT/dyld-cache hint (visibility).
6. **`packages/tddy-daemon/src/sandbox_session.rs`** — `wait_for_sandbox_ready` polls the child
   and **fails fast** with the decoded reason instead of blocking 120s (added `&mut SandboxHandle`
   arg; call site updated).
7. **`tests/profile_loopback_validity.rs`** — now asserts `status.success()` + stdout `"hi"`
   (was `code() != Some(6)`, which a signal-terminated child trivially passed — the false
   confidence that hid issue #1).

### Verification

```
./dev cargo test -p tddy-sandbox-darwin            # all green (incl. strengthened loopback test)
./dev cargo test -p tddy-daemon --test sandbox_repro_tmp   # runner boots; only PTY-spawn EPERM remains
```

`sandbox_runner_spawn_smoke` / `sandbox_behavior_acceptance` will pass through #1–#6 but still
fail at #7 until the PTY child-spawn deny is resolved. (NOTE: a throwaway
`packages/tddy-daemon/tests/sandbox_repro_tmp.rs` exists for diagnosis — **delete it** before
finishing.)

---

## ❗ Remaining issue: PTY child spawn EPERM

The runner now allocates a PTY successfully (`openpty` OK) but fails to fork/exec the agent
binary into the PTY slave:

```
ERROR step=spawn_claude_pty error=spawn claude in pty: Operation not permitted (os error 1)
```

- Site: `packages/tddy-tools/src/sandbox_runner.rs` ~L457, `pair.slave.spawn_command(cmd)`
  (`portable_pty` → fork + `setsid()` + `TIOCSCTTY` on the slave + `chdir(cwd)` + exec).
- The test uses `--claude-binary /bin/sleep`; `/bin` is in `process-exec*`, so a plain exec is
  allowed — the EPERM is from one of the PTY child-setup steps, not the binary path.
- The deny is **not** captured by `log stream`/`log show` with the obvious predicates; the
  kernel does not surface this one through the usual sandbox subsystem.

### Troubleshooting paths (ordered)

1. **Get the exact denied op via Seatbelt tracing.** `(trace)` alone is *not* permissive
   (exec is denied). Use a permissive trace profile to capture every operation the PTY child
   needs, then diff against the template:
   ```scheme
   (version 1)
   (allow default)
   (trace "/tmp/sbtrace.sb")
   ```
   Run the real runner under it (`sandbox-exec -f trace.sb /usr/bin/env -i … tddy-tools sandbox-runner …`),
   then `grep -iE 'ttys|ptmx|/dev/tty|process|pseudo-tty|fork|priv' /tmp/sbtrace.sb`. The
   generated rules show what an unconstrained run touches around the spawn.
2. **Likely candidates to allow** (test additively against `sandbox_repro_tmp`):
   - `(allow file-read* file-write* file-ioctl (regex #"^/dev/tty.*"))` — the controlling
     terminal `/dev/tty` (distinct from `/dev/ttysN`); `TIOCSCTTY` may open `/dev/tty`.
   - `(allow process-fork)` / `(allow process-exec*)` already present — confirm the agent
     binary's real path (node/claude, not `/bin/sleep`) is on the exec allow-list in prod.
   - `(allow pseudo-tty)` is present; verify it is actually a recognized operation on macOS 15
     (it parses, but may be a no-op — the real grant may be the `/dev/tty*` file rules).
3. **Isolate fork vs exec vs TIOCSCTTY.** Temporarily replace `spawn_command` with a bare
   `Command::new("/bin/sleep").spawn()` (no PTY) under the same profile; if that works, the
   EPERM is in the PTY/`setsid`/`TIOCSCTTY` path, not generic exec.
4. **Compare with `~/Code/expo-darwin-sandbox`** for its PTY/tty rules, if it spawns ttys.

### Open questions

1. Does `portable_pty`'s child setup open `/dev/tty` (controlling terminal) — needing a
   `/dev/tty` file rule beyond `/dev/ttysN`?
2. Is `TIOCSCTTY` gated by `file-ioctl` on the slave, or by a process/`pseudo-tty` operation?
3. Will the **real** agent binary (node-based `claude`) need additional exec/read allows
   (node runtime, `/usr/bin/env` shebang resolution) beyond what `/bin/sleep` exercises?

The original handoff narrative is retained below for history — note its central
"loopback network rules cause SIGABRT" claim is **disproven**.

---

## Context documents

| Document | Purpose |
|----------|---------|
| [PRD: Darwin-Sandboxed Claude CLI](../../ft/1-WIP/PRD-2026-06-27-darwin-sandbox-claude-cli.md) | Product requirements: Seatbelt jail, `(deny network*)`, SessionChannel egress |
| [Changeset: Darwin sandbox Claude CLI](./2026-06-27-darwin-sandbox-claude-cli.md) | Implementation status, architecture, acceptance test matrix |
| [Testing practices](../guides/testing.md) | Fluent-tests style, acceptance test conventions |
| Reference PoC | `~/Code/expo-darwin-sandbox` — prior art for Seatbelt + build confinement (broader read policy than this feature) |

### Key code locations

| Area | Path |
|------|------|
| SBPL template | `packages/tddy-sandbox-darwin/profiles/sandbox-claude.sb.tmpl` |
| Profile render + loopback network rules | `packages/tddy-sandbox-darwin/src/profile.rs` |
| `sandbox-exec` spawn wrapper | `packages/tddy-sandbox-darwin/src/spawn.rs` |
| Allow-list builder (`otool -L`, toolchain detect) | `packages/tddy-daemon/src/sandbox_session.rs` (`build_allow_read_paths`) |
| In-jail runner + egress shim | `packages/tddy-tools/src/sandbox_runner.rs` |
| Host SessionChannel bridge | `packages/tddy-daemon/src/sandbox_session.rs` (`dial_and_bridge`, `relay_egress_request`) |
| Boot / failure diagnostics | `packages/tddy-sandbox/src/log.rs`, runner `boot_log()` in `sandbox_runner.rs` |
| Diagnostic probe test | `packages/tddy-daemon/tests/sandbox_runner_inspect.rs` |
| Smoke test (failing) | `packages/tddy-daemon/tests/sandbox_runner_spawn_smoke.rs` |
| Daemon acceptance (failing) | `packages/tddy-daemon/tests/sandbox_behavior_acceptance.rs` |
| Confinement tests (**passing**) | `packages/tddy-sandbox-darwin/tests/seatbelt_confinement_acceptance.rs` |
| Loopback profile test (**misleading pass**) | `packages/tddy-sandbox-darwin/tests/profile_loopback_validity.rs` |

---

## Problem being solved

**Goal:** Run Claude Code CLI inside a macOS Seatbelt sandbox so a runaway agent cannot write or read outside an explicit allow-list, and cannot open outbound network sockets. External reachability (LLM HTTP, MCP tool side-effects) is relayed by the host over a bidi **`SessionChannel`** gRPC stream — not via `HTTPS_PROXY` or a host TCP proxy.

**Target spawn flow:**

```
tddy-daemon
  └─► sandbox-exec -f profile.sb
        └─► env -i HOME=… TMPDIR=… tddy-tools sandbox-runner …
              ├─► binds loopback gRPC port (pre-declared in SBPL)
              ├─► binds loopback egress HTTP shim port
              ├─► writes sandbox.ready marker
              └─► host dials in; EgressRequest frames relayed to reqwest
```

**Current blocker:** The `sandbox-exec` child dies with **SIGABRT** before `sandbox-runner` reaches `main()` (no boot log files). All daemon-level acceptance that depends on in-jail spawn is blocked.

---

## What was implemented (not the blocker)

These pieces appear to work **outside** Seatbelt or in unit/integration tests that do not use loopback network rules:

- **`SessionChannel` proto** — `EgressRequest` / `EgressResponse` on `SessionFrame` (`packages/tddy-service/proto/sandbox.proto`)
- **In-jail egress HTTP shim** — `TDDY_EGRESS_SHIM`, fixed loopback ports, `SandboxSessionRelay` queue (`packages/tddy-tools/src/sandbox_runner.rs`)
- **Host egress relay** — `relay_egress_request` in `dial_and_bridge` (`packages/tddy-daemon/src/sandbox_session.rs`)
- **Boot logging** — dual-write to `{egress}/sandbox-runner.log` and `{project}/sandbox-runner.boot.log`; panic hook writes `sandbox-runner.failure`
- **Superseded TCP proxy removed** — `egress_proxy.rs` deleted; no `HTTPS_PROXY` in spawn path
- **Runner behavior tests (host-side)** — `tddy-tools` SessionChannel PTY/tool tests pass when runner is not jailed

Manual run of `tddy-tools sandbox-runner` **outside** `sandbox-exec` completes boot logging and writes the ready marker.

---

## What was attempted and failed

### 1. Full daemon spawn + acceptance tests

**Files:** `sandbox_runner_spawn_smoke.rs`, `sandbox_behavior_acceptance.rs`

**Result:** Child exits with `ExitStatus(unix_wait_status(6))` (SIGABRT). Ready marker never appears. `wait_for_sandbox_ready` times out (120s).

**Observed artifacts inside egress dir:**

- `sandbox-spawn.json` — written (spawn succeeded at host level)
- `sandbox-exec.stderr.log` — only `"sandbox-exec spawned pid=…"` (wrapper started)
- `sandbox-exec.stdout.log` — **empty**
- `sandbox-runner.log` / `sandbox-runner.boot.log` — **missing** (runner never reached `boot_log()`)

### 2. Diagnostic inspect test

**File:** `packages/tddy-daemon/tests/sandbox_runner_inspect.rs`

Runs multiple `sandbox-exec` probes against the same profile used for real spawn. Latest output (nix dev shell, macOS):

```
allow_read_paths (13): nix apple-sdk, nodejs bin, homebrew, bash, target/debug,
  CoreFoundation.framework, libiconv, gettext, /usr/lib, /bin (duplicates)
profile bytes=3605

probe echo                    exit=None success=false
probe tools-help-direct       exit=None success=false
probe tools-help-via-env-i    exit=None success=false
probe runner-via-env-i        exit=None success=false
minimal profile echo          exit=None success=false   (only /usr/bin allow-list)
full profile echo             exit=None success=false

spawn via spawn() → try_wait after 2s: ExitStatus(unix_wait_status(6))
```

**Interpretation:** Even `/bin/echo hi` aborts inside the profile. This is not a `tddy-tools`-specific failure — it is a Seatbelt / SBPL / runtime confinement issue.

### 3. Expanding `allow_read_paths` via `otool -L`

**File:** `build_allow_read_paths()` in `sandbox_session.rs`

Added Mach-O load-path detection for `tddy-tools` and `--claude-binary` (`/bin/sleep` in tests). Allow-list grew to 13 entries including nix store paths.

**Result:** No improvement. All probes still SIGABRT.

**Note:** An earlier hypothesis (“bad allow-list entry breaks profile”) is **weakened** because the **minimal-profile control** (only `/usr/bin` in `@ALLOW_READ_PATHS@`) also fails when loopback ports are present.

### 4. Smoke-test preflight checks

**File:** `sandbox_runner_spawn_smoke.rs` lines 105–134

Preflight asserts `status.code() != Some(6)` for echo and `--help` probes.

**Result:** Preflight **passes even when probes abort**, because signal termination yields `status.code() == None`, not `Some(6)`. Exit code `6` specifically means “invalid SBPL syntax”; SIGABRT is a **runtime** Seatbelt violation, not a parse error.

### 5. `profile_loopback_validity` test assumed green

**File:** `packages/tddy-sandbox-darwin/tests/profile_loopback_validity.rs`

Test passes with `assert_ne!(output.status.code(), Some(6))` only — **does not assert `status.success()`**.

**Result:** Test reports **ok** while `/bin/echo` may still SIGABRT. This test does **not** prove loopback profiles are runnable.

### 6. Manual shell probe (incomplete but indicative)

Ad-hoc profile with loopback rules (`network-bind`, `network-outbound`, `network-inbound` for `localhost:55900`) run via `sandbox-exec /bin/echo hi` in nix dev shell:

```
Abort trap: 6
exit=134
```

(134 = 128 + 6 = SIGABRT)

---

## What is currently known

### Symptom summary

| Observation | Detail |
|-------------|--------|
| Failure mode | SIGABRT (`wait status 6`, `status.code() == None`) |
| When | Immediately on any command under profiles that include **loopback network rules** |
| Scope | Affects `/bin/echo`, `/usr/bin/env -i …`, `tddy-tools`, full runner argv |
| Outside jail | `sandbox-runner` starts, logs, writes ready marker |
| Boot logs in jail | Absent → crash before runner initialization |
| Profile parse | No SBPL parse error on stderr (would be exit 65 / explicit `sandbox-exec: …` message) |

### Tests that pass vs fail

| Test | `loopback_allow_ports` | Result |
|------|------------------------|--------|
| `seatbelt_confinement_acceptance` (write/read deny) | `vec![]` → `(deny network*)` only | ✅ 2/2 |
| `profile_loopback_validity` | `vec![55900, 55901]` | ✅ passes assertion but **does not verify echo succeeds** |
| `sandbox_runner_inspect` | pre-assigned grpc + shim ports | All probes SIGABRT |
| `sandbox_runner_spawn_smoke` | grpc + shim ports | SIGABRT, no ready marker |
| `sandbox_behavior_acceptance` | grpc + shim ports | Timeout waiting for ready marker |

**Strongest lead:** Profiles **without** per-port loopback allows work for basic shell confinement tests. Profiles **with** the loopback network policy rendered in `profile.rs` appear to cause **runtime SIGABRT** for otherwise valid commands.

Current loopback policy (from `packages/tddy-sandbox-darwin/src/profile.rs`):

```scheme
(deny network*)
(allow network-bind (local tcp "localhost:*"))
(allow network-outbound (remote tcp "localhost:{port}"))
(allow network-inbound (local tcp "localhost:{port}"))
```

(repeated per pre-assigned port in `loopback_allow_ports`)

### Environment notes

- Development runs inside **nix dev shell** (`./dev`). Temp dirs look like `/tmp/nix-shell.*/.tmp*/project`.
- `allow_read_paths` includes **nix store** paths (apple-sdk, bash, libiconv, gettext, nodejs).
- `DARWIN_BASE` in profile derives from `$TMPDIR` parent chain (`profile.rs::darwin_user_temp_base`) — may be `/tmp` or `/private/var/folders` depending on TMPDIR. Unlikely to cause SIGABRT but affects write allow scope.
- Spawn command uses **non-canonical** binary path: `…/packages/tddy-daemon/../../target/debug/tddy-tools` (should canonicalize).

### Secondary issues (not primary blocker)

| Issue | Impact |
|-------|--------|
| `SessionMetadata` missing `sandbox` field in several daemon unit-test initializers | `cargo test -p tddy-daemon` (lib tests) fails to compile; integration tests via `--test …` still run |
| Smoke preflight checks wrong predicate | Masks SIGABRT failures |
| `profile_loopback_validity` weak assertion | False confidence |
| Possible runner behavior test regression | `Connection refused` to gRPC when run outside jail context — separate from Seatbelt SIGABRT |

---

## Architecture diagram (intended vs actual)

```mermaid
flowchart TB
  subgraph intended [Intended — blocked at spawn]
    D[tddy-daemon] --> SE[sandbox-exec]
    SE --> ENV["env -i …"]
    ENV --> SR[sandbox-runner]
    SR --> GRPC[loopback gRPC bind]
    SR --> SHIM[loopback egress shim]
    SR --> READY[sandbox.ready marker]
    D -->|SessionChannel dial| GRPC
    SR -->|EgressRequest| verify| SHIM
  end

  subgraph actual [Actual today]
    SE2[sandbox-exec] --> ABORT[SIGABRT before runner main]
  end
```

---

## Possible paths to try next

Ordered by likelihood / cost. A new agent should treat **loopback SBPL rules** as the top hypothesis until disproven.

### A. Fix loopback network policy (highest priority)

1. **Reproduce minimally:** Render profile with `loopback_allow_ports: vec![55900]` vs `vec![]`. Run `/bin/echo hi` under each. Confirm empty ports work, non-empty abort.
2. **Consult Seatbelt / SBPL docs and `expo-darwin-sandbox`:** Compare network rule syntax. Try alternatives:
   - `(allow network* (local ip "127.0.0.0/8"))` or `(remote ip "127.0.0.0/8")` instead of `localhost` string forms
   - `(allow network-bind (local tcp "*:PORT"))` without wildcard bind
   - Separate `(allow network-outbound (remote tcp "127.0.0.1:PORT"))` vs `localhost`
   - `(allow network-inbound (local tcp "127.0.0.1:PORT"))` only (host dials in — outbound from jail to shim may need different rule)
3. **Apple `sandbox-exec` logging:** Run with `SANDBOX_LOG=1` or check Console.app / `log stream` for Seatbelt violation message at abort time.
4. **Fix `profile_loopback_validity`:** Assert `status.success()` and stdout contains `hi`. Same for smoke preflight.

### B. Alternative networking model (if loopback SBPL is wrong approach)

1. **Unix domain sockets only:** Runner already has `--grpc-socket` path; if gRPC can bind UDS instead of TCP loopback, SBPL may not need `network-*` allows at all. Egress shim could also use UDS + `SessionChannel` without HTTP TCP.
2. **Host-initiated connections only:** If runner binds listeners, Seatbelt may need only `network-inbound` on specific ports, not `network-bind (localhost:*)` wildcard.
3. **Revisit superseded proxy** only if product accepts outbound network from jail — **explicitly rejected by PRD** unless developer consents.

### C. Allow-list / binary loading (lower priority after minimal echo fails)

1. **Binary-search `allow_read_paths`:** Temporarily remove nix/homebrew entries; test if full profile without loopback works (baseline from confinement tests already does).
2. **Canonicalize all binary paths** before profile generation and `process-exec*` rules.
3. **Run `otool -L` on `/bin/echo` and `/bin/sleep`** — unlikely relevant if echo alone aborts under loopback profile.
4. **Compare with expo-darwin-sandbox profile** for read/exec/network sections.

### D. Test harness improvements

1. **Keep `sandbox_runner_inspect.rs`** (or move to `tddy-sandbox-darwin`) as a permanent diagnostic until spawn is green.
2. **Add matrix test:** `{no loopback, loopback}` × `{minimal allow-list, full allow-list}` × `{echo, spawn}`.
3. **Dump rendered profile to egress** on spawn failure (already partially available via inspect test writing `profile.sb`).

### E. Unblock compile / cleanup

1. Add `sandbox: None` (or appropriate default) to all `SessionMetadata { … }` test initializers in `tddy-daemon`.
2. Canonicalize `tddy-tools` path in spawn argv.
3. Re-run `./test -p tddy-daemon --test sandbox_runner_spawn_smoke` and `./test -p tddy-daemon --test sandbox_behavior_acceptance` after Seatbelt fix.

---

## Suggested verification sequence (for next agent)

```bash
# 1. Diagnostic inspect (integration test only — avoids lib test compile errors)
./dev cargo test -p tddy-daemon --test sandbox_runner_inspect -- --nocapture

# 2. Confinement baseline (no loopback — should pass)
./dev cargo test -p tddy-daemon -p tddy-sandbox-darwin seatbelt_denies -- --nocapture

# 3. After fixing loopback SBPL — strengthen and run:
./dev cargo test -p tddy-sandbox-darwin loopback_network_profile_is_accepted_by_sandbox_exec -- --nocapture

# 4. Smoke then daemon acceptance
./dev cargo test -p tddy-daemon --test sandbox_runner_spawn_smoke -- --nocapture
./dev cargo test -p tddy-daemon --test sandbox_behavior_acceptance -- --nocapture

# 5. Full package verify
./verify
# Read .verify-result.txt for evidence
```

---

## Open questions

1. Is `localhost` a valid Seatbelt network matcher, or must rules use `127.0.0.1` / CIDR?
2. Does `(allow network-bind (local tcp "localhost:*"))` alone trigger SIGABRT even before any bind attempt?
3. Does nix dev shell / SIP / macOS version affect `sandbox-exec` network rules differently than plain `/tmp` paths?
4. Can tonic/hyper bind UDS on macOS for this use case, eliminating TCP loopback from the jail entirely?

---

## Summary for handoff

> **⚠️ Superseded by the [RESOLUTION](#-resolution-2026-06-27) section at the top.** The
> hypothesis below (loopback TCP rules cause the SIGABRT) was **disproven**. The real cause
> was a missing `(literal "/")` in the read allow-list, which made `dyld` abort while
> locating the shared cache. The text is kept verbatim for history.

**The SessionChannel egress architecture and sandbox-runner logic are largely implemented and work outside the jail.** The production spawn path is blocked because **`sandbox-exec` children SIGABRT under SBPL profiles that include loopback TCP allow rules**, before any sandbox-runner code runs. Existing confinement tests pass only because they use `(deny network*)` without loopback exceptions. The most likely fix is correcting Seatbelt network policy syntax or switching to Unix-domain sockets to avoid network allowances entirely. Fix weak tests that treat “not exit 6” as success before trusting green CI on loopback profiles.
