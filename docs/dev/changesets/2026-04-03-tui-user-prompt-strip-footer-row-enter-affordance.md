# 2026-04-03 — TUI user prompt strip, footer row, Enter affordance

**Type:** Feature

**`tddy-tui`**: **`layout_chunks_with_inbox`** footer row and **separator row** below status; **`LayoutAreas::footer_bar`**, **`enter_pane`**; **`paint_user_prompt_activity_strip`** (Running, white on dark grey); **`enter_button_rect`** three columns right of prompt (margin), height from below status through prompt text + footer (rule row excluded); **`paint_enter_affordance`** light box frame, **U+23CE** on first prompt text row; **`TDDY_E2E_NO_ENTER_AFFORDANCE`** overlay skip; **`ViewState::last_select_click_option`** (Select double-click). Feature docs **`docs/ft/coder/tui-status-bar.md`**, **`docs/ft/web/web-terminal.md`**; changelogs **`docs/ft/coder/changelog.md`**, **`docs/ft/web/changelog.md`**; package **`packages/tddy-tui/docs/architecture.md`**, **`packages/tddy-tui/docs/changesets.md`**. (tddy-tui)
