# 2026-07-01 — Configurable FastContext model + plain-mode free-prompting completion

**Type:** Feature+Fix

`--fastcontext-model`/`fastcontext_model` config threaded through `create_backend` to `FastContextBackend::new` (default unchanged: `microsoft/FastContext-1.0-4B-RL`), enabling any OpenAI-compatible model tag including a locally-served Ollama model. `run_full_workflow_plain`/`run_goal_plain`'s `WaitingForInput` handling now checks a new `waiting_for_input_has_pending_questions` predicate before erroring `no pending questions`; when false (e.g. a single `--recipe free-prompting` turn with `--agent fastcontext`/`stub`), prints the response via `plain_goal_cli_output` and completes instead of crashing. Feature [discovery-agent.md](../../../../docs/ft/coder/discovery-agent.md), [workflow-recipes.md](../../../../docs/ft/coder/workflow-recipes.md); PR [#251](https://github.com/uppin/tddy-coder/pull/251). (tddy-coder, tddy-core, tddy-discovery, tddy-workflow-recipes)
