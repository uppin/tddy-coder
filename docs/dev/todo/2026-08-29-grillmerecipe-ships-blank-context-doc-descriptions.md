# 2026-08-29 — `GrillMeRecipe` ships blank context-doc descriptions

**Category:** Future enhancement
**Source:** pr-stack-docs changeset, 2026-08-29

`GrillMeRecipe` does not override `SessionArtifactManifest::artifact_doc_descriptions()`, so its two
context docs (`grill-me-brief.md`, `exploration.md`) reach the web with an empty `description`.
`PrStackRecipe` supplies a one-liner per key (`pr_stack/mod.rs:342-362`) and is the model to follow.
Cheap; affects any client rendering a doc list.
