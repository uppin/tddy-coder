# 2026-07-22 — Code pane syntax highlighting

- The Worktree **Code pane** file preview now syntax-highlights recognized code files (Rust, TS/TSX, Python, JSON, YAML, and more) instead of showing plain monospace text. The language is inferred from the file's extension; files with no recognized extension (e.g. `LICENSE`) stay plain, and Markdown keeps its sanitized-markup rendering. Highlight colors follow the app's light/dark theme. See [session-code-pane.md](../session-code-pane.md).
