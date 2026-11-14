# 2026-11-14 — The pr-stack prompt describes the daemon git tools

**Type:** Feature

The `pr_stack` prompt's tool list describes what the daemon tools do now: `pr_repoint` rebases
a node's branch onto its effective base in its own worktree on the daemon, force-pushes it and
repoints its PR; `pr_resolve_conflicts` reports the conflicting files without touching the
worktree and, with `delegate`, asks the node's own session to resolve them. The repoint bridge
treats `rebase_onto`'s "branch has no worktree checked out" refusal as the remote-only case it
already skipped. The schema manifest is regenerated for the `tddy-tools` surface change.
