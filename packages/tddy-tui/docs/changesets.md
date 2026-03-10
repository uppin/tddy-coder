# Changesets Applied

Wrapped changeset history for tddy-tui.

- **2026-03-10** [Feature] Plan Approval Gate — view_state: plan_review_selected, markdown_scroll_offset; key_map: plan_review_key, markdown_viewer_key; render: render_plan_review, MarkdownViewer with tui-markdown; layout: question_height for PlanReview. tui-markdown 0.3, ratatui 0.30. (tddy-tui)
- **2026-03-09** [Feature] TUI E2E Testing & Clarification Question Fix — layout.rs: question_height(mode) for Select/MultiSelect/TextInput. render.rs: render_question (header, options, selection cursor, Other, MultiSelect checkboxes), dynamic area (question_height.max(inbox_h)) reuses inbox slot. Prompt bar shows hints and text input for question modes. Clarification questions now visible in TUI. (tddy-tui)
- **2026-03-09** [Feature] gRPC Remote Control — run_event_loop accepts optional external_intents and debug flag. Drains external intents via try_recv; passes to presenter.handle_intent(). Debug area shown only when debug=true (--debug). Enables gRPC clients to inject intents alongside crossterm keyboard input. (tddy-tui)
