# 2026-04-03 — User prompt strip, footer row, Enter affordance

**Type:** Feature

`layout_chunks_with_inbox` allocates `footer_bar`, a separator row below the status bar, and narrowed prompt/footer widths for the Enter strip. `paint_user_prompt_activity_strip` draws Running follow-up text on the last activity line (white on dark grey). `enter_button_rect` is three columns with margin, from the first row below the status bar through prompt text lines and footer (horizontal rule row excluded); `paint_enter_affordance` uses light box-drawing glyphs and U+23CE on the first prompt text row; `TDDY_E2E_NO_ENTER_AFFORDANCE` skips overlay paint. `ViewState::last_select_click_option` supports double-click confirm in Select. Feature docs `docs/ft/coder/tui-status-bar.md`, `docs/ft/web/web-terminal.md`. Tests: layout, `mouse_map`, `render`; `tddy-e2e` `grpc_terminal_rpc`. (tddy-tui)
