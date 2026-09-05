# 2026-07-01 — `FreePromptingRecipe::plain_goal_cli_output` prints via `println!`

**Type:** Fix

was `log::info!`-only, which is invisible in the `tddy-coder` binary's plain-mode output (logs redirect to a per-session `debug.log` file, not stderr); now matches the convention every other shipped recipe's `plain_goal_cli_output` already uses (`BugfixRecipe`, `TddRecipe`). Part of a cross-package fix for plain-mode `free-prompting` crashing with `no pending questions`. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes)
