# 2026-07-23 — `effective_replaced_tools`

**Type:** Feature

folds the semantic-index decision into the replaced-tool set: `SemanticSearch` joins the set (dropped from `--allowedTools`, hard-disabled via `--disallowedTools`) when indexing is disabled; unchanged otherwise. Feature [semantic-index.md](../../../../docs/ft/coder/semantic-index.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox-recipes)
