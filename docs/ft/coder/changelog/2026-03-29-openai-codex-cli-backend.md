# 2026-03-29 — OpenAI Codex CLI backend

- **Core**: `CodexBackend` implements `CodingBackend` with `codex exec` and `codex exec resume <id>`, `--json` JSONL stdout, `-C` working directory, `-m` model; sandbox and approval flags derived from recipe `GoalHints` (read-only plan goals use read-only sandbox; editing goals use workspace-write; `--ask-for-approval never` for non-interactive runs).
- **Prompt**: System instructions merge into the user prompt with the same precedence as Cursor (`system_prompt_path` over inline `system_prompt`).
- **CLI**: `--agent codex`; `--codex-cli-path` and `TDDY_CODEX_CLI`; YAML `codex_cli_path` in coder config; `tddy-tools` availability required for codex like claude and cursor.
- **Selection**: Interactive menu order Claude → Claude ACP → Cursor → Codex → Stub; default model label `gpt-5` for agent key `codex`.
- **Tests**: Unit tests in `tddy-core`; stub-based integration tests in `tddy-integration-tests` (`codex_backend`); CLI acceptance for `--agent codex`.
- **Docs**: [Coder overview](../1-OVERVIEW.md), [planning step](../planning-step.md), [implementation step](../implementation-step.md); cross-package index `docs/dev/changesets.md`; `packages/tddy-core/docs/architecture.md` and package `changesets.md` files (`tddy-core`, `tddy-coder`).
