# 2026-07-26 — Claude models: versionless aliases by default, one shared catalog

- Claude sessions now default to the **versionless `opus` alias**, so a session tracks the newest Opus release instead of freezing on the generation that was current when it started; `sonnet` and `haiku` join it at the top of the dropdown.
- **Version-pinned ids remain selectable** in the same dropdown for callers who need a frozen generation, refreshed to the Claude 5 family (`claude-opus-5`, `claude-sonnet-5`, `claude-haiku-4-5-20251001`); labels read `Claude Opus (latest)` vs `Claude Opus 5 (pinned)` so the two are never confused.
- The web keeps **no model list of its own** — the hardcoded `CLAUDE_CLI_MODELS` constant is gone and every dropdown renders whatever `ListAgentModels` returns, preselecting the daemon's `default_model`. See [tool-session-model-selection.md § Model sourcing per backend](../tool-session-model-selection.md#model-sourcing-per-backend).
