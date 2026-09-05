# 2026-03-07 — Conversation Logging

- **CLI**: `--conversation-output <path>` logs the entire agent conversation in raw bytes to the specified file. Each NDJSON line is written in real time as it is received, so you can tail the file during long runs.
