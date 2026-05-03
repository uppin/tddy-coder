# Changeset: Acceptance-tests session actions (TDD)

**Date**: 2026-05-03  
**Status**: Documentation wrap (implementation landed on branch)  
**Type**: Feature + documentation

## Shipped behavior (permanent docs)

- **`tddy-tools list-actions`**: JSON includes **`acceptance_tests_session_actions_contract_version`** (**`1`**) alongside **`actions`** for automation envelope detection (**`list_actions_contract`**).
- **TDD `acceptance-tests`**: **`before_acceptance_tests`** creates **`actions/`** and writes three manifests when each basename is absent (**`acceptance-single-test.yaml`**, **`acceptance-selected-tests.yaml`**, **`acceptance-package-tests.yaml`**) with **`architecture: native`**, **`command: [/bin/true]`**, stable **`id`** / **`summary`**; operators replace **`command`** with project **`cargo`** / **`./dev`** invocations.
- **System prompt**: acceptance-tests prompt documents **`list-actions`**, **`invoke-action`**, scoped runs, and **`tddy-tools get-schema acceptance-tests`**.
- **`tddy-coder`**: optional session **`coder-config.yaml`** **`session_actions_specialist`** (**`agent`**, **`model`**) merges into **`Args.session_actions_specialist_*`** when CLI equivalents are unset (CLI wins when set).
- **Tests**: **`packages/tddy-integration-tests/tests/workflow_recipe_acceptance_actions.rs`**; expanded **`packages/tddy-tools/tests/actions_cli_acceptance.rs`**; recipe unit tests under **`tddy-workflow-recipes`**.

**Feature docs:** [session-actions.md](../../ft/coder/session-actions.md), [workflow-recipes.md](../../ft/coder/workflow-recipes.md). **Changelog:** [coder/changelog.md](../../ft/coder/changelog.md). **Indexes:** [docs/dev/changesets.md](../changesets.md); [tddy-tools changesets](../../../packages/tddy-tools/docs/changesets.md), [tddy-workflow-recipes changesets](../../../packages/tddy-workflow-recipes/docs/changesets.md), [tddy-coder changesets](../../../packages/tddy-coder/docs/changesets.md).

## Follow-ups (not blocking doc wrap)

- [ ] **`Args.session_actions_specialist_*`**: presenter / backend selection uses the specialist pair where the PRD calls for a dedicated session-actions pass.
- [ ] Default manifests remain smoke stubs (**`/bin/true`**) until each workspace pins real test commands in **`actions/*.yaml`**.
