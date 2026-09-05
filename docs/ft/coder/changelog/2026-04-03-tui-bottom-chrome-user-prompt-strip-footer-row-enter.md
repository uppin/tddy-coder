# 2026-04-03 — TUI bottom chrome: user prompt strip, footer row, Enter affordance

- **Layout**: **`layout_chunks_with_inbox`** allocates a **footer** row below the prompt block, a **separator row** below the status bar, and **`LayoutAreas`** includes **`footer_bar`** and **`enter_pane`**.
- **Activity (Running)**: The last line of the activity log shows **`> {running_input}`** with **white** foreground on **dark grey** background when follow-up text is non-empty.
- **Mouse**: **`enter_button_rect`** is **three** columns wide to the right of the prompt text (with margin); height spans from the first row **below the status bar** through prompt **text** lines and the **footer** (the bottom prompt **rule** row is excluded when present). **`paint_enter_affordance`** draws a light box frame with **U+23CE** on the first prompt text row. **`TDDY_E2E_NO_ENTER_AFFORDANCE`** skips overlay paint for stable byte-level tests. **`ViewState::last_select_click_option`** supports double-click to confirm in **Select** mode.
- **Tests**: **`packages/tddy-tui`** layout, rect geometry, strip styling, and env-gate tests; **`tddy-e2e`** **`grpc_terminal_rpc`** where terminal streaming uses the env gate.
- **Docs**: [tui-status-bar.md](../tui-status-bar.md); [web-terminal.md](../../web/web-terminal.md#connected-terminal-ux); **`packages/tddy-tui/docs/changesets.md`**.
