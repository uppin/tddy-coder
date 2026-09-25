# 2026-09-25 — moving a nested module to a crate leaves its parent's `use <module>::*;` dangling, and globs the destination's whole root into the parent

**Category:** Future enhancement (engine defect; the origin does not compile after the move, and `check --deep` does not see it coming)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), plan
`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/05a-attachment-progress-to-session-files.jsonl`,
op 0 (`move_module_to_crate`: `connection_service::attachment_progress` → `tddy-session-files`,
`reexport: glob`)

## What happened

The moved module is not a crate-root module. It is a private child of `connection_service`, and its
parent re-exports all of it to its siblings:

```rust
// packages/tddy-session-lifecycle/src/connection_service.rs, before the move
mod attachment_progress;
pub(crate) use attachment_progress::*;
```

Ten sibling files reach its five items through that glob: `use super::AttachmentProgressSink;`,
`use super::super::AttachmentMaterialization;`, and so on. None names `attachment_progress` itself.
The deep check's survey therefore found nobody to re-point:

```text
op 0 survey: packages/tddy-session-lifecycle/src/connection_service/attachment_progress.rs -> tddy-session-files (tddy_session_files), 5 item(s) reached from outside, 0 caller(s)
no findings
```

`apply` replaced the `mod` line with a glob over the destination's **root**. It left the parent's
re-export of the module it had just removed in place:

```rust
// packages/tddy-session-lifecycle/src/connection_service.rs, as the engine left it
pub use tddy_session_files::*;              // was `mod attachment_progress;`
pub(crate) use attachment_progress::*;      // names a module that is no longer here
```

`attachment_progress` does resolve through the new glob, as `tddy_session_files::attachment_progress`.
But its items are `pub(crate)` there, so the glob-of-a-glob re-exports none of them to lifecycle.
Every sibling failed:

```text
error[E0432]: unresolved import `super::AttachmentProgressSink`: no `AttachmentProgressSink` in `connection_service`
error[E0432]: unresolved import `super::super::AttachmentMaterialization`: no `AttachmentMaterialization` in `connection_service`
…
error: could not compile `tddy-session-lifecycle` (lib) due to 19 previous errors
```

The facade also published `tddy-session-files`' whole root (`service`, `context_files`,
`host_documents`, …) as `tddy_session_lifecycle::connection_service::…`, paths no caller used. That
is the nested-module form of the root-glob cause in
[`2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md`](./2026-09-25-restructure-glob-facade-re-exports-a-name-the-origin-shadows.md).

## Why

The facade is written for a module that is reached **by name** (`crate::m::X`): replacing `mod m;`
with something that makes `m` resolve keeps those paths working. A module that is reached through its
parent's `use m::*;` has no name-reaching callers, so the survey reports 0 of them. And the parent's
re-export is the one line that has to change, but the facade writer does not look at it.

The visibility half is the known widening gap
([2026-09-09](./2026-09-09-restructure-defects-from-the-first-cross-crate-move.md), item 3). It bites
harder here: with no callers surveyed, nothing even hints that the five `pub(crate)` items are used
from the origin.

## What was fixed by hand

```rust
// packages/tddy-session-lifecycle/src/connection_service.rs
pub(crate) use tddy_session_files::attachment_progress::*;
```

Every `pub(crate)` in the moved `attachment_progress.rs` became `pub`: the three structs, their fields
and methods, and the two free fns. The siblings' `use super::…` lines were left alone, and resolve
through the re-export.

## What would fix it

When the moved module's parent holds a `use <module>::*;` or `use <module>::{…};`, rewrite **that
line** to the destination path (`use <dest>::<module>::*;`, keeping its visibility) and drop the `mod`
line with no extra facade. Count every item the re-export makes visible as reached from outside, so
the widening question is asked. `check --deep` should say so too: "0 caller(s)" beside "5 item(s)
reached from outside" is the signature of a re-exported module.
