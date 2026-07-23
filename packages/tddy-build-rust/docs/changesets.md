# Changesets Applied

Wrapped changeset history for tddy-build-rust.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-23** [Feature] **bsp-build-server — mode-aware lowering** — `RustPlugin::lower_mode` emits `cargo test`/`cargo run` actions for `BuildMode::Test`/`Run` (Compile stays `cargo build`); unsupported modes are rejected. Cross-package [docs/dev/changesets.md](../../../docs/dev/changesets.md). Feature [bsp-build-server.md](../../../docs/ft/coder/bsp-build-server.md). (tddy-build-rust)
- **2026-06-20** [Feature] **tddy-build-rust — BUILD.yaml config** — `packages/tddy-build-rust/BUILD.yaml` declares `tddy-build-rust:lib` with `srcs` glob and dep on `tddy-build:lib`. (tddy-build-rust)
- **2026-06-20** [Feature] **tddy-build-rust — plugin inputs/outputs + real workspace example** — plugin now emits `srcs`+`outputs`+`working_dir` on lowered actions so the content-addressed cache invalidates on source edits; ships `examples/workspace/` (interdependent multi-package cargo fixture) with integration tests covering deps-first ordering, real `cargo build` (tool-gated), cache hit/miss, and circular-reference detection. (tddy-build-rust)
- **2026-06-16** [Feature] **tddy-build-rust — new plugin crate** — extracted from `tddy-build` plugin architecture refactor; lowers `rust_binary`/`rust_library` targets to `cargo build -p <pkg>` with `--bin`, `--features`, `--release`, `--target` flags; `deny_unknown_fields` config structs. Feature: [docs/ft/build/tddy-build.md](../../../docs/ft/build/tddy-build.md). (tddy-build-rust)
