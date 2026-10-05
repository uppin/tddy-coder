# Changeset: `repoint_call` re-points one call's callee, or the receiver of every call of one method

**Date**: 2026-10-05
**Status**: 🚧 In Progress
**Type**: Feature (new restructure operation; text edits only, no change to what existing operations do)
**Stack**: `#sharpen` 7/8, branch `feature/sharpen/repoint-call`, wave 2. PR title:
`feat(code-restructuring): repoint_call re-points a call's callee or every receiver of a method (#sharpen 7/8)`.
Base in the linear stack: `feature/sharpen/retarget-impl` (K=6). **Real edges**: `tidy-engine-files` (K=1, file overlap and the new home of `RefactorKind`) and
`retarget-impl` (K=6, **surface**: the `restructure verify` declaration carrier this node extends).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-05-sharpen-repoint-call-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

The scan followed `deferred-work/references/planning-cross-check.md`. `grep -rl 'Claimed by:'` over `tddy-code-restructuring`,
`tddy-tools` and `tddy-index-daemon` finds one file (`broken-restructure-anchors-empty-outline.md`) whose value is `none` (#537 merged), so **no 🚧
claimed issue is in this change's path and there is no wait-or-proceed fork.** `tddy-lsp` is not touched.

| Item | Verdict | What this change does about it |
|---|---|---|
| `docs/dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md` — **exists only on `feature/carve/lifecycle-ports-agents` (PR #532)**; whichever of that PR and this node lands second deletes it at wrap | ℹ **ANSWERED (receiver half)**; not ✅ here | The receiver/callee half is delivered here (Scope). The **delegator half** (`leave_delegator`) belongs to `retarget-impl`, the lowest node, which claims the whole entry and deletes it at wrap. This node keeps a reference and deletes nothing |
| [2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md](../todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md) | ⚠ **DURING** — deliberately **not** closed | Developer decision: calls and receivers only. `self.<field>` -> `state.<field>` is a field read (see Boundaries) and stays this master todo. Not in ✅ RESOLVED HERE; the wrap must not delete it |
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` | ⚠ **DURING** | `backends/rust.rs` gains wiring only (about 10 lines: `mod`, `SUPPORTED`, one `check` arm, one `resolve` arm). All logic in `backends/rust/repoint_call/`. A history row is appended at wrap |
| [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **DURING** | Same constraint |
| [2026-10-05-restructure-engine-files-past-the-500-line-budget.md](../todo/2026-10-05-restructure-engine-files-past-the-500-line-budget.md) | ⚠ **DURING** (resolved by `tidy-engine-files`, not here) | This node adds a `RefactorKind` variant to `plan/refactor_kind.rs` (the kind's home after `tidy-engine-files`), one `RefactorOp` field to `plan.rs`, one `mod` + call to `plan/codec.rs`. It must leave all of them <= 500 production lines: see O8 (headroom after tidy) |
| `packages/tddy-code-restructuring/docs/code-issues/dead-code-plan-filehint-modified.md`, `oversized-file-test-binary.md`, `complexity-rust-facade-lines.md`, `broken-restructure-anchors-empty-outline.md` | — | Not in this node's path |
| [2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md](../todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md) § 6 (`verify` reports one `)` lost on a reflowed call) | — Unrelated, noted | A lengthened callee can make rustfmt reflow a call and hit § 6. Not fixed here; the new pairing is declared, so it never depends on the token pass |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md) (operation list), `src/plan/refactor_kind.rs` (variant), `src/plan.rs` (one field; and `callee: None` on the 20 full `RefactorOp` struct literals in 15 files),
  `src/plan/codec.rs` (one `mod`, one call) and new `src/plan/codec/repoint_call_fields.rs`, `src/plan/rust_syntax.rs` (`one_callee`),
  `src/backends/rust.rs` (wiring), new `src/backends/rust/repoint_call.rs` + `repoint_call/{single,receivers,sites}.rs`,
  visibility-only widening in `backends/rust/signature_rewrites/call_site.rs` and `signature_rewrites.rs`, `src/verify.rs` + `src/verify/retarget.rs`
  (the carrier retarget-impl builds), `src/restructure_args.rs`, `src/runner/options.rs`, `src/runner/comparison.rs`.
  Docs at wrap: new `docs/repoint-call.md`, [signature-rewrites.md](../../../packages/tddy-code-restructuring/docs/signature-rewrites.md) (cross-link),
  [docs/ft/coder/rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md) (`## Signature and call-site operations`, `## Verify`),
  [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md) (table row + section), SKILL.md operation count (dev-only).
- **`tddy-index-daemon`**: `proto/code_index.proto` (`VerifyRequest` gains `repeated string repoints`), `src/cli.rs`, `src/queries.rs` — carrier plumbing only.
- **`tddy-tools`**: `src/index_client.rs::verify` — carrier plumbing only. No operation is named anywhere in `tddy-tools`.

## Related Feature Documentation

- [PRD-2026-10-05-sharpen-repoint-call.md](../../ft/coder/1-WIP/PRD-2026-10-05-sharpen-repoint-call.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Signature and call-site operations`, `## Rust operations (v1)`, `## Verify`

## Summary

A new operation, `repoint_call`, rewrites the part of a call that is in front of its argument list and keeps the arguments. **Single form**: an item anchor with a
relative range over one call and a `callee` text (one path or method chain) that replaces the callee. **Bulk form**: an item anchor on a method, without a range, and a
`callee` template beginning `$receiver` that inserts hops after the receiver of every call of that method the server knows. `restructure verify` accepts a repeatable
`--repoint OLD=NEW` declaration so the re-pointed statements are accounted for.

## Background

After a method moves to another type, its callers change by a few tokens that no call-site operation expresses (`add_call_arg` and its three siblings edit argument lists only):
`self.common_room_slot(x)` becomes `self.peer_routing.common_room_slot(x)`; `x.m(..)` becomes `x.agent_roster().m(..)`. `#carve` 17/21 did 28 such sites and 7 wrappers by hand
(todo above). The engine already finds every reference to a method (`item_move.rs:141 sites_of`) and already parses one call out of a range (`call_site.rs:99 call_in`).

## Responsibility

- New `RefactorKind::RepointCall` (`repoint_call`), field `callee`, codec rules and the callee parser, `SUPPORTED`/`check`/`resolve` wiring (the #584 slice).
- **Single form**: replace the callee of exactly one call, arguments byte for byte.
- **Bulk form**: for every reference to a method, insert the template's hops after the receiver; refuse anything else it finds, all at once, before writing.
- Refusals at plan read (library level) and at `check`/`check --deep`/`apply` (text-level, before any server for the single form).
- `restructure verify --repoint OLD=NEW` through the carrier `retarget-impl` builds: CLI flag, `Options`, `verify::Declared`, `VerifyRequest`, daemon and CLI plumbing.
- Register the new live test binary in `.config/rust-e2e.filterset` **and** the `rust-analyzer` group of `.config/nextest.toml`.

## Plan-line schema (the contract)

```jsonl
{"op":"repoint_call","anchor":{"kind":"item","item":"app::host::Host::roster_work","file":"src/host.rs","start":{"line":3,"col":9},"end":{"line":3,"col":38},"fingerprint":"sha256:…"},"callee":"self.peer_routing.common_room_slot"}
{"op":"repoint_call","group":"retarget","anchor":{"kind":"item","item":"app::host::Host::common_room_slot","file":"src/host.rs","fingerprint":"sha256:…"},"callee":"$receiver.agent_roster().common_room_slot"}
```

| Field | Single form | Bulk form |
|---|---|---|
| `anchor` | `item` with `start` **and** `end`: the function containing the call, a range **relative to it over exactly one call** (`callee(args)` or `receiver.method(args)`) — the same anchor the four argument operations take | `item` with **neither** `start` nor `end`: the **method** whose references are re-pointed (an associated function that has a `self` receiver). Lowers to a zero-width range at its name |
| `callee` | the complete new callee, one chain (grammar below), **no** `$receiver` | a template: `$receiver` + one or more hops + `.` + **the method's own name** (a rename is `rename_symbol`) |
| `group` | allowed | allowed |
| every other field (`variant`, `name`, `to`, `reexport`, `type`, `expr`, `order`, `also`, `with_private_deps`, `to_file`) | refused: "cannot honour" | refused |

The form is read from the anchor's shape; there is no `variant`. `callee` is required.

**Callee grammar** (`plan::rust_syntax::one_callee`, parsed with `syn`, exactly one expression carrying no statement, as `one_expr` does): the outer
expression is a **path** (`a::b::f`, `a::b::f::<T>`, `<T as Tr>::f`) or a **field access** (`self.peer.f`, `self.agent_roster().f`, `x.0.f`); beneath it only paths, fields,
tuple indexes, method calls, calls, `(…)`, `&`, `*`, `?`, `.await` and indexing may appear, each argument itself statement-free. Refused, naming the text: an outer call or
method call (`f(x)`: the arguments stay with the call), a block, closure, `if`, `match`, `async`, macro, binary or assignment expression, a trailing method turbofish (`self.f::<T>` does
not parse as a chain), an empty string, two expressions, trailing text. In the bulk form the text is parsed with `$receiver` replaced by a sentinel identifier, which must occur **once, as
the leftmost segment**, followed by at least one hop; the last segment is read back and must equal the anchored method's name.

**Single-form rules** (`repoint_call/single.rs`, a pure function of the text and a range):
- the range must be exactly one call: `call_in` is reused unchanged, so "is not a call expression" and the argument-count `server_defect` are the existing refusals;
- the span replaced is everything before the argument list's `(`: the original callee, **including** a turbofish; the arguments, comments between arguments and the `(`…`)` are untouched;
- the **old callee must contain no call** (paths, fields, `self`, tuple indexes, `?`, `.await` only). `a.m(x).n(y)` is refused naming the inner call: its arguments would be dropped
  silently; re-point the inner call on its own range first. The *new* callee may contain calls: they are the author's text;
- an original method call with a turbofish (`recv.m::<T>(x)`) cannot be re-pointed by a field-chain callee (it could not restate `::<T>`): refused, naming the call; a path callee may carry its own;
- a callee equal to the current text re-points nothing and is refused;
- the old receiver is dropped when the new callee does not contain it (`self.dir_for(id)` -> `lookup::dir_for(id)`): that is the request, not an error. Arguments are never added or removed: compose with `add_call_arg`/`remove_call_arg` in one `group`.

**Bulk-form rules** (`repoint_call/sites.rs`, `receivers.rs`):
1. The anchored item must be a method (a `self` receiver, read from the item's text with `syn`); otherwise refused ("no method syntax to re-point").
2. References come from `RustBackend::sites_of` with the position of the lowered anchor; the server's whole workspace answers, including other packages, `tests/`, `examples/`.
3. Each site is classified from the text around the name token on the masked text: **method call** (`.name`, optional `::<…>`, then `(`) is re-pointed; a site inside a comment is skipped and counted
   in the note; **everything else in code** (a path call `Host::name(&h, 1)`, a function pointer `.map(Host::name)`, a `use`, a bare call) is refused. All refusals are collected and named
   `file:line` in **one** error, and nothing is written.
4. The receiver is the maximal postfix chain ending at the `.`: paths, fields, tuple indexes, method calls, calls, indexing, `?`, `.await`. A receiver that ends in a block or closure, or contains a
   comment, is refused. The candidate `receiver.name(args)` range is handed to `call_in`: `syn` must read it as one `Expr::MethodCall` whose method is `name`.
5. The edit is an **insertion** of the hops (`.agent_roster()`) after each receiver's end, so `a.m(b.m(1))` and `a.m().m()` compose without overlapping edits. Insertions in one file are applied last-first.
6. No references at all is a **no-op with a note**, not an error (O7).

**Refusal classes** (all `plan is malformed:` unless stated): missing `callee`; `callee` not one chain; `$receiver` in a single form, missing, repeated or not leftmost in a bulk form; a single anchor
without a range or a bulk anchor with one; a field the operation cannot honour; a single range that is not exactly one call (`this seam cannot be cut here:`); a callee equal to the current text; an old
callee containing a call; a turbofish the callee cannot restate; a non-method bulk anchor; a site that is not a method call (`this seam cannot be cut here:`, listing every site).

## Boundaries

- **Calls and receivers only. Developer decision (2026-10-05): "C2a: calls and receivers only."** The `self.field` -> `state.field` family stays **out** and stays the master todo
  [2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md](../todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md).
  The difference is not cosmetic: that edit reads a **field** (references of a field, not a method), must **insert** `let state = …;`, must **drop** a `&` the new binding makes redundant, and refuses unlisted
  fields. None of that is a call. This node adds a hop to a method call's receiver (`self` -> `self.peer`); it never rewrites a field read.
- **Not the delegator** and **not `retarget_impl`** (K=6): no forwarding method is written, no `impl` is edited, the method's declaration is untouched.
- **No argument edits**: `add_call_arg` and siblings exist; a group composes them. **No rename** of the called method.
- **Bulk form re-points method-call syntax only.** UFCS sites, function-pointer uses and imports are refused, not rewritten.
- **No `check --deep` site listing**: `Rehearsal` drops `Resolution.notes` today and the plumbing is `repoint-facade`'s (its acceptance needs it). The bulk form's count is a note printed by `apply`. No edge to K=8.
- **`verify` stays plan-less**: it learns a declaration, never a plan. `Excused` and the wire message gain **no counter**: the pairs are counted under `repointed`.
- **No `tddy-tools` or daemon behaviour change** beyond carrying the declaration. Out of scope: references in code rust-analyzer evaluates as inactive (it reports only the active cfg), macro-generated call sites; the compile gate names what results.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| **`tidy-engine-files`** (K=1, `feature/sharpen/tidy-engine-files`) | `plan.rs`, `plan/codec.rs`, `item_anchor.rs` each <= 500 production lines by child modules, no behaviour change. In particular `RefactorKind` and its `impl` now live in `plan/refactor_kind.rs` (its decision D1, developer-approved 2026-10-05), reachable at the old path through `pub use`; `RefactorOp` stays in `plan.rs`. **File overlap and layout only**: no signature is consumed | the `RepointCall` variant is added in `plan/refactor_kind.rs`; the `callee` field is added to `RefactorOp` in `plan.rs`; the codec call sits in the post-split `plan/codec.rs`; a new codec child `repoint_call_fields.rs` follows the child-module precedent; rebases onto it first | re-split those files, or lean on their headroom (O8) |
| **`retarget-impl`** (K=6, `feature/sharpen/retarget-impl`) | **The `verify` declaration carrier**: `verify::compare_with(before, after, &Declared)` with `Declared { retargets }`, `verify/retarget.rs` (rules R1 rename pairing, R2 header accounting, both counted in `repointed`), repeatable `--retarget OLD=NEW` (`RestructureVerifyArgs` -> `Options.retargets` -> `runner::verify`), `VerifyRequest.retargets = 3` and the `tddy-tools`/daemon plumbing. Also the `RefactorOp` field `to_type` and `RefactorKind::RetargetImpl` in `plan/refactor_kind.rs`/`rust.rs` | **extends** the carrier: `Declared.repoints: Vec<Repoint { from, to }>`, one rule **R-call** in `verify/retarget.rs` (or a sibling file; named `R-call`, not R3, because `retarget-impl`'s optional delegator milestone M4 already owns the name R3), `--repoint OLD=NEW`, `Options.repoints`, `VerifyRequest.repoints` (next free proto number after retarget-impl's; 4 or 5), same plumbing path. Rebases the `RefactorOp` field and `SUPPORTED` edits over its own | rebuild the carrier, change R1/R2, or touch the delegator todo's closure |

`repoint-facade` (K=8) sits above and is not consumed. The `RefactorOp` literals are paid for by each field's own node (developer-approved 2026-10-05): this node edits the 20 full struct literals in 15 files in its own first commit (`callee: None`, beside the `to_type: None` that `retarget-impl`, below it on the line, already added).

## Draft PR contract

Published with the wave-2 contract commit: **public surface that does not compile today**, and the failing tests below. Private helper names are a proposal that green may reshape, recording the change here.

- `tddy_code_restructuring::RefactorKind::RepointCall` (serde `repoint_call`), added in `plan/refactor_kind.rs`; `RefactorOp.callee: Option<String>` (serde `callee`, omitted when `None`), added in `plan.rs`; the 20 full `RefactorOp` struct literals in 15 files (in `src/` and `tests/`) gain `callee: None` in this node's first commit. Measured on `a77bca29`: `git grep -n 'RefactorOp {' -- packages | wc -l` is 80 textual sites (2 definitions, 41 function signatures, 17 `..base` forms that need no edit, 20 full literals); cross-check `git grep -n 'order: Vec::new()' -- packages/tddy-code-restructuring | wc -l` is 20 in 15 files.
- `plan::rust_syntax::one_callee(text: &str) -> Result<syn::Expr>` and `one_receiver_template(text: &str, method: &str) -> Result<Vec<String>>` (the hop texts).
- Crate-private: `backends::rust::repoint_call::{findings(&RefactorOp, &Workspace) -> Result<Vec<String>>, RustBackend::repoint_call(&mut self, &RefactorOp, &Workspace) -> Result<Resolution>}`,
  `single::rewrite_callee(text: &str, range: Range, callee: &str) -> Result<Vec<TextEdit>>`, `receivers::insertions_for(text: &str, site: usize, hops: &str) -> Result<usize /*insert offset*/>`.
- `tddy_code_restructuring::verify::Declared { retargets, repoints }`, `Repoint { from: String, to: String }`; `Options.repoints`; `RestructureVerifyArgs.repoint: Vec<String>` (`--repoint OLD=NEW`); proto `VerifyRequest.repoints`.
- Failing tests: the thirty in "Acceptance tests", red on `master` for the reason each states.

## Green wave

**Wave:** 2 of 2.
**Greenable independently:** yes, once `feature/sharpen/tidy-engine-files` and `feature/sharpen/retarget-impl` are on its base (the carrier is `retarget-impl`'s; without it the `verify` step has nothing to extend).
**Concurrent with:** `feature/sharpen/plan-header`, `feature/sharpen/repoint-facade` (same wave, no edge between them; the line serialises them because they share `plan.rs`, `plan/codec.rs`, `backends/rust.rs`, `verify.rs` and the `RefactorOp` literals).
**Blocks:** none (`feature/sharpen/repoint-facade` follows it on the line and consumes nothing from it).
Real dependency edges (whole stack): `tidy-engine-files -> plan-header, retarget-impl, repoint-call, repoint-facade`; `move-fidelity -> repoint-facade`; `retarget-impl -> repoint-call`. Nothing else is an edge: `spawn-record` and `apply-heartbeat` consume nothing and nothing consumes them (`spawn-record` lands after open draft PR #586, a merge-order fact, not a stack edge).

## Successor PRs

`feature/sharpen/repoint-facade` (K=8) follows in the line; it consumes nothing from this node.

## Scope

- [ ] **Plan surface**: `RepointCall` (in `plan/refactor_kind.rs`), `callee` (in `plan.rs`), `repoint_call_fields.rs`, `one_callee`, `one_receiver_template`; the 20 full literals in 15 files
- [ ] **Single form**: `repoint_call/single.rs`, `findings` for range anchors, resolve arm answered before any server
- [ ] **Bulk form**: `sites.rs`/`receivers.rs` over `sites_of`, collected refusals, insertion edits, the apply note
- [ ] **`verify`**: `Declared.repoints`, rule R-call, `--repoint`, request field, proto, daemon and CLI plumbing
- [ ] **Registration**: new live binary in `.config/rust-e2e.filterset` and the `rust-analyzer` group in `.config/nextest.toml`; `scripts/nextest-serial-groups.test.ts` still passes
- [ ] **Package documentation** at wrap (list under Affected Packages)
- [ ] **Testing**: the thirty acceptance tests pass; `./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools`, scoped; CI for the rest
- [ ] **Code quality**: `cargo check --all-targets` and `cargo clippy -p <touched> --all-targets -- -D warnings`, `cargo fmt`; `plan.rs`, `plan/codec.rs` <= 500 production lines (`restructure check --budget 500`, once at the end)

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

On `a77bca29` (re-read in Exploration 2):
- Argument-only call-site operations: `signature_rewrites/call_site.rs` (`rewrite_call :22`, `call_in :99`), private helpers, `edits_a_call_site` true for four kinds (`plan.rs:289`).
- `RefactorOp` has 14 fields, `deny_unknown_fields`, no `Default`; 20 full literals in 15 files list every field (measured: `git grep -n 'order: Vec::new()' -- packages/tddy-code-restructuring`).
- `sites_of` (`item_move.rs:141`) returns reference name offsets; a lowered item anchor without a range is a zero-width range at the name (`item_anchor.rs:166`).
- `verify::compare` has five passes; no pass pairs an inserted `.hop`; `Excused` is wire-visible (`queries.rs:305-307`).
- `check --deep` prints no notes (`rehearsal.rs`). A live binary needs two registrations.

### State B (Target)

`repoint_call` exists in both forms; `verify --repoint OLD=NEW` accounts for exactly the declared callee texts; nothing else changes.

### Delta (What's Changing)

#### `tddy-code-restructuring`
- **New** `backends/rust/repoint_call.rs` (run, `findings`), `repoint_call/single.rs` (~110 lines), `repoint_call/sites.rs` (~140), `repoint_call/receivers.rs` (~120): the backwards receiver walk.
- **Widened** (visibility only): `call_site::{Call, call_in}`, `signature_rewrites::{Span, Replacement, edits_of}` to `pub(in crate::backends::rust)`.
- **`backends/rust.rs`**: `mod repoint_call;`, `SUPPORTED` `[RefactorKind; N]` + one entry, a `check` arm, a `resolve` arm above `self.start(...)`.
- **`plan/refactor_kind.rs`** (the variant), **`plan.rs`** (the `callee` field), **`plan/codec.rs`**, **`plan/codec/repoint_call_fields.rs`**, **`plan/rust_syntax.rs`**: as in Responsibility.
- **`verify`**: rule R-call (below), `Declared.repoints`.

**R-call, declared call re-point pairing** (a pass beside retarget-impl's R1/R2, before pass 4): given `--repoint OLD=NEW` (callee texts; for the bulk form the method and its new hop, e.g.
`.common_room_slot=.agent_roster().common_room_slot`), a lost and a gained statement pair 1:1 when the gained equals the lost with every occurrence of `OLD` immediately followed by `(` (or `::<`)
replaced by `NEW`, outside strings, comments and lifetimes. Counted in `repointed`. A changed argument, a different method or an undeclared hop stays reported. Without a declaration `verify` is unchanged.
A single form that also changed arguments (`self.dir_for(id)` -> `lookup::dir_for(&self.root, id)`) is **outside** this proof and is reported.

#### `tddy-index-daemon`, `tddy-tools`
- Carrier plumbing for `repoints`, mirroring `retargets`. No other source change.

## Implementation milestones

- [ ] **M0** the first commit of code, mechanical: `RefactorOp.callee` and `callee: None` on the 20 full struct literals in 15 files; `cargo check -p tddy-code-restructuring --all-targets` clean (not `cargo build -p`: it skips the test targets that hold most of them)
- [ ] **M1** plan surface and codec: tests 1-12 pass (red first)
- [ ] **M2** single form: `single.rs`, `findings`, resolve arm; unit tests and tests 13-16
- [ ] **M3** bulk form: `sites.rs`, `receivers.rs`; tests 17-25
- [ ] **M4** registration of the live binary in both files
- [ ] **M5** `verify` R-call and `--repoint` through the carrier; tests 26-30
- [ ] **M6** docs staged; **M7** scoped gate and length gate

## Testing plan

### Testing Strategy

**Primary approach, three levels by what each needs.** Text-only rules in milliseconds (inline unit tests and a library binary over `Plan::parse` and a static `runner::check`); everything that
lowers an item anchor or asks for references against a live rust-analyzer fixture crate with `cargo check` as the assertion ("tens of seconds each", `SKILL.md:105`); `verify` at library level.

#### Option 1 (chosen): live fixture crate, `cargo check` as the oracle — bulk form, the single form end to end, groups
Fixture: one package `app` (`same_crate::an_app_holding`), plus `an_app_with_a_consumer` for another crate. **Trade-off**: slow (a server per test, serialised) but the only level at which "every reference" is the server's answer and the tree must compile.
**Location**: `packages/tddy-code-restructuring/tests/repoint_call_acceptance.rs` (new live binary; `.config/rust-e2e.filterset` + `.config/nextest.toml` group).

#### Option 2 (chosen): library level, no server
`Plan::parse` and static `runner::check` over range anchors (the static tier judges only those), plus inline unit tests of the pure functions. **Location**: `tests/repoint_call_plan_acceptance.rs`, `src/backends/rust/repoint_call/{single,receivers}.rs` (`#[cfg(test)]`), `tests/verify_accounts_for_declared_repoints.rs`.

#### Option 3 (rejected): a fake language server for the bulk form
It would make the bulk test fast and prove nothing about what rust-analyzer reports as a reference — the whole point of the form.

### Coverage Requirements

- [ ] Happy paths: field hop, path callee, hop through a no-arg call, other crate, `tests/` file
- [ ] Refusals: every row of "Refusal classes"
- [ ] Edge cases: nested and chained calls, strings and comments containing `(`/`,`, multi-line calls, a macro argument, a no-reference method
- [ ] Actual effects: bytes on disk and `cargo check`, not return values alone

## Acceptance tests

Names read as behaviour specifications. All are **red on `master`** (`unknown variant repoint_call` or a missing symbol unless stated).

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/repoint_call_plan_acceptance.rs` (new; library level, no server)

1. `a_repoint_call_with_a_callee_over_a_call_is_a_plan` — single-form JSON parses. *Fails today*: unknown variant.
2. `a_repoint_call_without_a_callee_is_refused_naming_the_field`.
3. `a_callee_that_is_not_one_path_or_method_chain_is_refused_naming_it` — table: `f(x)`, `a.b()`, `a + b`, `{ a }.b`, `|x| x`, `a.b; c.d`, `a::b!()`, ``, `self.m::<T>`.
4. `a_callee_may_hold_paths_fields_calls_and_a_qualified_self_type` — accepts `self.peer.f`, `self.agent_roster().f`, `a::b::f::<T>`, `<Host as Tr>::f`, `(*self.x).f`, `x.0.f`.
5. `a_bulk_callee_must_start_with_the_receiver_and_end_with_the_methods_own_name` — refuses no `$receiver`, bare `$receiver`, `x.$receiver.m`, two `$receiver`, a different last segment; accepts `$receiver.peer.m`, `$receiver.a().m`.
6. `a_single_callee_may_not_name_the_receiver_placeholder`.
7. `a_single_anchor_without_a_range_and_a_bulk_anchor_with_one_are_refused`.
8. `repoint_call_refuses_every_field_it_cannot_honour` — `variant`, `name`, `to`, `reexport`, `type`, `expr`, `order`, `also`, `to_file`, `with_private_deps`.
9. `callee_is_refused_on_every_other_operation` — a table of five other kinds.
10. `a_static_check_of_a_range_anchored_repoint_call_over_text_that_is_not_one_call_is_refused_with_no_server` — v1 plan, range over `f(a) + g(b)`; finding contains `is not a call expression`.
11. `a_static_check_refuses_a_callee_equal_to_the_current_one_and_an_old_callee_holding_a_call`.
12. `a_static_check_of_an_item_anchored_repoint_call_says_to_run_deep` — the existing "anchors by item" finding names the kind.

### `tddy-code-restructuring` — inline, `packages/tddy-code-restructuring/src/backends/rust/repoint_call/single.rs` and `receivers.rs` (`#[cfg(test)]`, pure text)

13. `single_replaces_only_what_precedes_the_argument_list` — arguments with nested calls, `","`, `")"` in strings, a `//` comment between arguments: bytes after the `(` identical.
14. `single_turns_a_method_call_into_a_field_chain_call_and_keeps_the_receiver_only_if_the_callee_names_it`.
15. `single_refuses_an_old_callee_holding_a_call_a_turbofish_the_callee_cannot_restate_and_a_noop`.
16. `the_receiver_walk_stops_at_the_operator_that_binds_looser` — `&x.m()`, `-x.m()`, `a + b.m()`, `x?.m()`, `fut.await.m()`, `v[0].m()`, `(a + b).m()` (parens belong to the receiver).
17. `a_receiver_ending_in_a_block_or_holding_a_comment_is_refused`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/repoint_call_acceptance.rs` (new live binary; fixture `app`, live rust-analyzer, `assert_compiles_with_its_tests`)

18. `a_call_is_re_pointed_through_a_field_and_the_crate_still_compiles` — `self.slot(x)` -> `self.peer.slot(x)`; every other call, comment and line byte-identical.
19. `a_path_call_is_re_pointed_to_another_module_and_its_arguments_are_kept` — `slot(a, f(b))` -> `lookup::slot(a, f(b))`.
20. `an_argument_added_and_a_callee_re_pointed_on_one_call_compile_as_one_group` — `add_call_arg` then `repoint_call` on the same call, one group (probe: if the ledger cannot translate the anchor, the documented order is arguments first).
21. `every_method_call_of_a_method_has_its_receiver_re_pointed_across_files` — `host.slot(1)`, `self.slot(2)`, `h.a().slot(3)`, one inside `format!`, in three files; receivers gain `.peer`; compiles.
22. `the_bulk_form_edits_a_caller_in_another_crate_and_a_test_binary`.
23. `a_nested_and_a_chained_call_of_one_method_are_both_re_pointed_without_overlap` — `a.m(b.m(1))`, `a.m().m()`.
24. `a_site_that_is_not_a_method_call_is_refused_naming_every_one_and_nothing_is_written` — `.map(Host::slot)`, `Host::slot(&h, 1)`, bare `use`; tree byte-identical.
25. `a_reference_in_a_comment_is_left_alone_and_a_method_nothing_calls_is_a_no_op_with_a_note` (result independent of the B3 probe).
26. `a_deep_check_refuses_what_an_apply_refuses_and_finds_nothing_in_a_good_plan` — `checking_the_plan(.., deep = true)`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/verify_accounts_for_declared_repoints.rs` (new; library level over `verify::compare_with`)

27. `a_declared_receiver_hop_is_accounted_for_and_counted_as_repointed`.
28. `an_undeclared_hop_a_replaced_hop_and_a_changed_argument_are_still_reported`.
29. `a_declared_callee_text_never_pairs_a_different_method` — `.slot` vs `.slots`.

### `tddy-index-daemon`, `tddy-tools`

30. `verify_carries_a_declared_repoint_through_the_cli_and_the_daemon_and_both_render_the_same_lines` — `packages/tddy-index-daemon/tests/dual_transport_acceptance.rs` (existing, already in `.config/rust-e2e.filterset`), beside the `--retarget` case `retarget-impl` adds to it; *fails today*: `unexpected argument '--repoint'`. (The daemon unit test in `packages/tddy-index-daemon/src/cli.rs` gains the field, as retarget-impl's does.)

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Taken by the developer (stack brief, quoted): "8-node decomposition approved (2026-10-05)."; "C2a: calls and receivers only."; "Prep node: ADD the mechanical node first."; "Log-history fix is its own PR #586 — NOT in this stack."

**OPEN** (not settled by the brief; each with a recommendation):
- **O1 — field count.** (a) **one field `callee`**, both forms, form read from the anchor shape — *recommended*: each field is a one-line edit in the 20 full literals in 15 files, paid in this node's own first commit; (b) two fields (`callee`, `receiver`), as the whole-work discovery assumed: clearer names, double the literals; (c) reuse `expr`: mis-named, and `expr` is a different parser's.
- **O2 — spelling of the bulk form.** (a) **anchor without a range + `$receiver` template** — *recommended*; (b) a `variant: "references"`: redundant with the anchor shape.
- **O3 — what the bulk form does with a non-call reference.** (a) **refuse all of them at once, writing nothing** — *recommended*; (b) skip and report: the plan author then thinks it was complete. Comments are skipped under both.
- **O4 — receiver replacement vs insertion in the bulk form.** (a) **insertion of hops only** — *recommended*, composes without overlap; (b) full receiver replacement: needs overlap rules for chains.
- **O5 — how `verify` accounts for it.** (a) **a declaration, `--repoint OLD=NEW`, extending retarget-impl's carrier** — *recommended*: consistent, auditable, a typo excuses less; (b) infer insertion-only pairs with no flag: no plumbing, but it excuses any statement that gained a hop, declared or not; (c) state the op is outside `verify`'s proof: noisy (one lost and one gained per site).
- **O6 — a wire counter.** (a) **count under `repointed`, no new field** — *recommended*; (b) a new `Excused` field: proto and three packages.
- **O7 — a bulk form with no reference.** (a) **success with a note** — *recommended* (a re-run is harmless); (b) refuse.
- **O8 — headroom (answered by `tidy-engine-files` D1, decided 2026-10-05).** `RefactorKind` now lives in `plan/refactor_kind.rs`, so `plan.rs` has about 167 lines of headroom (before `retarget-impl`'s and `move-fidelity`'s fields) and the variant lands in a file with about 300. Residual: measure `plan.rs`, `plan/refactor_kind.rs` and `plan/codec.rs` at green; if one would cross 500, move the codec rules into a child module in this node rather than cross it.
- **O9 — composing with argument operations.** Whether the ledger translates the anchor of a second operation over a call an earlier operation edited inside its range is **unverified**; test 20 decides, and the doc states the working order.
- **O10 — reach of the bulk form.** (a) **every package the server knows** — *recommended*, "every reference"; (b) the anchor's package only.

Decisions taken by this plan: single form reuses `call_in`; the old callee may hold no call (silent argument loss otherwise); the same-statement-count wire shape; the form is read from the anchor.

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
(empty)

### From @red (TDD Red Phase)
(empty)

### From @validate-changes (Change Validation)
(empty)

### From @validate-tests (Test Quality)
(empty)

### From @prod-ready (Production Readiness)
(empty)

### From @analyze-clean-code (Code Quality)
(empty)

### From @refactor (Completed Refactorings)
(empty)

## Validation Results

(empty; populated by `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`)

## TODO

- [x] Record initial discovery (`2026-10-05-sharpen-repoint-call-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-05-sharpen-repoint-call.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the scoped tests (`./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools`) — verify 100% pass; CI answers for the rest of the workspace
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <touched> --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-05-sharpen-repoint-call-initial-discovery.md`; the #532 delegator/receiver todo is deleted by `retarget-impl`
- [ ] USER REVIEW — work complete, decide next steps
