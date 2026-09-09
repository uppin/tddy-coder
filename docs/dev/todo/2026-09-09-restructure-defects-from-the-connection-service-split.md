# Restructure defects found by the `connection_service.rs` split

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

**Suggested guard, cheap and targeted.** The backend knows the set of names it moved. After the
assist, every `<modname>::<Ident>` it wrote should have `Ident` in that set; an `Ident` that is not
is a mangled rewrite, and the operation should refuse and name the line. That catches this exactly,
costs one pass over the produced text, and needs no server round trip.

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

**Suggested fix.** When a seam carries a facade, the import pass must not add a named import to the
parent for any symbol that facade re-exports. Alternatively, emit such an import as `pub use`. The
first is better — it is one fewer binding, and the facade is already the declared mechanism.

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

Option 1 is the one worth building: without it, any seam touching a proto type in this crate is
unreachable, which is most of them.

## What this says about the order of work

`extract_module` is the operation that works here (`needs_inference: false`, so it does not hit the
inference-readiness wall that stops `extract_method`), and it has now applied 41 times on this file.
The remaining obstacles are all in the **import-restoration pass**, not in the assist or the
anchors. D8 then D7 then D6 is the order that unblocks the most lines per unit of work.
