# Changeset: sandbox-builder — explicit cross-platform SandboxBuilder + strict reads

**Date:** 2026-06-28
**Branch:** `sandbox-builder`
**Packages:** `tddy-sandbox`, `tddy-sandbox-darwin`, `tddy-sandbox-cgroups`, `tddy-sandbox-runner`, `tddy-daemon`, `tddy-sandbox-app`
**Feature PRD:** [docs/ft/coder/sandbox-builder.md](../../ft/coder/sandbox-builder.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Add `SandboxBuilder` + `SandboxPlan` + typed sub-specs (`ReadSpec`/`CopySpec`/`SymlinkSpec`/`EnvSpec`/`SecretSpec`/`PolicySpec`/`NetworkSpec`/`ResourceLimits`) — `packages/tddy-sandbox/src/builder.rs`
- [x] Add `materialize` helpers (copies/symlinks/secrets) — `packages/tddy-sandbox/src/materialize.rs`
- [x] Move `build_sandbox_runner_env` → `tddy_sandbox::default_runner_env` (daemon fn now delegates)
- [x] Add explicit Claude recipe (`claude_required_reads`/`claude_required_copies`/`claude_policy`) — `packages/tddy-sandbox/src/claude_spawn.rs`
- [x] Strict reads: `render_plan` emits an explicit read allow-list with **no** `(allow file-read*)` wildcard (built inline). The wildcard template `sandbox-claude.sb.tmpl` and the `render_profile`/old `spawn`/`seed_claude_home_config` shims were **deleted**; all tests migrated onto the plan-based API (`render_plan`/`spawn_plan`/`spawn_sandbox_runner`)
- [x] `render_profile` → `render_plan(&SandboxPlan)` (explicit reads/exec/policy/network) — `packages/tddy-sandbox-darwin/src/profile.rs`
- [x] `spawn` → `spawn_plan` + materialize copies/symlinks/secrets — `packages/tddy-sandbox-darwin/src/spawn.rs`
- [x] Keep detectors as opt-in pure helpers (`detect_toolchain_reads`/`binary_exec_reads`/`system_baseline_reads`) in `claude_spawn.rs`
- [x] Linux `spawn_plan`: `plan_to_bind_mounts` + RO bind-mounts in child; map `plan.limits` → `CgroupLimits` — `packages/tddy-sandbox-cgroups/src/lib.rs` (⚠️ not compiled/run on this macOS host — Linux-gated)
- [x] Runner: read `TDDY_SECRET_*`, set on Claude PTY child only, unlink — `packages/tddy-sandbox-runner/src/runner.rs`
- [x] Dispatcher `spawn_sandbox_runner` → `build_sandbox_plan` + `spawn_plan` cfg-dispatch — `packages/tddy-daemon/src/sandbox_session.rs`
- [x] Migrate call sites — `packages/tddy-sandbox-app/src/spawn.rs`, `packages/tddy-daemon/src/connection_service.rs` (StartSession + resume); dropped `seed_claude_home_config` calls (credentials now copied via the plan)
- [x] Add `pivot_root` follow-up note — `docs/dev/TODO.md`

## Acceptance tests

- [x] `packages/tddy-sandbox-darwin/tests/seatbelt_confinement_acceptance.rs` — `a_strict_profile_still_lets_the_claude_binary_report_its_version`, `a_strict_profile_denies_reading_a_path_not_on_the_allow_list` (PASS)
- [x] `packages/tddy-sandbox-darwin/tests/secret_channel.rs` — `the_oauth_secret_is_passed_to_the_claude_child_and_never_appears_in_the_sandbox_exec_argv` (PASS; argv-level)
- [x] `packages/tddy-sandbox-runner/tests/secret_envs.rs` — `resolves_a_secret_file_into_the_real_env_var_and_unlinks_it` (PASS)

## Unit tests

- [x] `packages/tddy-sandbox/src/builder.rs` — builds-only-declared-reads, dedup, shadowing, copy/symlink validation, secret-not-in-env (PASS)
- [x] `packages/tddy-sandbox/src/claude_spawn.rs` — dyld-root read present, copies seed only credentials (PASS)
- [x] `packages/tddy-sandbox-darwin/src/profile.rs` — omits wildcard, emits declared reads, dyld-root literal, oauth loopback inbound (PASS)
- [~] `packages/tddy-sandbox-cgroups/src/lib.rs` (`#[cfg(linux)]`) — read→RO bind-mount, noexec flag, limits mapping (implemented; Linux-gated, not run on this macOS host)

## Validation Results

### pr-wrap (2026-06-28)

**Risk summary:** Critical 0 · Warning 1 · Info 2

- **validate-changes:** No `unwrap`/`expect`/`panic!` and no undocumented `unsafe` in new production code. The cgroups `unsafe` (namespace + RO bind-mount in `pre_exec`) carries a `SAFETY` comment. Secret handling is sound — values written `0600` under `scratch/.secrets/`, unlinked after the runner reads them, set on the inner Claude child only, and never logged or placed in the `sandbox-exec` argv (only the `TDDY_SECRET_*` file path is). No hardcoded secrets, no test-only branches, no fallbacks.
- **validate-prod-ready:** No leftover `todo!()`/`unimplemented!()`/`dbg!`/mocks. The only `FIXME`s are the tracked `pivot_root` follow-up; `placeholder` hits are benign doc comments.
- **validate-tests:** New tests are fluent Given/When/Then with real assertions (builder, recipe, render, secret-channel, secret-envs, strict seatbelt boot/deny). `sandbox_runner_inspect` remains a no-assertion **diagnostic** (pre-existing pattern), now migrated onto the plan API.
- **clean-code / gate:** `cargo fmt --check` clean; `cargo clippy --tests -- -D warnings` clean on all changed crates (incl. fixing four pre-existing daemon test lints); sandbox/darwin/runner test suites all green.

- **[WARNING] dead `spawn` in `tddy-sandbox-cgroups/src/lib.rs`** — the old spec-based `spawn` is now unused by production (the daemon dispatches `spawn_plan`) but still referenced by the crate's own Linux-gated tests. Left in place because the crate cannot be compiled/verified on this macOS host; tracked as a follow-up to remove alongside the Linux verification pass (mirrors the darwin old-`spawn` deletion already done).
- **[INFO]** The entire `tddy-sandbox-cgroups` backend is unverified on this host (no Linux target in the Nix shell) — code reviewed by inspection only.
- **[INFO]** End-to-end app run (real interactive `claude` under the strict builder, OAuth via secret channel) not yet performed.

## Delta summary

### `tddy-sandbox`

**New files:**
- `src/builder.rs` — `SandboxBuilder` (pure `build()`), `SandboxPlan`, and typed sub-specs.
- `src/materialize.rs` — host-side copy/symlink/secret materialization helpers shared by both backends.

**Modified files:**
- `src/lib.rs` — module + re-exports.
- `src/claude_spawn.rs` — explicit `claude_required_reads`/`claude_required_copies`/`claude_policy` recipe; `default_runner_env`.

### `tddy-sandbox-darwin`

**Deleted files:**
- `profiles/sandbox-claude.sb.tmpl` — the wildcard-bearing template, removed entirely (the empty `profiles/` dir went with it).

**Modified files:**
- `src/profile.rs` — `render_plan(&SandboxPlan)` builds the strict SBPL inline (explicit reads/exec/policy/network, **no `(allow file-read*)`**); old `render_profile` deleted; detectors kept as opt-in helpers.
- `src/spawn.rs` — `spawn_plan` + materialize copies/symlinks/secrets.
- `docs/troubleshooting.md` — update the wildcard note for strict reads.

### `tddy-sandbox-cgroups`

**Modified files:**
- `src/lib.rs` — `spawn_plan`; `plan_to_bind_mounts`; RO bind-mounts of declared reads in `enter_rootless_jail`; `plan.limits` → `CgroupLimits`.

### `tddy-sandbox-runner`

**Modified files:**
- `src/runner.rs` — read `TDDY_SECRET_*` files, set real env var on the Claude PTY child only, unlink.

### `tddy-daemon`

**Modified files:**
- `src/sandbox_session.rs` — `spawn_plan(plan)` dispatcher; `default_runner_env`/`seed_*` shims.
- `src/connection_service.rs` — StartSession + resume build the plan via `SandboxBuilder` + Claude recipe.

### `tddy-sandbox-app`

**Modified files:**
- `src/spawn.rs` — build the plan via `SandboxBuilder` + Claude recipe; declare the OAuth secret.

## Addendum — interactive hardening + repo mount (post-strict e2e)

Surfaced by running the real interactive session under the strict jail:

- **`/dev/*` device nodes need read grants** — shells/tools open `/dev/null` (and `/dev/zero`,
  `/dev/random`, `/dev/urandom`, `/dev/tty*`, `/dev/fd/*`, std{in,out,err}) `O_RDWR`; the strict
  read list only had the write/ioctl allows, so shell init failed. Added to `system_baseline_reads`
  + regression test `a_strict_profile_lets_a_shell_read_dev_null`.
- **Claude runtime tmpdir** — Claude keeps `/tmp/claude-$UID` regardless of `TMPDIR`, which is
  outside the write allow-list. `default_runner_env` now sets `CLAUDE_CODE_TMPDIR`/`CLAUDE_TMPDIR`
  to the scratch tmp (verified the override relocates the dir).
- **Writable repo mount + `--cwd`** (new capability): `MountSpec { host, jail, writable }` on the
  builder makes a host dir available in the jail — macOS grants read/write(+exec) at the real path;
  Linux bind-mounts it (rw when writable). The runner takes `--cwd` for Claude's start dir. The app
  mounts `--repo` read-write and starts Claude there (default cwd = repo), with a `--cwd` override.
  Daemon remote-codebase sessions pass no mounts (unchanged). Tests: builder mount/validation,
  render read+write rules, cgroups rw bind-mount mapping.

## Follow-ups

- Full minimal RO-root `pivot_root` filesystem confinement on Linux (this changeset lands RO
  bind-mounts of the declared reads only).
- Config-driven cgroup limits surface.
- Tool routing: the sandboxed Claude can use native tools (which see the in-jail tree) vs the
  `tddy-tools` MCP tools (host worktree); revisit the recipe's allow/deny so the intent is explicit.
