# Restructure defects found by the `connection_service.rs` split

> **Status: D6, D7, D8 and D9 are fixed** (branch `feature/connection-service-split/lsp-settle-budget`).
> D8's fix is proven end-to-end: the four seams it blocked now apply, and the run writes the five
> aliased `use` lines rust-analyzer could never offer. Kept as the record of what each defect was
> and how it presented, because each one presented as something it was not.

**Date:** 2026-09-09
**Found by:** applying `plan-4-free-items` / `plan-5-seed-and-stack` to
`packages/tddy-daemon/src/connection_service.rs` (17,605 lines).

Two of these produced a run that **reported success over code that did not compile**, which is the
class that matters: `restructure verify --against` compares statement multisets and would have
flagged both, but nothing ran it automatically and `apply` said `applied 3 of 3 operations`.

## D6 — a rewritten reference can come out mangled (data corruption)

`extract_module` moved `SeededCloneGuard` into `seeded_clone_guard`, and rewrote one reference to:

```rust
) -> Result<seeded_clone_guard::SeededCloneGuardloneGuardloneGuard, Status> {
```

`SeededCloneGuard` + `loneGuard` + `loneGuard`. Not present in `HEAD`; introduced by the apply.
Exactly one occurrence out of ~20 rewritten references to that symbol.

The shape — the tail of the identifier inserted twice, offset by four characters — is an
**overlapping text edit applied at a stale offset**: two edits targeting the same reference, the
second computed against the pre-first-edit text.

**Why it was not caught.** The backend already guards the adjacent case: it refuses when an
extraction *placeholder* occurs more often after the assist than before. That check is keyed to the
placeholder name (`modname`, `fun_name`), not to rewritten references, so a mangled *reference*
passes it.

**Fixed** by `refuse_mangled_rewrite`, exactly as suggested: the backend knows the set of names it
moved, so every `<modname>::<Ident>` it wrote must have `Ident` in that set. One pass over the
produced text, no server round trip. A path through any other module is not weighed, including one
whose qualifier merely ends with this module's name.

Rust identifiers must resolve, so this class is loud rather than silent — the compiler caught it.
That is luck about Rust, not a property of the tool.

## D7 — the import pass defeats the facade it was given

`reexport: "glob"` promises that callers outside the module keep resolving. The run wrote both:

```rust
// line 69, added by the import-restoration pass
use crate::{agent_list_mapping::agent_allowlist_rows,
            connection_service::{seed_codebase::SeededAgentClones, stack_parent::StackParentHost}};
// line 1301, the requested facade
pub use seed_codebase::*;
```

A **private** named import shadows the glob re-export for `crate::connection_service::SeededAgentClones`,
so `cursor_cli_spawn.rs` failed with `E0603: trait import ... is private` — the facade was present
and inert.

The named import is also **redundant**: the glob brings the name into the parent's own scope, which
is why deleting it compiles.

**Fixed** by `facade_will_bind`, taking the first option: `restore_imports` now receives the moved
items and the reexport kind, and treats a name the facade will re-export as needing no import. The
predicate mirrors `facade_lines` — a glob covers everything the module holds, a named facade only
the top-level items something outside reaches.

## D8 — an aliased import cannot be reconstructed, and four seams are blocked on it

Refused, correctly:

```
no import rust-analyzer offered for `ProtoProbeOutcome` left fewer of its 4 unresolved
occurrence(s) — tried Import `tddy_service::proto::connection::ProbeOutcome`.
```

The parent imports 14 types under aliases (`ProbeOutcome as ProtoProbeOutcome`,
`ConnectionService as ConnectionServiceTrait`, `AgentActivityRecord as ProtoAgentActivityRecord`, …).
rust-analyzer offers the *unaliased* path, which does not provide the alias binding, so the pass
cannot restore it and refuses rather than writing a `use` that resolves nothing. **That refusal is
right** and the guard is working.

It blocks four planned seams: `host_messages`, `activity_hub`, `hooks_and_urls`, `agent_roster`
(~990 lines).

**Options, cheapest first.**

1. **Teach the import pass about aliases.** When a name is unresolved in moved code and the *parent's*
   `use` tree binds it via `as`, reconstruct `use <path> as <alias>;` from the parent's own
   declaration. The information is entirely in the source the backend already reads, no server
   involvement, and it generalises past this file — the alias is how every generated-proto type in
   this workspace is referred to.
2. Qualify the aliased names in the seam before extracting (mechanical, per-seam, hand-edited).
3. Drop the aliases at source and use the unaliased names throughout — a large, unrelated diff.

**Fixed** by `alias_target` / `aliased_bindings` / `with_module_import`, taking option 1. When a
name is unresolved in moved code and the parent's own `use` tree binds it via `as`, the binding is
reconstructed from that declaration and written as the module's first line — no server involvement,
because the server reports what a path resolves to and not what a file chose to call it. Only
declarations outside the module count, and `as _` binds no name so it contributes none.

Proven on the four seams it blocked (992 lines), which now apply and receive:

```rust
use tddy_service::proto::connection::ProbeOutcome as ProtoProbeOutcome;      // host_messages
use tddy_service::proto::connection::AgentActivityRecord as ProtoAgentActivityRecord;
use std::sync::Mutex as StdMutex;                                            // activity_hub
use tddy_service::proto::connection::ProjectEntry as ProtoProjectEntry;      // hooks_and_urls
use tddy_service::proto::connection::ConnectionService as ConnectionServiceTrait;
```

## What this says about the order of work

`extract_module` is the operation that works here (`needs_inference: false`, so it does not hit the
inference-readiness wall that stops `extract_method`), and it has now applied 41 times on this file.
The remaining obstacles are all in the **import-restoration pass**, not in the assist or the
anchors. D8 then D7 then D6 is the order that unblocks the most lines per unit of work.


## D9 — a `pub` glob facade over a module that publishes nothing

Found while validating the D8 fix. `facade_lines` emitted `pub use <module>::*;` unconditionally,
but the assist rewrites what it relocates to `pub(crate)`, so a seam of private items produced a
`pub` glob re-exporting nothing:

```
error: glob import doesn't reexport anything with visibility `pub`
       because no imported item is public enough
   --> connection_service.rs:781:9  |  pub use host_messages::*;
```

`clippy::unused_imports` names it, and under `-D warnings` it fails the build the restructure was
supposed to leave green — so this is not cosmetic.

**Fixed** by `widest_visibility`: `pub use` where some relocated item is `pub`, `pub(crate) use`
otherwise. `pub(crate)` is the right default — a seam that moved nothing public has nothing to
publish, and it is both what the assist widened its members to and how the parent's own dependents
reach the facade.

## What a facade still cannot do, and it is worth stating

A glob facade re-exports the module's **items**. It cannot re-export a name the module merely
**imports**, because a private `use` is not a re-export. So a child of the parent — one of the 29
extracted test modules, reaching names through `use super::*` — loses any name that was a *parent
import* consumed by moved code and pruned when that code left.

That is what `ssh_agent_block_handler_tests.rs` hit for `SshAgentKey` and `ProtoProbeOutcome`. The
fix is local and belongs in the test file (bind the two names there), not in the parent, which no
longer uses either in its lib target — carrying them would trade a resolution error for an
unused-import error. Worth knowing before the next seam: **check the extracted test modules, not
only the lib, after a seam that moves proto converters.**
