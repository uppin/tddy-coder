# 2026-11-12 — The daemon migrates PR stacks at startup

**Type:** Architecture

- `build_session_host` runs `tddy_pr_stack::stack_migration::migrate_session_stacks` over the data directory before the host serves, beside the headless-files sweep (`sweep_what_the_last_run_left`). Covered by `tests/startup_stack_migration_acceptance.rs`.

Cross-package: [PR stacks live in the project](../../../../docs/dev/changesets/2026-11-12-project-stack-store.md). Product: [Where a stack lives](../../../../docs/ft/coder/pr-stacking.md#where-a-stack-lives).
