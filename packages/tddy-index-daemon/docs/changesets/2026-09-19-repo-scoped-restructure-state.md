# 2026-09-19 — The restructure error set gains a repo-scoped-journal refusal

**Type:** Fix

`tddy-code-restructuring` keys run state by the plan, and refuses rather than silently adopting a
journal left at `<root>/.restructure/` by an older repository-scoped run. That is a new error
variant, and `status.rs` maps it — classified `FailedPrecondition`, beside `JournalExists`. The
mapping is exhaustive by design, so a new variant is a compile error here rather than a default arm.

This crate's own apply loop still opens run state repository-scoped, so a plan applied **through the
index daemon** keeps the old behaviour: a completed plan blocks the next one under that root. It is
one line to change, but the per-root queue is justified partly by the journal carrying no plan
identity — in `apply.rs`, `index.rs`, `operations.rs`, `queries.rs` and `code-index-service.md` —
so the change is a documentation reconciliation as much as a call change. Recorded as
`code-issues/stale-repo-scoped-restructure-state-apply.md`.

Nothing is broken meanwhile: the queue serializes tree access as well as journal access, so it is
merely stricter than it now needs to be.
