# 2026-03-28 — Connection screen: workflow recipe (TDD / Bugfix)

- **ConnectionScreen**: Per project, **Start New Session** includes a **Workflow recipe** control (**TDD** vs **Bugfix**); the selected value is sent as **`recipe`** on **`StartSession`** / **`StartSessionRequest`** (proto **`connection.proto`** / **`remote.proto`**).
- **Vite**: **`tddy-livekit-web`** resolves via **`resolve.alias`** to package source for component tests and dev without a prior **`dist`** build.
- **Feature docs**: [workflow-recipes.md](../../coder/workflow-recipes.md), [web-terminal.md](../web-terminal.md) (Connection screen).
