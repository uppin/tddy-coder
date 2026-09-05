# 2026-03-10 — Goal Enhancements

- **changeset.yaml**: Replaces `.session` and `.impl-session` as the unified manifest. Contains name (PRD name from plan agent), initial_prompt, clarification_qa, sessions (with system_prompt_file per session), state, models, discovery, artifacts.
- **Plan goal**: Project discovery (toolchain, scripts, doc locations, relevant code). Demo planning (demo-plan.md). Agent decides PRD name. Stores initial_prompt and clarification_qa in changeset.yaml.
- **Observability**: Each goal displays agent and model before execution. State transitions displayed.
- **System prompts**: Stored in session directory (e.g. system-prompt-plan.md); referenced per-session via system_prompt_file in changeset.yaml.
- **Green goal**: Executes demo plan when demo-plan.md exists; writes demo-results.md.
- **Model resolution**: Goals use model from changeset.yaml when --model not specified; CLI --model overrides.
