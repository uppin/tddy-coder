# 2026-07-01 — Configurable FastContext model + fix plain-mode free-prompting crash

- `--fastcontext-model` CLI flag / `fastcontext_model` YAML config key lets the FastContext (Discovery) backend target any OpenAI-compatible model tag — including a locally-served model via Ollama — instead of only the hardcoded `microsoft/FastContext-1.0-4B-RL` default
- Fixed `tddy-coder`'s plain (non-TUI) CLI crashing with `Error: no pending questions` on any single-shot `--recipe free-prompting` turn (e.g. `--agent fastcontext`, `--agent stub`) — `FreePromptingRecipe`'s single `prompting` task has no graph successor by design, so the workflow engine reports the same "waiting" status it uses for a genuine clarification question; the plain CLI now tells the two apart and completes instead of erroring
- `BackendInvokeTask` now persists a no-submit turn's response into the session context so plain-mode callers have something to print
- Feature: [discovery-agent.md](../discovery-agent.md), [workflow-recipes.md](../workflow-recipes.md)
- PR: [#251](https://github.com/uppin/tddy-coder/pull/251)
