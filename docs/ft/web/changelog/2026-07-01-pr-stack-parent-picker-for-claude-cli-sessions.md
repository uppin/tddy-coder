# 2026-07-01 — PR stack parent picker for Claude CLI sessions

- Claude CLI sessions can now be placed in a PR stack by selecting a parent in the new-session form, with git-base chaining automatically applied (child worktree branches off the parent's branch)
- Parent picker now renders for **both Tool and Claude CLI** session types (previously tool-only)
- Picker filters to **PR-stack orchestrator sessions only** (recipe `orchestrate-pr-stack` or `plan-pr-stack`), including childless orchestrators — replaces the old child-derived heuristic
