# 2026-06-26 — PR-stack session UI: recipe dropdown, parent picker, collapsible drawer groups

- `CreateSessionPane` recipe field replaced with a `<select>` listing all 9 workflow recipes (tdd, tdd-small, bugfix, free-prompting, grill-me, review, merge-pr, plan-pr-stack, orchestrate-pr-stack); default is "tdd"
- New parent-picker `<select>` (tool sessions only): lists orchestrator sessions so a child can be attached to an existing PR-stack; hidden for claude-cli sessions
- `SessionDrawer` groups PR-stack children under the orchestrator session in a collapsible `<details>/<summary>` element; children render at `depth={1}` (indented)
- Orphan children (orchestrator not present in the list) fall through to the flat list
- New utils: `stackParentCandidates(sessions)`, `groupSessionsByStack(sessions)`
