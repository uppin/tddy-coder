# 2026-09-25 — a cross-crate move re-points only the moved file's top-level `use` lines, not those inside its own `mod tests`

**Category:** Future enhancement (engine defect; the destination's test build fails after the move)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`22787218:docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/03b-session-catalog-daemon-half-to-session-activity.jsonl`,
op 0 (`move_cluster_to_crate`: `session_deletion` + `session_reader` + `user_sessions_path` +
`session_list_enrichment` → `tddy-session-activity`)

## What happened

`session_list_enrichment.rs` has a `use crate::…` at the top of the file and two more inside its
inline test module:

```rust
// packages/tddy-session-lifecycle/src/session_list_enrichment.rs, before the move
use crate::session_context_docs::ContextDocKind;           // line 26, top level

#[cfg(test)]
mod tests {
    use super::*;
    …
    use crate::session_attachments::copy_attachment_into_session;   // line 1423
    use crate::session_context_docs::ATTACHMENT_DOC_DESCRIPTION;    // line 1424
    use tddy_workflow::session_attachments_root;                    // line 1428
    use tempfile::TempDir;
```

Lifecycle's `crate::session_context_docs` and `crate::session_attachments` are facades over
`tddy-session-files`. The header pass re-pointed line 26, and the manifest pass added
`tddy-session-files` for it. It left lines 1423–1424 exactly as they were:

```rust
// packages/tddy-session-activity/src/session_list_enrichment.rs, as the engine left it
use tddy_session_files::session_context_docs::ContextDocKind;   // re-pointed
…
    use crate::session_attachments::copy_attachment_into_session;   // unchanged
    use crate::session_context_docs::ATTACHMENT_DOC_DESCRIPTION;    // unchanged
    use tddy_workflow::session_attachments_root;                    // crate not carried
```

The test build then failed, with the lib build already clean:

```text
error[E0432]: unresolved import `crate::session_attachments`
    --> packages/tddy-session-activity/src/session_list_enrichment.rs:1423:16
error: could not compile `tddy-session-activity` (lib test) due to 1 previous error
```

`tddy-workflow` was not added to the destination's `[dev-dependencies]` either. It is a `path`
crate, so it is the pass's job and not the registry-crate limitation. That is the same blindness,
seen from the manifest side.

The compile gate reported only the lib errors, which stop cargo before the test target. So this
surfaced only after the lib was fixed by hand, and it was confirmed by reverting that line alone.

## Why

The header pass treats a file's "`use` header" as its **top-level** `use` declarations. A `use`
inside the file's own `#[cfg(test)] mod tests { … }` is a `use` declaration too, and `crate::`
changed meaning for it in exactly the same way when the file changed crates. Neither pass sees it.
This is not the documented body-path limitation, which is about `crate::x::f(…)` in an expression.

## What was fixed by hand

```rust
    use tddy_session_files::session_attachments::copy_attachment_into_session;
    use tddy_session_files::session_context_docs::ATTACHMENT_DOC_DESCRIPTION;
```

```toml
# packages/tddy-session-activity/Cargo.toml
[dev-dependencies]
tddy-workflow = { path = "../tddy-workflow" }
tempfile = "3"
```

## What would fix it

Walk every `use` item in the moved file, at any module depth, not just the file's root items. A
`crate::` head inside an inline module of the moved file changed meaning just as a top-level one
did. A `super::` head that stays inside the moved file must not change (gap H in
[the destructure-run gaps](2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md)).
A crate first named under `#[cfg(test)]` goes to `[dev-dependencies]`.
