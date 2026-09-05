# 2026-03-18 — Terminal Resize Support

**Type:** Bug Fix

Event::Resize handling in event_loop with terminal.clear(); apply_resize in virtual_tui (resize, clear, prev_frame.clear). Unit tests: apply_resize_clears_prev_frame, resize_and_clear_then_draw_produces_correct_frame_area. (tddy-tui)
