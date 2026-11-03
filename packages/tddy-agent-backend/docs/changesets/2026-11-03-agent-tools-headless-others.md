# 2026-11-03 — Headless Cursor, Codex and Oz from their specs

**Type:** Feature

`#agent-tools` 9/12 ([#798](https://github.com/uppin/tddy-coder/pull/798)).

- Adapters `cursor-stream`, `codex-jsonl`, `oz-ndjson` (stream) and `cursor-models` (models), wrapping today's parsers.
- `CursorBackend` / `CodexBackend` `with_headless_renderer` and `invoke_from_spec`; `HeadlessDriver` renders Cursor, Codex and Oz from a renderer.
- Code issues: new `oversized-file-codex`; `oversized-file-cursor` and `oversized-file-headless-driver` regressed (restructuring deferred to the sweep node).

See [architecture.md](../architecture.md#adapters-adapters).
