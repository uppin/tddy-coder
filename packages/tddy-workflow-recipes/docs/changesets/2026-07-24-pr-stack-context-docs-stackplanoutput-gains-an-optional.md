# 2026-07-24 — pr-stack-context-docs: `StackPlanOutput` gains an optional `exploration` field and `after_write_stack_plan` persists `artifacts/exploration.md` when non-blank (parity with tdd/bugfix); the `PrStackRecipe` manifest registers `exploration → exploration.md` and the `orchestrate` `before_task` prepends the `<context-reminder>` header for the on-disk manifest docs. New defaulted `SessionArtifactManifest::artifact_doc_descriptions()` (key→one-liner), overridden by `PrStackRecipe` for all five known artifacts (consumed by the daemon's context-docs surface). Feature [pr-stacking.md](../../../../docs/ft/coder/pr-stacking.md), [exploration-artifact.md](../../../../docs/ft/coder/exploration-artifact.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes)

**Type:** Feature


