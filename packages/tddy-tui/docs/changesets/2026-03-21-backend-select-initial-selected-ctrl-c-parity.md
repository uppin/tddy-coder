# 2026-03-21 — Backend Select `initial_selected`; Ctrl+C parity

**Type:** Feature

`view_state` / `render` / `layout` / `key_map` / `mouse_map` handle `Select` with `initial_selected`. `ctrl_interrupt` module: `key_is_ctrl_c_press`, `ctrl_c_interrupt_session` shared by `event_loop` and `virtual_tui` (RPC ETX 0x03 kills tracked child like local TUI). Integration test `virtual_tui_ctrl_c_kills_child` (unix). (tddy-tui)
