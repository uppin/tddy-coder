# 2026-07-24 — pr-stack-context-docs: new `session_context_docs` module

**Type:** Feature

`context_docs_for_session(recipe, session_dir)` lists the recipe manifest's planning docs (absolute `artifacts/` path + on-disk existence + per-key description via `SessionArtifactManifest::artifact_doc_descriptions()`; blank/unknown recipe → empty), and `read_session_context_doc_utf8` reads an allowlisted doc with a canonicalize-and-contain guard rooted at `session_artifacts_root` (non-manifest / traversal basename → `PermissionDenied`; mirrors `session_workflow_files`). `session_list_enrichment::apply_session_list_status_to_proto` populates `SessionEntry.context_docs` (field 27). New internal `tddy-workflow` dep. Tests: `session_context_docs` 4 + `apply_context_docs_to_proto_lists_the_recipe_manifest_docs`. Feature [pr-stacking.md](../../../../docs/ft/coder/pr-stacking.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)
