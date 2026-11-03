# 2026-11-03 — Cursor CLI from cursor.yaml

**Type:** Feature

`#agent-tools` 7/12 ([#796](https://github.com/uppin/tddy-coder/pull/796)).

- `specs/cursor.yaml` is embedded and listed in `builtin_specs()`.
- `McpSlot::Placed { path, format }` (`{home}` resolved against `SlotInputs.home`, else `SlotError::UnboundHome`) and `McpFormat::CursorMcpJson`.
- `InstructionsSlot.rules_file`: the preamble written whole to the named file.
- `cursor-hooks-json` (`slots::cursor::CursorHooksJson`): `.cursor/hooks.json` over a daemon socket; params without one are `SlotError::NoDaemonSocket` through `HooksAdapter::try_render`.

See [coding-agent-specs.md](../coding-agent-specs.md).
