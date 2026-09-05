# 2026-03-28 — Workflow goal conditions and session context

- **Engine**: Workflow transitions evaluate declarative **`goal_conditions`** against **`Context`**. **`Context::merge_json_object_sync`** applies session JSON so predicates see the same keys as the persisted session file.
- **CLI**: **`tddy-tools set-session-context`** merges JSON into **`.workflow/<session-id>.session.json`** using **`TDDY_SESSION_DIR`** and **`TDDY_WORKFLOW_SESSION_ID`**. The subcommand is **not** registered in **`goals.json`** (session utility, not a schema-backed planning goal).
- **TDD graph**: Boolean session key **`run_optional_step_x`** selects the demo branch after green; recipe hooks use **`tddy-tools ask`** and **`set-session-context`** to record the choice.
- **Docs**: [workflow-recipes.md](../workflow-recipes.md) (session context section), [workflow-json-schemas.md](../workflow-json-schemas.md) (`set-session-context`), [implementation-step.md](../implementation-step.md) (demo workflow and state machine).
