# 2026-03-09 — TUI E2E Testing & Clarification Question Fix

**Type:** Feature

layout.rs: question_height(mode) for Select/MultiSelect/TextInput. render.rs: render_question (header, options, selection cursor, Other, MultiSelect checkboxes), dynamic area (question_height.max(inbox_h)) reuses inbox slot. Prompt bar shows hints and text input for question modes. Clarification questions now visible in TUI. (tddy-tui)
