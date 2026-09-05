# 2026-06-16 — **tddy-build-rust

**Type:** Feature

new plugin crate** — extracted from `tddy-build` plugin architecture refactor; lowers `rust_binary`/`rust_library` targets to `cargo build -p <pkg>` with `--bin`, `--features`, `--release`, `--target` flags; `deny_unknown_fields` config structs. Feature: [docs/ft/build/tddy-build.md](../../../../docs/ft/build/tddy-build.md). (tddy-build-rust)
