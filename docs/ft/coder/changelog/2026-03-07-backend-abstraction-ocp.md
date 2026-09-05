# 2026-03-07 — Backend Abstraction (OCP)

- **Backends**: Claude Code CLI and Cursor agent supported. Use `--agent claude` (default) or `--agent cursor`
- **CLI**: `--agent <name>` selects backend; `--prompt <text>` provides feature description (alternative to stdin)
- **Architecture**: InvokeRequest slimmed (Goal enum, no Claude-specific fields). InvokeResponse.session_id optional. Stream parsing split per backend (stream/claude.rs, stream/cursor.rs)
- **changeset.yaml**: Session entries include `agent` field for resume
