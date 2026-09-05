# 2026-03-28 — StartSession and spawn: `recipe`

- **`StartSession` / `StartSessionRequest`**: Optional **`recipe`** (`tdd` or `bugfix`); empty behaves like **`tdd`**. Session **`changeset.yaml`** persists **`recipe`** for the new session.
- **Spawn**: **`SpawnRequest`** includes **`recipe`**; the daemon passes **`--recipe`** to **`tddy-coder`** when set.
- **Package**: [connection-service.md](../../../../packages/tddy-daemon/docs/connection-service.md). Coder feature: [workflow-recipes.md](../../coder/workflow-recipes.md).
