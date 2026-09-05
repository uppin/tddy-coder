# 2026-03-21 — Interactive backend selection

- **CLI**: `--agent` is optional. When omitted, choose backend via TUI dropdown or plain stdin menu before the workflow; when set, behavior matches the previous default path.
- **Defaults**: Cursor uses `composer-2`; `--model` overrides per-backend defaults and is passed to `cursor agent` as `--model`.
- **tddy-demo**: Still defaults to stub when `--agent` is omitted (no interactive menu).
- **Product reference**: [Coder overview — Backend selection](../1-OVERVIEW.md#backend-selection-at-session-start).
