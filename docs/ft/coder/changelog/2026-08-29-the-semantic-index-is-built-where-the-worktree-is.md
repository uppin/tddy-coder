# 2026-08-29 — The semantic index is built where the worktree is

- **A split-placement session may ask for a semantic index.** The field was refused outright on a split start; it is now carried to the **codebase host**, which builds the index against the worktree that actually exists there, for the session's `workspace` half. The agent host indexes nothing, having no checkout to index. See [semantic-index.md](../semantic-index.md) and [remote-managed-worktree.md](../../daemon/remote-managed-worktree.md).
- **The Semantic index toggle is no longer withdrawn** once a codebase host is selected, and the chosen value is submitted rather than blanked.
- ⚠️ **Still not answerable, and unchanged by this.** `SemanticSearch` returns *"index query not yet wired"* on every session shape, because the query-side embedder is not wired into the tool engine. What ships is the index being built on the right host; the query path remains the target state, now stated as such in the feature doc.
