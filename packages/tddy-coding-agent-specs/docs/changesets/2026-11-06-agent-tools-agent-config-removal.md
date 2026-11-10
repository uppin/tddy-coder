# 2026-11-06 — claude.yaml declares the fullscreen renderer

**Type:** Feature

`#agent-tools` 19/19 ([#825](https://github.com/uppin/tddy-coder/pull/825)).

- `specs/claude.yaml` `run.env: { CLAUDE_CODE_NO_FLICKER: "1" }` — the only place it is set; an override without it starts Claude inline. `coding_agent.answer_wait_secs` is now the answer wait's only source.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-agent-config-removal.md).
