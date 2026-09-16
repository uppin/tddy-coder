# 2026-09-16 — `.restructure/`'s layout is not public API

**Type:** Refactor

`StatePaths`' `journal` and `ledger` fields are private. `StatePaths`, `StatePaths::under`,
`open_run`, `restore_ledger` and `commit_operation` stay public, which is what a host driving the
apply loop actually needs — it passes the value along rather than reading a path out of it.

The fields were public so that a host could read the journal for a status-style query. Nothing needs
that: `runner::status` returns `PlanProgress`, so the one caller that might have gone looking for a
path asks for the answer instead. Keeping the layout private means re-keying the journal to a plan
identity stays a non-breaking change.

The layout is now asserted from inside the module that owns it, which is the only place that can see
it.
