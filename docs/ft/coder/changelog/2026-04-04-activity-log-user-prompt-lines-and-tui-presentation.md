# 2026-04-04 — Activity log: user prompt lines and TUI presentation

- **Core**: **`ActivityKind::UserPrompt`** marks submitted feature text and queued inbox lines in **`activity_log`** / **`ActivityLogged`**. **`format_user_prompt_line`** returns plain text (no `User: ` prefix); **`format_queued_prompt_line`** keeps the **`Queued: `** prefix. **`tddy-service`** maps the kind to the **`UserPrompt`** string for RPC consumers.
- **TUI**: User prompt entries render as a three-row inset block (margins on all sides): first row empty, text on rows two and three with hard wrap and ellipsis when needed; panel **`Rgb(85, 85, 85)`**, text **`Rgb(255, 255, 255)`** bold.
- **Tests**: **`presenter_integration`** expects exact submitted text; **`tddy-tui`** unit tests for user-prompt row layout helpers.
- **Docs**: [Activity log streaming](../activity-log-streaming.md); **`packages/tddy-core/docs/architecture.md`**; **`packages/tddy-tui/docs/architecture.md`**.
