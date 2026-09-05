# 2026-04-06 — Telegram `/start-workflow`: branch/worktree intent step

- **`tddy-daemon`**: After a recipe is saved (excluding **More recipes** follow-up), the bot prompts for **branch/worktree intent** (**New branch + worktree** vs **Work on existing branch**). The choice is written to **`changeset.yaml`** under **`workflow.branch_worktree_intent`** (`new_branch_from_base` / `work_on_selected_branch`) before project selection. Inline **`callback_data`** uses compact **`intent:nb|s:<session_id>`** and **`intent:ws|s:<session_id>`** so payloads stay within Telegram’s 64-byte limit with a UUID session id.
- **Feature doc**: [telegram-session-control.md](../telegram-session-control.md). Package history: [changesets/](../../../packages/tddy-daemon/docs/changesets/).
