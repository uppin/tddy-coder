# Changesets Applied

Wrapped changeset history for tddy-core.

- **2026-03-07** [Feature] Claude Stream-JSON Backend — Replaced plain-text backend with NDJSON stream processing (`--output-format=stream-json`), session management (`--session-id`/`--resume`), structured question extraction from AskUserQuestion, structured-response and delimited PRD/TODO parsing, progress callback. (tddy-core)
- **2026-03-06** [Feature] Planning Step Implementation — Added CodingBackend trait, ClaudeCodeBackend, MockBackend, Workflow state machine, output parser, artifact writer. (tddy-core)
