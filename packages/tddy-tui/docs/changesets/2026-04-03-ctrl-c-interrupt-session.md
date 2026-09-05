# 2026-04-03 — `ctrl_c_interrupt_session`

**Type:** Bug Fix

only **`kill_child_process`**; does **not** set workflow **`shutdown`** (Stop / TUI Ctrl+C / ETX no longer end the full TUI/presenter loop). **`process_virtual_tui_input_chunk`** drops unused **`shutdown`** parameter. Contract tests in **`ctrl_interrupt`**. (tddy-tui)
