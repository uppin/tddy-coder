# 2026-04-03 — TUI Stop pane

**Type:** Feature

`right_chrome_reserve_cols`, **`stop_pane`** in **`LayoutAreas`**, **`stop_button_rect`**, **`paint_stop_affordance`** (red ■); **`handle_mouse_event`** → **`UserIntent::Interrupt`**; **`event_loop`** / **`virtual_tui`** call **`ctrl_c_interrupt_session`**; narrow width omits Stop. **`TDDY_E2E_NO_ENTER_AFFORDANCE`** hides Enter and Stop. Feature docs **`docs/ft/coder/tui-status-bar.md`**, **`docs/ft/web/web-terminal.md`**. (tddy-tui)
