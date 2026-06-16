# Changeset: tddy-build — Bazel-like build system

**Date:** 2026-06-16
**Branch:** `thoracic-nasturtium`
**Packages:** tddy-build (new), tddy-tools, tddy-core, tddy-coder
**Feature PRD:** [docs/ft/build/tddy-build.md](../../ft/build/tddy-build.md)

## Goal

Add `tddy-build`: a standalone, content-addressed, Bazel-inspired build system. Targets are first-class proto messages discovered from `BUILD.yaml`, executed by `tddy-tools build`, relay-capable via `TDDY_SOCKET`, and discoverable by the agent via `tddy-tools build-list`.

## Hard requirements

1. Every build target type has a corresponding proto message in `packages/tddy-build/proto/`.
2. `BUILD.yaml` deserializes directly into those proto types — no parallel serde struct layer.
3. `tddy-build` is independent from `tddy-coder` (no compile-time dep on it).
4. `tddy-tools` gains `build` + `build-list`, relay-capable via `TDDY_SOCKET`.
5. `tddy-coder` respects declared build targets (agent discovers them via the CLI).

## Decisions

- **All 7 target types execute** (rust_binary, rust_library, typescript, docker_image, script, tool, group). Hermetic coverage via command-construction + `--dry-run`; cargo/bun/docker execution behind availability gates.
- **Agent surfacing = CLI discovery** (`build-list`), matching session actions. No context injection; no `build` feature on tddy-core.
- **Relay via extension points**: `tddy-core` defines `BuildExecutor` trait + wire types + `ToolCallResponse` variants; `tddy-coder` owns the `tddy-build` dependency and registers the executor into the toolcall listener. `tddy-build` stays dependency-free.

## TODO

- [x] Create/update PRD documentation (`docs/ft/build/tddy-build.md`)
- [x] Create changeset (this file)
- [x] Write failing acceptance tests (red)
  - [x] `packages/tddy-build/tests/build_acceptance.rs` (10 tests)
  - [x] `packages/tddy-tools/tests/build_cli_acceptance.rs` (3 tests)
- [x] Green: proto serde wiring (oneof internal tag, enum string↔i32 helpers, per-message `default`/`deny_unknown_fields`)
- [x] Green: `discovery`, `graph` (topo sort + cycle detection + waves), `lower` (7 types), `cache` (SHA-256 + atomic write), `executor` (wave-based parallel)
- [x] Green: `tddy-build::service` (shared JSON entry points for CLI + relay)
- [x] Green: `tddy-tools` `build`/`build-list` subcommands + relay client
- [x] Green: `tddy-core` `BuildExecutor` trait + registry + wire types + listener handlers
- [x] Green: `tddy-coder` executor impl + registration (`build_executor::register`)
- [ ] Write failing unit/integration tests — manifest, cache, graph, lower, tddy-core toolcall (deferred; acceptance suite covers behavior)
- [ ] Update changelog / docs

## Acceptance tests (red — all failing)

`packages/tddy-build/tests/build_acceptance.rs`

1. `build_manifest_yaml_round_trips_all_target_types`
2. `build_manifest_rejects_unknown_fields`
3. `cache_key_is_deterministic`
4. `cache_hit_skips_execution`
5. `cache_miss_on_input_mtime_change`
6. `build_executes_script_target`
7. `build_respects_tool_target_bin_dir`
8. `cycle_detection_returns_error`
9. `build_action_dag_parallel_wave_ordering`
10. `dry_run_emits_argv_for_all_seven_target_types`

`packages/tddy-tools/tests/build_cli_acceptance.rs`

11. `build_list_outputs_json_with_all_targets`
12. `build_cli_executes_script_target`
13. `build_cli_dry_run_prints_plan_without_executing`

## Affected files

- `packages/tddy-build/` — new crate (proto, build.rs, src/{manifest,discovery,graph,lower,cache,executor,serde_helpers,error}.rs)
- `Cargo.toml` — add `packages/tddy-build` to workspace members
- `test` — add `-p tddy-build` to the build line
- `packages/tddy-tools/src/{main.rs,cli.rs}` — `Build`/`BuildList` subcommands + relay (green)
- `packages/tddy-core/src/toolcall/{mod.rs,listener.rs,build.rs}` — extension trait + wire types + handlers (green)
- `packages/tddy-coder/` — `BuildExecutor` impl + registration (green)

## Verification results

- `tddy-build` tests: **10/10** acceptance pass (`build_acceptance.rs`).
- `tddy-tools` CLI: **3/3** pass (`build_cli_acceptance.rs`).
- `tddy-core` `session_actions_acceptance`: **7/7** pass (validates the test rename + portable `true` fixture).
- `cargo clippy -D warnings`: clean on `tddy-build`, `tddy-tools` (bins), `tddy-coder` (bins), `tddy-core` (lib). `cargo fmt`: changeset files clean.

Incidental pre-existing fixes on this branch (from PR #206) that were blocking `./test` on macOS, unrelated to tddy-build:
- Removed an accidental duplicate `#[cfg(test)] mod tests` in `packages/tddy-tools/src/pty_relay.rs` (it called the livekit-gated `encode_resize()`, breaking non-livekit `cargo test`).
- `packages/tddy-core/tests/session_actions_red.rs` → `session_actions_acceptance.rs`: dropped red/green-phase wording; fixture command `/bin/true` → portable `true` (macOS lacks `/bin/true`).

Out of scope / still red in the broader workspace (pre-existing, environment-dependent, not introduced here): `codex_acp_backend` (needs the codex-acp binary), `virtual_tui_start_bugfix` (e2e), `remote_list_tools` (needs a relay daemon), and unused-import clippy warnings in `remote_ctx_wiring_acceptance` / `remote_cli_subcommand_acceptance`.

## Risk

YAML→prost serde (Req #2) is novel in this repo — no existing crate derives serde on prost types. The red round-trip test pins the contract; green must add internal `#[serde(tag="type")]` on the `BuildTarget.config` oneof, string↔i32 enum `deserialize_with` helpers, and per-message `default`/`deny_unknown_fields`.
