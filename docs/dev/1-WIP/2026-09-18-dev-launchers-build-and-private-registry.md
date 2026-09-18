# 2026-09-18 — `./web-dev` and `./desktop-dev` take `--build` and `--resolve-private-repo`

**Type:** Feature

Both dev launchers gained the same two opt-in preparation steps, consumed by the script itself and
never forwarded to the process it launches (`tddy-daemon` for `./web-dev`, `tauri dev` for
`./desktop-dev` — neither accepts them).

## `--build` — build what the running stack can *invoke*

Each launcher previously built only the one process it starts: `./web-dev` built `tddy-daemon`
(`BUILD_TARGETS`), and `./desktop-dev` built nothing at all, since `tauri dev` owns the
`tddy-desktop` build. But the daemon — hosted in its own process by the desktop app — resolves six
further binaries **at runtime**, as siblings of `current_exe()` and then as a bare name on `PATH`,
**with no existence check anywhere**
([spawn.rs](../../../packages/tddy-daemon/src/index_daemon/spawn.rs),
[sandbox_session.rs](../../../packages/tddy-daemon-sandbox/src/sandbox_session.rs),
[project_provision.rs](../../../packages/tddy-projects/src/project_provision.rs)). A missing or stale
sibling in `target/debug` is therefore never a build error: it surfaces much later as a session that
will not start, or as a dev run silently driving whatever `./install` last put on `PATH`.

The list lives in one place, [scripts/dev-runtime-binaries.sh](../../../scripts/dev-runtime-binaries.sh)
(`DEV_RUNTIME_PACKAGES` plus a `dev_runtime_cargo_args` helper), sourced by both launchers:
`tddy-coder`, `tddy-tools`, `tddy-sandbox-runner`, `tddy-index-daemon`, `tddy-remote-git-repo`,
`tddy-session-sync` — exactly `install`'s `DESKTOP_BINARIES`, which is the production statement of
"these must sit beside the daemon". `tddy-daemon` and `tddy-supervisor` are deliberately absent (a
launcher names its own backend), as is `tddy-sandbox-app` (started by hand, not invoked by the
stack). Building the whole workspace instead is not the alternative: that is the tens-of-minutes
local run [AGENTS.md](../../../AGENTS.md) forbids.

## `--resolve-private-repo` — resolve JS deps against the private registry

Delegates to the repo's existing two-step `bun run local-registry-install` (lock resolution via
`scripts/resolve-local-lock.ts`, then the registry-pinned `scripts/local-bun-install.sh`), so neither
launcher reimplements it and neither can fall back to a public-registry `bun install`.
`LOCAL_REGISTRY_URL` overrides the registry. Opt-in rather than automatic because it rewrites
`node_modules` and the lock resolution for the whole workspace.

Both scripts run this before anything is started, so a failure leaves no half-started stack behind
and the Vite readiness wait is not charged for it.

## Tests

Static contract checks in [`tddy_e2e::dev_script_contract`](../../../packages/tddy-e2e/src/dev_script_contract.rs),
driven by three integration suites: `tests/web_dev_script.rs`, `tests/desktop_dev_script.rs` and
`tests/dev_runtime_binaries.rs`. They pin that each flag is documented in the usage header, matched
as the script's own argument rather than forwarded, that `--build` sources the shared list instead of
hardcoding a package set, that `--resolve-private-repo` goes through `local-registry-install` and
that no launcher runs a bare `bun install` — and that the shared list still covers every binary
`install` ships as a daemon sibling, which is what keeps the dev and production lists from drifting.
