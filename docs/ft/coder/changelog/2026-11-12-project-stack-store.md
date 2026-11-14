# 2026-11-12 — PR stacks live in the project

`#orchestrators 2/11` ([#879](https://github.com/uppin/tddy-coder/pull/879)).

- **A PR stack belongs to its project**, stored per host under `projects/<project_id>/stacks/`, and
  is addressed by its id. The planning session keeps only that id.
- **Existing stacks are moved automatically** when the daemon starts. Nothing changes on the PR-stack
  screen, in the `pr_*` tools or in the recipe.
- A pr-stack session no longer resumes into the deprecated orchestration loop; it resumes into
  planning.
- An older daemon cannot read a migrated stack.

See [PR stacking § Where a stack lives](../pr-stacking.md#where-a-stack-lives).
