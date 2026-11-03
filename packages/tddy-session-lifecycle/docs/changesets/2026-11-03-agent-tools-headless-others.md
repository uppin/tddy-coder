# 2026-11-03 — Headless Cursor, Codex and Oz from their specs

**Type:** Feature

`#agent-tools` 9/12 ([#798](https://github.com/uppin/tddy-coder/pull/798)).

- `coding_agent_launcher` plans every headless harness through `headless_plan_from_spec` and records `RenderedSlots.written`; without coding agents, a headless launch is refused with the load error.
- Code issue `oversized-file-coding-agent-launcher` regressed 967 → 1036 (restructuring deferred to the sweep node).
