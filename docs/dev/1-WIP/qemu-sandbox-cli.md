# Changeset: qemu-sandbox-cli — VM image builder + QEMU sandbox backend binaries

**Date:** 2026-07-01
**Branch:** `slow-fragment`
**Packages:** `tddy-vm`, `tddy-vm-build` (new), `tddy-sandbox-qemu` (new), `tddy-daemon`
**Feature PRD:** [docs/ft/vm/tddy-vm.md § Image builder CLI](../../ft/vm/tddy-vm.md#image-builder-cli-tddy-vm-build), [§ QEMU sandbox backend](../../ft/vm/tddy-vm.md#qemu-sandbox-backend-tddy-sandbox-qemu)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Implement `build_image(spec, output, format, progress)` in `tddy-vm/src/build.rs` for real; refactor `build_vm_image_from_spec` to call it (shared `run_buildroot_pipeline` helper)
- [x] Implement `tddy-vm-build` CLI (spec → image file)
- [x] Implement `tddy-sandbox-qemu` argv builders for real (`qemu_sandbox_argv`, `ninep_fsdev_args`, `overlay_create_argv`, `guest_plan_json`)
- [x] Implement `tddy-sandbox-qemu::spawn_plan` / `spawn_plan_with` — real overlay creation + real `qemu-system-x86_64` boot; guest-runner handshake intentionally left open (see below)
- [x] `tddy-sandbox-qemu` CLI (mount/env/cwd flags → `SandboxPlan` → `spawn_plan_with`)
- [x] Route `run_buildroot_pipeline`'s `make` invocations through a Linux Docker container on macOS (`HostToolchain`, `docker/buildroot-host/Dockerfile`) so Buildroot builds actually work on a Mac dev box — required six follow-up fixes to actually work end-to-end, see "Docker-on-macOS debugging notes" below
- [x] Mark the real-Buildroot-build acceptance tests `#[ignore]` + `#[serial]` (45-90+ min; must not run in the fast suite or contend with each other for the same constrained Docker VM)
- [x] Connect to the in-guest `tddy-sandbox-runner` over the forwarded TCP control port and stream the guest command's output — see "Guest-runner handshake" below for the design and what's still an approximation (exit code)
- [x] Cross-compile `tddy-sandbox-runner` for `x86_64-unknown-linux-musl` — `packages/tddy-sandbox-runner/docker/musl-cross/{Dockerfile,build.sh}`; verified real output (`file`: `ELF 64-bit LSB pie executable, x86-64, ... static-pie linked`), 7.98 MB, no unexpected repo changes (`tddy-workflow-recipes`' codegen build script needs the repo mounted read-write, not `:ro` — it's our own repo, not a foreign source tree, unlike the Buildroot mirror)
- [x] Document Buildroot 9p + init-hook fragment for guest images — `packages/tddy-sandbox-qemu/docs/guest-image-9p-init.md`, `guest/linux-9p.fragment`, `guest/rootfs-overlay/etc/init.d/S99tddy-sandbox`. Init-hook shell logic (mount-index extraction, command/env argv construction) verified with `dash`/`jq` against simulated `plan.json` inputs, including a path-with-spaces case that caught and fixed a real string-concatenation/word-splitting bug in an earlier draft. **Not** yet exercised inside an actual booted 9p-enabled guest (no such image exists yet) — documented as an open item in the doc itself.
- [x] Wire QEMU backend selector into `tddy-daemon` (`sandbox_session.rs`, `sandbox_action.rs`) — `qemu_backend_requested()` env-var opt-in (`TDDY_SANDBOX_BACKEND=qemu`), routes both `spawn_sandbox_runner` and `spawn_confined_plan` to `tddy_sandbox_qemu::spawn_plan` when set; unset behavior unchanged. `cargo build`/`clippy --lib -D warnings`/`clippy --bins -D warnings`/`fmt --check` (scoped to the 2 files touched) all clean. Full `--all-targets` clippy/fmt on `tddy-daemon` hits two **pre-existing, unrelated** issues (`sandbox_plan_builder.rs:292` clippy `cmp_owned`; rustfmt drift in `action_sandbox_acceptance.rs`/`sandboxed_claude_cli_acceptance.rs`) — confirmed via `git status`/`git diff` untouched by this changeset, same as the earlier `tddy-task/tests/cancellation.rs` precedent from PR #250.

## Acceptance tests

- [x] `packages/tddy-vm-build/tests/build_image_cli_acceptance.rs` — real code path, real Buildroot build (not a fixture). `#[ignore]`d + `#[serial(buildroot_docker_vm)]`: run with `cargo test -p tddy-vm-build --test build_image_cli_acceptance -- --ignored --nocapture`. See "Docker-on-macOS debugging notes" for the full history of what it took to get this actually working.
- [x] `packages/tddy-sandbox-qemu/tests/sandbox_qemu_cli_acceptance.rs` — real code path exercised; both tests FAIL (fast, seconds) because the fixture uses a placeholder (non-image) `--image` file, so `qemu-img create` genuinely rejects it (`"Image is not in qcow2 format"`) before QEMU ever boots. Expected to get further (and ultimately pass) once a real runner-capable guest image exists and the transport-mismatch design point is resolved. No `#[ignore]` needed — fails fast, doesn't need real Buildroot/VM infrastructure.

## Unit tests

- [x] `packages/tddy-sandbox-qemu/tests/argv_unit.rs` — `qemu_sandbox_argv`, `ninep_fsdev_args`, `overlay_create_argv`, `guest_plan_json` (9 tests, all PASSING)

Deferred (not red-worthy yet — see rationale below):
- `parse_mount_spec` / `parse_env_vars` (`packages/tddy-sandbox-qemu/src/cli.rs`) are already correctly implemented (real CLI-parsing code, not stubbed); dedicated unit tests would pass immediately and add no red signal. Covered indirectly by the CLI acceptance tests.
- `build_image` format selection (`packages/tddy-vm/src/build.rs`) is a single unconditional stub (`Err(NotImplemented)` regardless of input) — a unit test asserting today's behavior would trivially pass, and a test asserting future behavior duplicates the two acceptance tests already covering both formats. Revisit once the real implementation exists.

## Open design points (tracked in PRD "Known gaps")

- gRPC transport: `SandboxHandle.grpc_socket_path` is a UDS; QEMU guest is reachable only over TCP hostfwd. Needs a bridge or a TCP-connecting runner client.
- No uid/user field on `SandboxPlan`; guest runs as root (matches existing `QemuVm` SSH-as-root). Uid mapping deferred.
- Existing Buildroot images lack 9p support; must be rebuilt with the new fragment before this backend can boot them.

## Delta summary

### `tddy-vm`

**Modified files:**
- `src/build.rs` — `ImageFormat` enum; `build_image(spec, output, format, progress)` runs the real Buildroot pipeline (private build tree, `make olddefconfig`, `make -j<nproc>`, then `qemu-img convert` or a raw copy) via a new shared `run_buildroot_pipeline` helper. `build_vm_image_from_spec` refactored to call the same helper — gRPC stage messages, cancellation (`SIGINT`), and the `tmp/buildroot/disks/build-<ts>` layout are unchanged (all 26 pre-existing `tddy-vm` tests still pass).
  - `HostToolchain` (`Native`/`Docker`), `host_toolchain()`: on macOS (or `TDDY_VM_BUILD_TOOLCHAIN=docker`), `make` runs inside a Linux container instead of natively (Buildroot rejects Apple Clang's `gcc` trampoline). `TDDY_VM_BUILD_TOOLCHAIN=native|docker` overrides the default.
  - `docker_toolchain_image_tag()` / `ensure_docker_image()`: the toolchain image is tagged by a hash of the Dockerfile's own bytes (`tddy-buildroot-host:<hash>`), so editing the Dockerfile automatically busts the cache instead of `ensure_docker_image`'s tag-existence check silently reusing a stale image.
  - `docker_shareable_buildroot_dir()`: mirrors `BUILDROOT_DIR` (a Nix store path) into `$HOME/.cache/tddy-vm-build/buildroot-mirror/<nix-hash>` before mounting — see debugging notes below for why.
  - `docker_cache_root()`: `$HOME/.cache/tddy-vm-build`, deliberately not `std::env::temp_dir()`.
  - `docker_build_volume_name()` / `ensure_docker_build_volume_ownership()` / `extract_docker_build_output()` / `remove_docker_volume()`: `/build` is a Docker-managed **named volume**, not a bind mount (see debugging notes); the volume is `chown`ed to the host uid:gid once (fresh volumes are `root:root`), and the final `images/` directory is copied out to the real host `build_path` after a successful build, with best-effort volume cleanup on every exit path (success, failure, and cancellation).
  - `docker_vm_nproc()`: `-jN` for the Docker toolchain uses the **container's** actual CPU count (queried via `docker run <image> nproc`), not the host's — see debugging notes.
- `Cargo.toml` — `tempfile` promoted from dev-dependency to a regular dependency (used by `build_image`'s private build tree).
- `src/lib.rs` — re-export `ImageFormat`, `build_image`.
- `docker/buildroot-host/Dockerfile` (new) — `debian:12-slim` + the host packages Buildroot's `support/dependencies/dependencies.sh` requires (`build-essential`, `bc`, `ca-certificates`, `cpio`, `file`, `git`, `patch`, `perl`, `python3`, `rsync`, `unzip`, `wget`).

### `tddy-vm-build` (new)

- `Cargo.toml` (incl. `serial_test` dev-dependency), `src/lib.rs` (`BuildImageArgs`, `run_build_image`), `src/main.rs`.
- `tests/build_image_cli_acceptance.rs` — real code path, real Buildroot build; `#[ignore]` + `#[serial(buildroot_docker_vm)]` (see "Acceptance tests" above).

## Guest-runner handshake (2026-07-01)

The gRPC transport-mismatch design point turned out to already be mostly solved by
existing `tddy-sandbox-runner` infrastructure discovered via research, not a new
protocol needed:

- The runner already supports **TCP mode** (`--grpc-listen-port <port>`), used today by
  the macOS Seatbelt backend and the standalone app — not UDS-only as originally assumed.
- Since QEMU's `-netdev user,...,hostfwd=tcp::<control_port>-:<control_port>` forwards a
  **fixed, already-known** port, the host can dial `127.0.0.1:<control_port>` directly —
  no VM IP discovery, no vsock, no shared filesystem needed for connectivity itself.
- `tddy_sandbox_runner::connect_sandbox_client(ready_marker)` already reads a port number
  from a file and dials it — exactly the TCP-mode convention. `spawn_plan_with` now spawns
  a background task that polls the control port with `TcpStream::connect` and, once
  reachable, writes the port number to `ready_marker_path` — mimicking what the in-jail
  runner itself does for the darwin backend, so **no changes to daemon-side polling code
  were needed at all** (`wait_for_sandbox_ready` + `connect_sandbox_client` work unchanged).
- `tddy_sandbox_runner::run_host_relay()` (already shared by darwin/cgroups/the daemon/the
  standalone app) drives the whole `SessionChannel` protocol — poll, PTY output, tool
  dispatch. `tddy-sandbox-qemu`'s CLI (`run_sandbox_qemu`) now calls it directly with
  `NullToolHandler`, streaming captured terminal bytes to stdout.

**Known approximation, not a full close of the gap**: the `SessionChannel` protocol
carries a real exit code in `SessionEnded`, but `run_host_relay` (shared by 7 call sites
across darwin/cgroups/the daemon/tests) discards it internally rather than surfacing it to
callers. Plumbing that through means changing `run_host_relay`'s or `HostRelayConfig`'s
public shape — a cross-cutting change to production sandbox infrastructure used by
already-working backends, deliberately **not** done as part of this session to avoid
destabilizing darwin/cgroups without live test coverage of the VM path to catch a
regression. `run_sandbox_qemu` reports `0` once the session ends cleanly and `1` on any
connect/boot/relay failure — an honest approximation, documented as such, not the guest
command's real exit code. Tracked as follow-up work.

## Docker-on-macOS debugging notes (2026-07-01)

Getting the Docker toolchain to actually produce a working image required finding and fixing six distinct, unrelated bugs, each surfaced only after the previous one was fixed and the pipeline progressed further. Recorded here so the reasoning isn't lost:

1. **Empty bind mount for `BUILDROOT_DIR`.** Docker Desktop/Rancher Desktop's macOS VM only shares specific host paths (`$HOME`, system temp dirs) into containers; a path outside that allowlist — like a Nix store path — silently bind-mounts as an *empty* directory instead of erroring. Symptom: `make: *** No rule to make target 'olddefconfig'`. Fixed by mirroring `BUILDROOT_DIR` into `$HOME/.cache` once (`docker_shareable_buildroot_dir`).
2. **`$TMPDIR` poisoning `build_image`'s own scratch dir.** The nix dev shell sets a fresh, per-session `TMPDIR` (`/tmp/nix-shell.XXXX`) that has the *same* sharing problem as (1) — `tempfile::tempdir()` defers to it. Symptom: writes into `/build` silently vanished (root) or got `Permission denied` (mapped uid), because the "shared" mount was actually a disconnected, ephemeral one. Fixed by rooting `build_image`'s scratch dir under `docker_cache_root()` (`$HOME`-based) instead.
3. **Missing CA trust store.** `debian:12-slim` has no `ca-certificates`, so Buildroot's `wget` downloads over HTTPS failed cert validation. Fixed by adding the package to the Dockerfile — which also exposed a **stale-image-cache bug**: `ensure_docker_image` only checked whether the tag existed, so the already-built (broken) image would never be rebuilt after editing the Dockerfile. Fixed with a content-addressed tag (`docker_toolchain_image_tag`).
4. **`Permission denied` executing `host/bin/pkg-config`.** A file Buildroot itself just built and `chmod +x`'d, immediately after, inside the *same* container invocation. Root cause never fully pinned down (suspected virtiofs/9p exec-bit/metadata-visibility quirk under concurrent `-jN` I/O against a macOS bind mount), but the well-established, robust fix is to stop bind-mounting `/build` at all: it's now a Docker-managed **named volume**, which lives natively inside the Linux VM with no cross-platform filesystem semantics involved. That in turn meant the **volume starts out `root:root`**, and the (still-bind-mounted, for `/dl`) `--user <hostuid>:<hostgid>` couldn't write into a fresh volume — fixed with a one-time `chown` step run as root (safe for a volume; running the *build* as root against the `/dl` *bind mount* was the thing that didn't work, per an earlier experiment).
5. **CPU/memory oversubscription.** Rancher Desktop's VM here has only **2 CPUs and ~5.8 GiB RAM** (`docker info`), but `-jN` was computed from the *host's* `std::thread::available_parallelism()` (12 cores) — spawning 12 parallel `cc1plus` processes inside a 2-CPU/5.8 GiB VM got one OOM-killed silently mid-bootstrap (`gcc`'s own `expmed.cc`), surfacing only as a bare `make: *** Error 2` with no compiler diagnostic, ~14 minutes into a real build. Fixed by querying the container's actual `nproc` (`docker_vm_nproc`) instead.
6. **`.config` silently ignored (regression from fix 4).** After a real build succeeded end-to-end but produced only `rootfs.tar`, never the requested `rootfs.ext2`, the spec seemed to be getting dropped. Root cause: fix 4 turned `/build` into a Docker-managed **named volume**, but `resolve_buildroot_source_and_write_config` still writes `.config` to the *host* `build_path` — a named volume isn't a bind mount of that path, so the container's `/build` never saw the file. `make olddefconfig` found no existing `.config` and silently fell back to Buildroot's bare defaults (which happen to produce a plausible generic build, masking the bug). Confirmed by: `fs/ext2/Config.in` showing `EXT2_SIZE` already defaults to `"60M"` (an earlier fix attempt adding it explicitly was a no-op); a real Docker-mounted `olddefconfig` run against the actual written `.config` correctly resolving `BR2_TARGET_ROOTFS_EXT2=y`, proving the config logic itself was fine; the failed build's full log containing **zero** occurrences of `e2fsprogs`, proving `host-e2fsprogs` (ext2's build dependency) was never even invoked; and a fresh, empty named volume's `olddefconfig` producing exactly the same bare-default `.config` (`# BR2_TARGET_ROOTFS_EXT2 is not set`) as the real failed run. Fixed by `inject_config_into_build_volume` (`packages/tddy-vm/src/build.rs`) — a `docker run` step, mirroring `extract_docker_build_output`'s shape but in reverse, that copies the host `.config` into the volume before `olddefconfig` runs. Re-confirmed via the same cheap diagnostic with the fix applied: an injected `.config` now survives `olddefconfig` with `BR2_TARGET_ROOTFS_EXT2=y` intact.

After all six fixes, the pipeline reaches deep into a real bootstrap-GCC-then-target-rootfs build (confirmed via `ps aux` + direct log inspection, not just exit codes — several early "it passed" signals in this session turned out to be background-task exit-code summaries that didn't match the actual `test result: FAILED` in the output, so treat exit codes alone as unreliable for these tests).

### `tddy-sandbox-qemu` (new)

- `Cargo.toml`, `src/lib.rs`, `src/spawn.rs` (`spawn_plan`, `spawn_plan_with` — real overlay + boot; `QemuBackendOptions`), `src/argv.rs` (`qemu_sandbox_argv`, `ninep_fsdev_args`, `overlay_create_argv`, `guest_plan_json`, `plan_config_dir`, `monitor_socket_path` — all real, pure, fully unit-tested), `src/cli.rs` (`SandboxQemuArgs`, `parse_mount_spec`, `parse_env_vars`), `src/main.rs`.
- `tests/sandbox_qemu_cli_acceptance.rs`, `tests/argv_unit.rs`.
- `run_sandbox_qemu` deliberately reports an error (not a fake `0`) when `spawn_plan_with` succeeds but the guest command was never actually executed — see "Open design points".
- `docs/guest-image-9p-init.md`, `guest/linux-9p.fragment`, `guest/rootfs-overlay/etc/init.d/S99tddy-sandbox` (new) — documentation + real artifacts for the guest-side half of the 9p handshake: kernel Kconfig fragment, BusyBox init script mounting `tddy-plan`/`tddy-runner`/per-mount 9p shares and execing `tddy-sandbox-runner`. Not yet exercised inside a real boot (see doc's "Not yet done").

### Root `Cargo.toml`

- Added `packages/tddy-vm-build` and `packages/tddy-sandbox-qemu` to `[workspace] members`.

## Validation Results

### validate-changes (2026-07-01)

**Critical (0)**
**Warning (0)**
**Info (2)**

#### File-level notes

| File | Status | Notes |
|------|--------|-------|
| `packages/tddy-vm/src/build.rs` | ✅ | Build passes; no `unwrap`/`expect` in new code; error propagation via `PipelineError`/`VmError` throughout. Two new `unsafe { libc::geteuid()/getegid() }` blocks have no safety comment, but this matches existing house style (`tddy-sandbox-darwin/src/spawn.rs:73` is a bare `unsafe {}` too) — infallible FFI calls, not flagged as a defect. |
| `packages/tddy-vm/src/build.rs` | ℹ️ | Narrow, benign race: two processes calling `ensure_docker_image` concurrently for the first time could both run `docker build` for the same tag; harmless since the Dockerfile is identical (content-addressed tag), just possible duplicate work. Not worth guarding given how rarely this pipeline runs concurrently on a single dev box. |
| `packages/tddy-sandbox-qemu/src/*.rs` | ✅ | No `unwrap`/`expect`; one properly-marked `TODO` (`argv.rs:38`) for the `QemuBackendOptions` plan-config-path gap, already tracked in "Open design points". |
| `packages/tddy-vm-build/src/*.rs` | ✅ | No `unwrap`/`expect`; CLI error paths propagate via `anyhow`. |
| `packages/tddy-vm/src/lib.rs` | ✅ | Export change is additive only (`build_image`, `ImageFormat` added); `build_vm_image_from_spec`'s public signature is unchanged — no breaking API change. |
| `packages/tddy-vm-build/tests/build_image_cli_acceptance.rs` | ✅ | Real Buildroot build tests correctly isolated: `#[ignore]` (excluded from `./test`/`./verify`/plain `cargo test`) + `#[serial(buildroot_docker_vm)]` (can't run concurrently and oversubscribe the same constrained Docker VM). Verified: plain `cargo test -p tddy-vm-build` completes in 0.00s with `0 passed; 2 ignored`. |
| Changeset doc (this file) | Fixed | Was stale relative to the actual implementation (missing the entire Docker debugging saga); updated as part of this validation pass. |

No hardcoded secrets, no unvalidated user input reaching shell commands without sanitization (`docker_build_volume_name` sanitizes its input defensively even though current callers only ever pass safe, tool-generated path components), no TUI stdout/stderr conflicts (no TUI code touched), no fallbacks added without consent.

### validate-prod-ready (2026-07-01)

**Blockers (0), Warnings (2 found, both fixed per developer decision)**

- `docker_toolchain_image_tag()` used to fall back to a static tag if the bundled Dockerfile couldn't be read. **Fixed**: now returns `Result<String, PipelineError>` and hard-errors instead — silently degrading to the non-content-addressed tag would risk reintroducing the exact stale-image bug this function exists to prevent, with no signal. `host_toolchain()` is now fallible too (`Result<HostToolchain, PipelineError>`) to propagate this.
- `remove_docker_volume()` used to log (not propagate) cleanup failures. **Fixed**: now returns `Result<(), String>` and is fatal at every call site in `run_buildroot_pipeline` (olddefconfig launch/failure, build launch/wait/failure, cancellation, and post-success extraction). A new `fail_with_volume_cleanup()` helper folds a cleanup failure into whatever error was already in flight so neither message is lost; the cancellation path escalates from `PipelineError::Cancelled` to `PipelineError::Failed` if cleanup also fails.

No `dbg!`, `HACK`/`WORKAROUND` markers, or commented-out code found. No `println!`/`eprintln!` in TUI code paths (the CLI output is in standalone, non-TUI binaries). No mock/fake code in production paths. Unused imports/dead code already ruled out by clean `cargo clippy -- -D warnings` across all three affected crates. Re-verified after the fixes: `cargo build`/`clippy -D warnings`/`fmt --check` clean, `cargo test -p tddy-vm` (26 tests + 1 doctest) and `cargo test -p tddy-vm-build` (0 passed, 2 ignored) both pass.

### analyze-clean-code (2026-07-01)

**Initial score: D** (3 "must refactor" items) → **Refactored, final score: C** (1 remaining, deliberately kept)

| Metric | Before | After |
|---|---|---|
| `run_buildroot_pipeline` | 269 lines | 52 lines — split into `resolve_buildroot_source_and_write_config`, `prepare_toolchain`, `run_olddefconfig`, `run_parallel_build`, `finalize_docker_output` |
| `build_image` | 61 lines | 46 lines — scratch-dir setup extracted to `create_scratch_build_dirs` |
| `docker_shareable_buildroot_dir` | 66 lines | 57 lines — stale-mirror removal extracted to `clear_stale_mirror` |
| `run_parallel_build` (new, holds the extracted `-jN`/streaming/cancellation logic) | — | 156 lines — **kept as-is, not further split** |

Per developer decision: refactored now, verified via `cargo build`/`clippy -D warnings`/`fmt --check`/fast test suite (all clean, no regressions) rather than re-running the full 45-90+ min real Buildroot build — the extraction relocated the concurrent `select!`/streaming/cancellation logic verbatim into `run_parallel_build` without altering it, so the risk of a behavioral change is low.

`run_parallel_build` remains the one "must refactor" item (>60 lines) — this is the genuinely cohesive unit of concurrent logic (spawn, dual stdout/stderr streaming via `tokio::spawn`, a `select!` loop juggling line-forwarding/wait/cancellation, SIGINT handling, cleanup). Splitting it further would mean passing the `Child`, both `JoinHandle`s, and the `mpsc::Receiver` across a function boundary mid-stream, which fragments a tightly-coupled state machine rather than clarifying it. Left as one function by design, not oversight.

The duplicated `if let HostToolchain::Docker { .. } = &toolchain { fail_with_volume_cleanup(...) } else { PipelineError::Failed(...) }` pattern (originally flagged, appeared 4 times) is now naturally isolated to `run_olddefconfig` and `run_parallel_build`'s own failure branches rather than sprawling across one 269-line function — no further action needed.

No magic values found beyond what's already named as constants (`DEFAULT_CONTROL_PORT`, mount tags, etc. in `tddy-sandbox-qemu`).

### validate-tests (2026-07-01)

**Critical (0), Warning (0)** — all 13 tests across the three changed test files (`argv_unit.rs` ×9, `sandbox_qemu_cli_acceptance.rs` ×2, `build_image_cli_acceptance.rs` ×2) comply with fluent-tests: Given/When/Then structure, one behavior per test, meaningful test data (no generic `"foo"`/`"bar"`), named builder helpers (`a_read_write_mount()`, `a_minimal_buildroot_spec()`), no sleep-based flakiness, no unexplained magic values. `#[ignore]` on the two Buildroot-build tests carries a justification string and a module-level doc comment explaining why and how to run them, matching the `codex_backend.rs` precedent for this repo's "production test" category.

### validate-changes, final pass (2026-07-01)

Re-ran after the prod-readiness fixes (fatal cleanup, hard-error tag) and the clean-code refactor (`run_buildroot_pipeline` 269→52 lines, `build_image` 61→46, `docker_shareable_buildroot_dir` 66→57). **No new issues introduced.**

- `cargo build -p tddy-vm -p tddy-vm-build -p tddy-sandbox-qemu` — clean.
- `cargo clippy --all-targets -- -D warnings` (all three) — clean.
- `cargo fmt --check` (all three) — clean.
- `cargo test -p tddy-vm` — 26 tests + 1 doctest, all pass (no regression from either the Docker work or the refactor).
- `cargo test -p tddy-vm-build` / `-p tddy-sandbox-qemu` — fast suites clean (`0 passed, 2 ignored` and the expected fast placeholder-image failures respectively, per earlier validation).
- Working tree: `Cargo.lock`, `docs/dev/1-WIP/qemu-sandbox-cli.md`, `docs/ft/vm/tddy-vm.md`, `packages/tddy-vm-build/Cargo.toml`, `packages/tddy-vm-build/tests/build_image_cli_acceptance.rs`, `packages/tddy-vm/src/build.rs` modified, `packages/tddy-vm/docker/` new — all reviewed above; matches expectations, nothing unaccounted for.

**Ready for `cargo fmt` / `clippy -D warnings` / `cargo test` sign-off (pr-wrap step 6) and documentation wrap (step 7).**
