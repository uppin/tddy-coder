# Changeset: tddy-build — plugin architecture (vacate target-specific knowledge)

**Date:** 2026-06-16
**Branch:** `pinnate-fennel`
**Packages:** tddy-build, tddy-build-rust (new), tddy-build-typescript (new), tddy-build-docker (new), tddy-tools, tddy-coder
**Feature PRD:** [docs/ft/build/tddy-build.md](../../ft/build/tddy-build.md)

## Goal

Remove all build-target-specific (Rust/TypeScript/Docker) knowledge from the
`tddy-build` engine. The engine becomes a generic, content-addressed build runner
plus a wiring point for a `BuildPlugin` trait. Each language/ecosystem "recipe" lives
in its own crate (`tddy-build-rust`, `tddy-build-typescript`, `tddy-build-docker`) and
lowers its own typed config into `BuildAction`s. The engine retains only its
structural vocabulary — `script`, `tool`, `group` — as built-ins.

## Hard requirements

1. `tddy-build` contains **no** `cargo`/`rustc`/`bun`/`docker` literals and no
   knowledge of `rust_binary`/`rust_library`/`typescript`/`docker_image` types.
2. `tddy-build` keeps **zero `tddy-*` dependencies** (it must not depend on the plugin
   crates).
3. The `BUILD.yaml` authoring format is unchanged for existing target types
   (`config: { type: <name>, …fields }`).
4. A target whose `type` has no registered plugin (and is not a built-in) fails with a
   clear `unknown target type: <name>` error.
5. Plugins are wired explicitly via a `PluginRegistry` passed into the engine API; no
   process-global plugin state.

## Decisions

- **Scope = recipes only.** `rust`/`typescript`/`docker` → plugin crates.
  `script`/`tool`/`group` stay built into `tddy-build` because they carry special
  graph/executor semantics (group → build-order edges; tool → `PATH` `bin_dir`; script
  → the generic command escape hatch).
- **One crate per plugin** — `tddy-build-rust`, `tddy-build-typescript`,
  `tddy-build-docker`. Each depends only on `tddy-build` + `serde`/`serde_yaml`.
- **Explicit registry.** `tddy-build` defines `BuildPlugin` + `LowerContext` +
  `PluginRegistry`. Engine entry points (`service::build_json` / `build_list_json`,
  `graph` lowering, `executor`) take `&PluginRegistry`. Binaries assemble the registry
  at their wiring points (`tddy-tools/src/build_cli.rs`, `tddy-coder/src/build_executor.rs`).
- **Open manifest schema.** `BuildManifest`/`BuildTarget` move from prost-generated
  types to plain serde structs; `BuildTarget.config` becomes
  `Option<{ type: String, #[serde(flatten)] fields: serde_yaml::Value }>`. This retires
  the prior "BUILD.yaml deserializes directly into prost types" requirement — that
  closed `oneof` was the coupling. `BuildAction` (+ `FileSet`/`OutputDecl`/cache types)
  stay proto: they are the stable engine↔plugin contract and feed the cache key.

## TODO

- [x] Create/update PRD documentation (`docs/ft/build/tddy-build.md`)
- [x] Create changeset (this file)
- [x] Write failing acceptance tests (red)
  - [x] `packages/tddy-build/tests/build_acceptance.rs` (rewrite: built-ins + inline fake plugin)
  - [ ] `packages/tddy-tools/tests/build_cli_acceptance.rs` — unchanged; green guard for the unchanged external contract (rust_binary via the wired plugin)
- [x] Write failing unit tests (red)
  - [x] `tddy-build` — `tests/plugin_registry.rs` (registry semantics + `LowerContext`)
  - [x] `tddy-build` — `tests/manifest_open_config.rs` (open config schema)
  - [x] `tddy-build-rust` / `-typescript` / `-docker` — per-recipe argv (ported from
        the former `lower.rs::tests`, now traveling with the moved code)
- [x] Green: `BuildPlugin` trait + `PluginRegistry` (`plugin.rs`); `builtin.rs`
- [x] Green: serde `manifest.rs`; thread `&PluginRegistry` through `lower`/`graph`/`executor`/`service` (`build_list_json` stays registry-free — listing reads the `type` tag only)
- [x] Green: trim proto (`targets.proto` + `manifest.proto` removed; `build.rs` compiles `actions.proto` + `cache.proto`)
- [x] Green: three plugin crates + workspace member registration
- [x] Green: wire registry in `tddy-tools`/`tddy-coder`
- [ ] Update changelog / architecture docs (at wrap)

## Acceptance tests (red — all failing to compile against current code)

`packages/tddy-build/tests/build_acceptance.rs` — new plugin-contract tests:

1. `manifest_round_trips_builtin_and_plugin_configs` — open config; `type` tag + verbatim plugin fields
2. `registered_plugin_lowers_target_actions` — engine dispatches to the registered plugin
3. `unknown_target_type_without_plugin_errors` — `unknown target type: demo`
4. `builtin_script_tool_group_lower_without_any_plugin` — built-ins work with an empty registry
5. `group_membership_drives_build_order_without_plugins`
6. `tool_bin_dir_is_prepended_to_action_path`
7. `service_list_reports_raw_type_string_for_plugin_target`

Retained engine tests in the same file (adapted to the new `&PluginRegistry` /
serde-manifest signatures, so they too fail to compile until green):
`build_manifest_rejects_unknown_fields`, `cache_key_is_deterministic`,
`cache_hit_skips_execution`, `build_executes_script_target`,
`cycle_detection_returns_error`, `build_action_dag_parallel_wave_ordering`.

`packages/tddy-tools/tests/build_cli_acceptance.rs` — **unchanged green guards** (the
external CLI contract is identical; they must keep passing through the refactor once
the rust plugin is wired): `build_list_outputs_json_with_all_targets`,
`build_cli_executes_script_target`, `build_cli_dry_run_prints_plan_without_executing`.

## Affected files

- `packages/tddy-build/src/` — NEW `plugin.rs`, `builtin.rs`; CHANGED `manifest.rs`
  (serde structs), `lower.rs`, `graph.rs`, `executor.rs`, `service.rs`, `lib.rs`, `build.rs`
- `packages/tddy-build/proto/tddy/build/v1/` — delete `targets.proto`; trim `manifest.proto`
- `packages/tddy-build-rust/`, `-typescript/`, `-docker/` — new crates
- `Cargo.toml` — add the three crates to workspace members
- `test` — add `-p tddy-build-rust …` to the build line
- `packages/tddy-tools/src/build_cli.rs` (+ `Cargo.toml`) — assemble & pass registry
- `packages/tddy-coder/src/build_executor.rs` (+ `Cargo.toml`) — assemble & pass registry

## Verification results

- `tddy-build`: **47/47** (24 lib + 14 `build_acceptance` + 5 `plugin_registry` + 4 `manifest_open_config`).
- `tddy-build-rust` **4/4**, `tddy-build-typescript` **2/2**, `tddy-build-docker` **2/2**.
- `tddy-tools` `build_cli_acceptance` (green guard) **3/3**; full `tddy-tools` suite green except two
  **pre-existing macOS environment failures** in `session_action_pipeline_integration`
  (`echo -n` builtin behavior; missing executable) — unrelated to this change, those files are untouched.
- `cargo clippy -D warnings` clean on `tddy-build`, the three plugin crates, and `tddy-tools` bins;
  `tddy-coder` + `tddy-tools` bins build clean.
- Hard req #1 confirmed: no `cargo`/`rustc`/`bun`/`docker`/recipe-type literals remain in
  `packages/tddy-build/src` outside doc comments. Hard req #2: `tddy-build/Cargo.toml` keeps zero `tddy-*` deps.

## Risk

The manifest layer moves off prost (open config). `BuildAction` remains proto, so the
cache-key contract is unchanged. The red round-trip + dispatch tests pin the new
contract: built-ins lower with an empty registry; plugin types dispatch by `type` name;
unknown types error.
