# 2026-03-07 — Claude Stream-JSON Backend

- **Output format**: Switched from plain text to NDJSON stream (`--output-format=stream-json`)
- **Session management**: `--session-id` on first call, `--resume` on Q&A followup for context continuity
- **Structured Q&A**: Questions from `AskUserQuestion` tool events; TUI mode uses ratatui Select/MultiSelect with "Other" option; plain mode uses stdin (one answer per line)
- **Real-time progress**: Tool activity display (Read, Glob, Bash, etc.)
- **Output parsing**: Structured-response format (`<structured-response content-type="application-json">`) with delimiter fallback
- **Agent output**: Always visible; on resume with `--conversation-output`, replayed output is skipped
