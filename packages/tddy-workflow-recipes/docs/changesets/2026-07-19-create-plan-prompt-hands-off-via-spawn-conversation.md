# 2026-07-19 — Create-plan prompt hands off via `spawn_conversation`

**Type:** Feature

`grill_me/prompt.rs` `create_plan_system_prompt` gains a required final "Hand off to implementation" step: once both brief files exist, commit `plans/<slug>.md`, then call `tddy-tools spawn_conversation` (kebab CLI `spawn-conversation`) with a prompt referencing the absolute session-artifact brief path + `plans/<slug>.md` and a `branch`. `grill_me/mod.rs` leaves `goal_hints("create-plan").allowed_tools` at `vec![]` (empty = allow-all; no gating change) and `goal_requires_tddy_tools_submit` at `false`. Test: Create-plan prompt names `spawn_conversation`; pr-stack acceptance green (no regression). Feature [spawn-conversation.md](../../../../docs/ft/coder/spawn-conversation.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes)
