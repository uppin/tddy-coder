# 2026-04-03 — TUI Stop pane (interrupt beside Enter)

- **tddy-tui**: **`right_chrome_reserve_cols`** reserves **four** or **eight** columns for Enter-only vs Enter+Stop; **`stop_button_rect`** / **`paint_stop_affordance`** (red **U+25A0**); left-click → **`UserIntent::Interrupt`** → **`ctrl_c_interrupt_session`** (not sent to presenter). **`VirtualTui`** and local **`event_loop`** handle **`Interrupt`** like **Ctrl+C**.
- **tddy-tui**: **`ctrl_c_interrupt_session`** only kills the tracked backend child (**`kill_child_process`**); it no longer sets the workflow **`shutdown`** flag, so Stop / TUI **Ctrl+C** / byte **0x03** do not exit the full TUI run (SIGINT via **`ctrlc`** still tears down the runner).
- **tddy-core**: **`UserIntent::Interrupt`** (presenter no-op for exhaustiveness).
- **Docs**: [tui-status-bar.md](../tui-status-bar.md) (**Mouse mode: Stop control**).
