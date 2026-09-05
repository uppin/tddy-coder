# 2026-03-09 — TUI E2E Testing & Clarification Question Fix

- **tddy-e2e package**: New workspace member for E2E tests. gRPC-driven tests (grpc_clarification, grpc_full_workflow) and PTY test (pty_clarification with termwright, run with `--ignored`).
- **Clarification question rendering**: TUI now displays clarification questions. layout.rs: question_height() for Select/MultiSelect/TextInput. render.rs: render_question (header, options, selection cursor, Other, MultiSelect checkboxes). Dynamic area reuses inbox slot when in question modes.
- **Prompt bar**: Shows "Up/Down navigate Enter select" for Select, "Up/Down navigate Space toggle Enter submit" for MultiSelect, and text input prompt for TextInput/Other modes.
- **Bug fix**: Clarification questions were never visible; root cause was empty prompt bar and missing question widget. Now fully rendered and interactable.
