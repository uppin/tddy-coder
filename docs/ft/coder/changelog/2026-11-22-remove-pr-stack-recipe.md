# 2026-11-22 — The pr-stack workflow recipe is removed

Stacks are planned and driven through the project stack view and the `pr_*` tools only. No recipe
surface — `tddy-coder --recipe`, the TUI, `/start-<recipe>`, the web recipe dropdown, `StartSession` —
offers `pr-stack`, `plan-pr-stack` or `orchestrate-pr-stack`, and the start form has no stack base
picker (`StartSessionRequest` field 31 is reserved). A session recorded with the removed recipe
lists, opens in the workflow chat view, and is refused on resume with the recipe named. See
[PR stacking](../pr-stacking.md).
