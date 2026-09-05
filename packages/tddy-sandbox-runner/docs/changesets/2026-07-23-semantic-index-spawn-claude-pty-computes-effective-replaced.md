# 2026-07-23 — semantic-index: `spawn_claude_pty` computes `effective_replaced_tools(replaced, semantic_index_enabled)`

**Type:** Feature

where `semantic_index_enabled` is the presence of the daemon-injected `TDDY_SEMANTIC_INDEX_DB` — before building the claude allow/disallow lists, so `SemanticSearch` is hard-disabled for un-indexed sandboxed sessions. Feature [semantic-index.md](../../../../docs/ft/coder/semantic-index.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox-runner)
