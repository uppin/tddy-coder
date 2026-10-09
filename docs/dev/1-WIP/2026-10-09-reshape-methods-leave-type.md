# Changeset: method bodies leave a type that stays — `read_fields_through` and `retarget_impl`'s forwarding delegator

**Date**: 2026-10-09
**Status**: 🚧 In Progress
**Type**: Feature (one new restructure operation, one refused variant implemented, two `verify` rules, one `verify` declaration)
**Stack**: `#reshape` 6/19, branch `feature/reshape/methods-leave-type`, wave 1. PR title:
`feat(code-restructuring): method bodies read fields through a state value and retarget_impl leaves delegators (#reshape 6/19)`.
Base in the linear stack: `feature/reshape/move-children` (K=5). **Real edges**: none in. Out: `methods-leave-type -> backend-session` (K=18 consumes self mode).
Nodes 1-12 share `backends/rust.rs` wiring and `plan/codec.rs`. They collide in the text only; no behaviour is shared.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-09-reshape-methods-leave-type-initial-discovery.md)
(Exploration 1 is the whole-work discovery; Exploration 2 is this node's).

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

`grep -rl 'Claimed by:'` over `packages/tddy-code-restructuring/docs/code-issues/` finds no record that
names `retarget_impl`, `verify` or the new operation's area, so **no claimed issue is in the path and
there is no wait-or-proceed fork.**

| Item | Verdict | What this change does about it |
|---|---|---|
| [2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md](../todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md) | ✅ **RESOLVED HERE** | `read_fields_through`, in field mode. Two parts of the entry's sketch are deliberately not built: its `fields` list (redundant, F1) and its `builder` method name (`expr` generalises it, F1). The `&`-drop and the "one auto-ref" rule are in scope. Entry deleted at wrap |
| [2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md](../todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md) | ✅ **RESOLVED HERE** | The receiver half landed in #594 (`repoint_call`). This node builds the delegator half (`leave_delegator`, P9, S7, R3) and the dead-delegator note. The entry's generic "report any wrapper with no caller left" is already met for private items in touched files by the tidy's `warning remains: … is never used` (`runner/tidy.rs:525-536`). The `pub` case moves to a new todo, `2026-10-09-restructure-dead-pub-wrappers-are-not-reported.md`. Entry deleted at wrap |
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` ([record](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md)) | ⚠ **DURING** | Wiring only in `backends/rust.rs`: `mod read_fields_through;`, one `SUPPORTED` entry, one `check` arm and one `resolve` arm (about 10 lines). All logic goes in `backends/rust/read_fields_through/` and `backends/rust/retarget_impl/delegator.rs`. A history row is added at wrap. `feature/reshape/rust-backend-split` closes the record |
| [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **DURING** | Same wiring, same 10 lines |
| Function-size list (whole-work discovery, Exploration 3): `retarget_impl` 70 lines (`backends/rust/retarget_impl.rs:79`), `parse_op` 206 lines (`plan/codec.rs:246`) | ⚠ **DURING** (owned by `fn-sizes-backend` / `fn-sizes-rest`) | Neither function grows (binding decision). `retarget_impl()` loses lines 121-132 to a new `the_replacement` helper, which is where the delegator branch goes. `parse_op` gains exactly one call line. All new logic is in new functions |
| [2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type.md](../todo/2026-10-07-restructure-retarget-impl-refuses-a-relative-import-of-its-target-type.md) | — Unrelated (node 11's) | `retarget_impl/imports.rs` is not touched |
| Other `docs/code-issues/*` (`broken-restructure-anchors-empty-outline.md`, `complexity-rust-facade-lines.md`, `dead-code-plan-filehint-modified.md`, `oversized-file-test-binary.md`) | — | Not in the path |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md).
  - Plan surface:
    - `src/plan/refactor_kind.rs`: the `ReadFieldsThrough` variant.
    - `src/plan/codec.rs`: one `mod` and one call.
    - New `src/plan/codec/rebind_fields.rs`.
    - `src/plan/codec/retarget_fields.rs`: P9.
    - `src/plan/codec/signature_fields.rs`: `expr` is admitted on the new op.
    - `src/plan.rs`: doc lines only on `name`/`expr`; **no new field**.
  - Backend:
    - `src/backends/rust.rs`: wiring.
    - New `src/backends/rust/read_fields_through.rs` and its children `read_fields_through/{range,typing,edits}.rs`.
    - New `src/backends/rust/retarget_impl/delegator.rs`.
    - `src/backends/rust/retarget_impl.rs`: the deferred refusal is removed and `the_replacement` is extracted.
    - `src/backends/rust/retarget_impl/rewrite.rs`: `Layout::with_delegators`.
  - Verify: `src/verify/retarget.rs` (R3 and the `rebinds` field of `Declared`), new `src/verify/rebind.rs`, `src/verify.rs` (pass order).
  - Front end: `src/restructure_args.rs` (`--rebind`), `src/runner/options.rs`, `src/runner/comparison.rs`.
- **`tddy-index-daemon`**: [README.md](../../../packages/tddy-index-daemon/README.md).
  - `proto/code_index.proto`: `VerifyRequest.rebinds = 5`.
  - `src/cli.rs` and `src/queries.rs`: pass-through.
  - `tests/dual_transport_acceptance.rs`.
- **`tddy-tools`**: `src/index_client.rs` carries `rebinds`.
- **Config**: `.config/nextest.toml` (`rust-analyzer` group) and `.config/rust-e2e.filterset` gain the two new live binaries.
- **Docs at wrap**:
  - New `packages/tddy-code-restructuring/docs/read-fields-through.md`.
  - [retarget-impl.md](../../../packages/tddy-code-restructuring/docs/retarget-impl.md): the delegator section, and its limits line removed.
  - [repoint-call.md](../../../packages/tddy-code-restructuring/docs/repoint-call.md): the "stays the open state-parameter todo" limit.
  - [rust-code-restructuring.md](../../ft/coder/rust-code-restructuring.md): `## Rust operations (v1)`, `### retarget_impl`, `## Verify`, `## CLI`.
  - [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md), `SKILL.md` (operation count 25 → 26, and the recipe: `read_fields_through` → `extract_method` → `extract_module` → move).

## Related Feature Documentation

- [PRD-2026-10-09-reshape-methods-leave-type.md](../../ft/coder/1-WIP/PRD-2026-10-09-reshape-methods-leave-type.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md): `## Rust operations (v1)`, `### retarget_impl`, `## Verify`

## Summary

**`read_fields_through`** is anchored on a range inside a method. It inserts `let <name> = <expr>;`
before the range and rebinds `self` in the range to `<name>`. It has two modes:

- **Field mode** (`expr` is not `self`): only `self.<field>` reads are rewritten. Each field is checked
  against the state value's type, using the server's answers on a trial text. A `&` the state already
  provides is dropped, so clippy's `needless_borrow` does not fire. Method calls on `self` are refused.
- **Self mode** (`expr` is `self`, `&*self` or `&mut *self`): every `self` is rewritten, method calls
  included. A following `extract_method` then writes a free function.

**`retarget_impl` with `variant: "leave_delegator"` and `expr`** moves the members as today. It leaves
a forwarding method in each moved method's slot in the old block, and the moved members follow in an
`impl New` block. Delegators that nothing calls are noted.

**`restructure verify`** accounts for both:

- R-rebind, through the new `--rebind NAME` declaration;
- R3, through the existing `--retarget OLD=NEW` declaration.

## Background

In the `#carve` 15 T3 port-move pilot, every `self.<field>` became `state.<field>` by hand. `#carve` 17
hand-wrote 7 forwarding delegators, and only `dead_code` found 2 dead wrappers. `#sharpen` 6/8 designed
the delegator (its M4) and cut it at the size decision point. The schema is published, and the engine
refuses it (`retarget_impl.rs:38-48,61-66`).

`#reshape` 18 (`feature/reshape/backend-session`) has to turn `RustBackend` operations into free
functions. Those bodies are almost all `self.<method>()` calls (discovery § 5), and a field-only
operation would refuse them. That is why self mode is in this node (developer decision, 2026-10-09).

## Responsibility

- New `RefactorKind::ReadFieldsThrough` (`read_fields_through`), its codec rules, and its
  `SUPPORTED`/`check`/`resolve` wiring.
- Field mode and self mode, as the rules below define them. Static findings for every lexical rule,
  and server-informed refusals for typing, all before anything is written.
- The `retarget_impl` delegator:
  - P9 at parse time and S7 before any write;
  - the forwarding text and its placement;
  - the dead-delegator note;
  - removing `deferred_delegator` and its `TODO(retarget-impl)`.
- `verify`: R3, R-rebind, and the `--rebind` declaration through the library, the CLI, `tddy-tools`
  and the daemon.
- Registering the two new live binaries, and only those (developer decision: node 1 registers the
  pre-existing unregistered suites).

## Plan-line schema and the rules (the contract)

### `read_fields_through`

```jsonl
{"op":"read_fields_through","anchor":{"kind":"range","file":"packages/tddy-session-lifecycle/src/connection_service/svc_provision_agent_clone.rs","start":{"line":375,"col":9},"end":{"line":392,"col":15}},"name":"state","expr":"self.agent_roster_state()"}
{"op":"read_fields_through","anchor":{"kind":"item","item":"tddy_code_restructuring::backends::rust::RustBackend::retarget_impl","file":"…/retarget_impl.rs","start":{"line":9,"col":9},"end":{"line":69,"col":10},"fingerprint":"sha256:…"},"name":"backend","expr":"self"}
```

| Field | Required | Meaning |
|---|---|---|
| `anchor` | yes | A `range`, or an `item` anchor **with** a relative range (lowered to a range, as for `extract_method`). A `symbol`, an `items` anchor, or an `item` anchor without a range is refused (RP3) |
| `name` | yes | The new binding: one identifier, not `self`, not a keyword (RP1) |
| `expr` | yes | One `syn::Expr` (existing `one_expr`). Written verbatim as the `let`'s initialiser. `self`, `&*self` or `&mut *self` (compared as tokens) select **self mode**; anything else selects **field mode** |
| `id`, `group` | as on every op | Composes with `extract_method` in one `group` |

The operation does not define `to`, `to_type`, `variant`, `callee`, `type`, `order`, `reexport`,
`also`, `to_file`, `canonical_paths` or `with_private_deps`, so each is refused (RP4). The fields that
existing rules already refuse keep their existing messages; `to` and `variant` are new in
`rebind_fields.rs`.

**Refusals.** The `plan is malformed:` class, reported by plain `check`, `check --deep` and `apply`:

| # | Message stem |
|---|---|
| RP1 | `` `read_fields_through` needs `name`: the binding the range reads `self` through `` / `` `name` must be one identifier other than `self`; `{name}` is not `` |
| RP2 | `` `read_fields_through` needs `expr`: the value `{name}` is bound to `` |
| RP3 | `` `read_fields_through` anchors on a range (a `range`, or an `item` with a relative range): `{kind}` names no statements to rebind `` |
| RP4 | `` `{field}` is not a field of `read_fields_through` `` |

The `this seam cannot be cut here:` class. RS1-RS4 read only text, so they are also **static
findings** for a `range` anchor (plain `check`). RS5-RS7 need the server and are reported by
`check --deep` and `apply`. Every one is raised before any edit is built:

| # | Message stem | When |
|---|---|---|
| RS1 | `` the range starts at line {n} col {c}, which is not the start of a statement: a `let` cannot be inserted before it `` | The code byte before the range start, comments and strings masked (`masked_to_code`), is not `;`, `{` or `}` |
| RS2 | `` the range names no `self`: there is nothing to rebind `` | — |
| RS3 | `` the range calls `self.{m}(…)` at line {n} and uses `self` bare at line {k}: read through `{name}` only fields, or rebind every `self` with `"expr": "self"` `` (every site listed) | Field mode only |
| RS4 | `` `{name}` is already written at line {n} of this function: the new binding would shadow it `` | `name` occurs as a whole word in the masked body of the enclosing `fn` |
| RS5 | `` the range is in no method with a `self` receiver `` | Outline: the enclosing symbol is not a method (kind 6), or its parameter list has no `self` |
| RS6 | `` `{expr}` has no field {f, g}: the range reads `self.f` and `self.g` `` | Field mode. In the trial text, `unresolved_names` (semantic tokens) reports `<name>.<f>` as unresolved |
| RS7 | `` `self.{f}` is `{T}` and `{name}.{f}` is `{U}`: they differ by more than one reference `` | Field mode. Hover types with lifetimes stripped: `U == T`, or `U == &T` / `&mut T`, is accepted; anything else is refused |

A same-typed field is accepted whether the state holds it by value or by clone. Types cannot tell the
two apart, and the docs say so.

The `rust-analyzer's answer was unusable:` class covers a hover with no `name: Type` line in its code
block.

**Edit** (one `FileEdit::Change`, as minimal edits via `seam_survey::minimal_edits`):

1. Insert `let <name> = <expr>;\n<indent>` at the range start. `<indent>` is the range-start line's
   leading whitespace.
2. **Field mode:**
   - every `self.<field>` site (a `self` token, then `.`, then an identifier not followed by `(` or
     `::<`, in masked code) becomes `<name>.<field>`;
   - `&self.<field>` / `&mut self.<field>` becomes `<name>.<field>` when RS7 found `U` to be a
     reference;
   - otherwise the `&` stays.
3. **Self mode:** every `self` token in masked code in the range becomes `<name>`, and every `Self`
   token becomes the enclosing impl's self type, as written in its header, generics included (read from
   the outline RS5 already requests).
4. A comment, a string, and anything outside the range is byte-identical. The original text is sent
   back to the server after the trial (`did_change`, the pattern at `imports.rs:56`).

**Note** (`Resolution.notes`, printed by `apply` and `check --deep`):
`read_fields_through: rebound 4 reads of self through `state` (fields session_agent_rosters, session_agent_clones); dropped 1 borrow`.
In self mode the note reads `… rebound every self (7 sites) through `backend``.

**Limits (to be written in the docs):**

- In field mode, `Self` (the type) in the range is not rewritten. Self mode rewrites it.
- A borrow conflict, or a `!Send` borrow alive across `.await`, is left to the compile gate.
- A field read that a macro generates (not written in the range) is invisible.
- One `expr` for the whole range.

### `retarget_impl` + `leave_delegator`

```jsonl
{"op":"retarget_impl","anchor":{"kind":"items","file":"src/host.rs","items":["app::host::Host::get","app::host::Host::put"],"fingerprints":["sha256:…","sha256:…"]},"to_type":"app::roster::Roster","variant":"leave_delegator","expr":"self.roster"}
```

**P9** (`plan is malformed:`, in `retarget_fields.rs`):

- `` `variant: "leave_delegator"` needs `expr`: the expression that reaches the new type from `self` ``
- `` `expr` on `retarget_impl` is the delegator's receiver, so it needs `variant: "leave_delegator"` ``

**S7** (`this seam cannot be cut here:`, `delegator.rs`). Raised before any edit, after S4. One
refusal lists every member that cannot be forwarded:

- `` `{member}` takes `{pattern}` as a pattern, so a forwarding method cannot name it ``
- `` `{member}` is an associated const or type: it has no body to forward ``
- `` `{member}` is a `const fn`, which cannot forward to a non-const call ``

**Placement (F5):**

- The old block keeps its header and every member that stays.
- Each moved member's byte range (its attached trivia included) is replaced, **in place**, by its
  delegator.
- `impl <New> {` + the moved members' bytes + `}` is inserted after the old block's closing `}`,
  separated by one blank line.
- A whole-block retarget gives `impl Old { <delegators> }` then `impl New { <members> }`.
- The path re-points (`Old::m` → `New::m` inside the moved members) are unchanged.

```rust
impl Host {                                   impl Host {
    pub fn get(&self) -> u32 { self.n }           pub fn get(&self) -> u32 {
    /// Replace the count.                            self.roster.get()
    #[inline]                                     }
    pub fn put(&mut self, v: u32) { … }           #[inline]
    pub fn size(&self) -> u32 { … }               pub fn put(&mut self, v: u32) {
}                                                     self.roster.put(v)
                                                  }
                                                  pub fn size(&self) -> u32 { … }
                                              }

                                              impl Roster {
                                                  pub fn get(&self) -> u32 { self.n }
                                                  /// Replace the count.
                                                  #[inline]
                                                  pub fn put(&mut self, v: u32) { … }
                                              }
```

**A delegator:**

- **Outer attributes:** every one except doc comments (`///`, `//!`, `#[doc]`), byte for byte.
- **Signature:** the member's text from its visibility (or `fn`/qualifier) to the body's opening `{`,
  byte for byte. The body's `{` is found by matching back from the member's final `}` in masked text.
- **Body:** on its own line, at the member's indentation plus four spaces. It is
  `<expr>.<name>(<arg names>)`, with `.await` appended when the signature is `async`. It is
  `<New>::<name>(<arg names>)` when the member has no receiver.
- **Arguments:** the parameter names, read with `syn` from the parsed member, in order.
- **Members without a receiver** are written as `<New>::…` regardless of `expr`.

**Dead-delegator note (F6).** `sites` (`retarget_impl.rs:120`, every reference to every moved member,
in every file) minus the sites inside the moved range gives each member's outside callers. A member
with none gets its delegator and the note
`retarget_impl: the delegator `Host::put` has no caller in the workspace: remove it, or retarget without leave_delegator`.
No extra server request is made.

### `verify`

- **`--rebind NAME`** (repeatable, `verify::Rebind`, `Declared.rebinds`) declares that `self` was
  rebound to `NAME`.
  - **R-rebind** (`verify/rebind.rs`) runs after R1/R2 and before R-call.
  - A lost and a gained statement pair 1:1 when the gained one equals the lost one with every
    whole-identifier `self` (outside strings, comments and lifetimes) replaced by `NAME`, also allowing
    one removed `&` directly before a replaced `self.` (the dropped borrow).
  - Then **one** gained statement `let NAME = …;` is excused per declaration.
  - `NAME` must be one identifier other than `self`.
- **R3** (in `verify/retarget.rs`, for each declared `--retarget OLD=NEW`):
  - a gained statement that is a `fn` signature line (ends `{`, holds `fn `) equal to a statement the
    ref already has is excused once per such ref statement;
  - a gained statement `<anything>.<m>(<p1, …, pn>)`, its `….await` form, or `NEW::<m>(…)` is excused
    when `m` and the parameter names are exactly those of an excused signature;
  - both are counted 1:1 into `repointed`.
- **Still reported:**
  - a rebind nobody declared;
  - a second `let`;
  - a changed argument;
  - a forwarding method whose arguments differ from the moved signature.
- **Honest limit:** `verify` proves "the shape a declaration produces", not that the plan produced it.
- **Carriers:**
  - `RestructureVerifyArgs.rebind: Vec<String>` (`--rebind`, `value_name = "NAME"`);
  - `runner::Options.rebinds`;
  - `runner/comparison.rs` builds `Declared` with them;
  - `VerifyRequest.rebinds = 5`;
  - `tddy-tools` `index_client.rs::verify`;
  - daemon `cli.rs` / `queries.rs`.

## Boundaries

- No `RefactorOp` field is added, so none of the 22 struct literals is edited.
- `read_fields_through` writes **no** state type, builder method or free function, and moves nothing.
  The state type and its builder are wiring, written by hand by design (state-parameter entry).
  Extraction is `extract_method`'s job.
- `retarget_impl` without the variant is unchanged. `repoint_call`, `extract_method`, `extract_module`
  and the crate moves are untouched.
- No generic dead-wrapper survey: the tidy already reports private ones, and the `pub` case is
  deferred (new todo).
- `retarget_impl()` (70 lines) and `parse_op` (206) do not grow. New logic goes in new functions.
- The pre-existing unregistered live suites are not registered here; that is node 1's job.
- No new dependency. In particular, `proc-macro2`'s `span-locations` is not enabled: sites come from
  masked lexical scanning, and typing comes from the server.

## Dependencies

This node has no parent in the stack: it consumes nothing a lower `#reshape` node delivers, and its
base, `feature/reshape/move-children`, is a line position only.

## Draft PR contract

The first push of this PR publishes this **owned surface** (exact signatures) and the failing tests
below:

- `RefactorKind::ReadFieldsThrough` (serde `read_fields_through`), in `plan/refactor_kind.rs`.
- `plan::codec::rebind_fields::refuse_a_rebind_it_cannot_honour(op: &RefactorOp) -> Result<()>`
  (`pub(super)`). P9 is added inside the existing `retarget_fields::refuse_a_retarget_it_cannot_honour`
  (its signature is unchanged).
- `backends::rust::read_fields_through` (declared `pub(crate) mod read_fields_through;` with
  `pub(crate) mod range; pub(crate) mod edits;`, so that `#reshape` 18's `detach_method` can reuse the
  self-mode rewrite from a sibling module):
  - `pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>>`
  - `impl RustBackend { pub(super) fn read_fields_through(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution> }`
  - `range`:
    - `pub(crate) enum Mode { Fields, Receiver }`
    - `pub(crate) fn mode_of(expr: &str) -> Mode`
    - `pub(crate) struct SelfSite { pub(crate) offset: usize, pub(crate) usage: SelfUse }`
    - `pub(crate) enum SelfUse { Field { name: String, borrowed: Option<std::ops::Range<usize>> }, Method(String), Bare, SelfType }`
    - `pub(crate) fn self_sites(text: &str, range: std::ops::Range<usize>) -> Vec<SelfSite>`
    - `pub(crate) fn lexical_refusals(text: &str, range: std::ops::Range<usize>, name: &str, mode: Mode) -> Vec<String>` (RS1-RS4)
  - `typing`:
    - `pub(super) fn field_type(hover: &serde_json::Value) -> Result<String>`
    - `pub(super) enum Agreement { Same, ByReference, Differs }`
    - `pub(super) fn agreement(host: &str, state: &str) -> Agreement`
  - `edits`: `pub(crate) fn rebound(text: &str, range: std::ops::Range<usize>, name: &str, expr: &str, self_type: &str, sites: &[SelfSite], drop_borrow: &BTreeSet<String>) -> String` (`self_type` is used only in self mode)
- `backends::rust::retarget_impl::delegator`:
  - `pub(super) fn refuse_unforwardable(members: &[(&str, &str)]) -> Result<()>` (S7)
  - `pub(super) fn forwarding_method(member_text: &str, expr: &str, new_type: &str) -> Result<String>`
  - `pub(super) fn dead_delegators(sites: &[Site], file: &str, moved: std::ops::Range<usize>, members: &[&str], old_type: &str) -> Vec<String>`
- `retarget_impl::rewrite::Layout::with_delegators(&self, moved: &str, delegators: &[String]) -> String`.
- `retarget_impl::the_replacement(…) -> Result<(String, Vec<String>)>`: extracted from
  `retarget_impl()` lines 121-132; returns the text and the notes.
- `verify`:
  - `pub struct Rebind { pub name: String }`
  - `impl FromStr for Rebind`
  - `Declared.rebinds: Vec<Rebind>`
  - `Declared::with_rebinds<'a>(self, rebinds: impl IntoIterator<Item = &'a String>) -> Result<Declared, String>`
  - re-exported as `verify::{Rebind}` beside `Declared`, `Retarget`.
- `RestructureVerifyArgs.rebind: Vec<String>` (`--rebind`). `runner::Options.rebinds: Vec<String>` and
  the `--rebind` arm in `Options` parsing.
- `tddy-index-daemon` `VerifyRequest.rebinds` (field 5).

Failing tests: acceptance tests 1-27 and 29-30 below are red on the first push. Test 28 is the
regression pin, green.

## Green wave

**Wave:** 1 of 4.
**Greenable independently:** yes. It has no parent, and its base's content is irrelevant to it.
**Concurrent with:** nodes 1-5 and 7-12 (`widen-same-crate` … `anchors-outline`). They share text in
`backends/rust.rs`, `plan/codec.rs` and `plan/refactor_kind.rs` (a rebase conflict, not a dependency).
**Blocks:** `feature/reshape/backend-session` (K=18, wave 4).
Real dependency edges (whole stack): `1→13`, `5→14`, `2→15`, `3→15`, `4→16`, `13→17`, `2→17`, `3→17`,
`17→18`, `6→18`, `4→19`, `17→19`.

## Successor PRs

- `feature/reshape/backend-session` (`#reshape` 18/19) reuses **only the self-mode body rewrite**
  (`self` → `<name>`, `Self` → the impl's self type) and the RS4 shadowing refusal, from inside its own
  `detach_method` operation. That operation rewrites every caller and leaves **no forwarding wrapper**,
  so it uses neither `leave_delegator` nor `extract_method`. The reused items are `pub(crate)` (Draft PR
  contract) so that a sibling module of `backends/rust` can call them.

## Scope

- [ ] **Plan surface**: variant, `rebind_fields.rs` (RP1-RP4), P9, `signature_fields.rs` admitting `expr`
- [ ] **Lexical rules**: `range.rs` (sites, RS1-RS4) as static findings and in `resolve`
- [ ] **Field mode**: trial text, RS5-RS7, hover typing, the `&` rule, the note
- [ ] **Self mode**
- [ ] **Delegator**: S7, the forwarding text, `Layout::with_delegators`, `the_replacement`, the dead-delegator note, `deferred_delegator` removed
- [ ] **`verify`**: R3, R-rebind, `Rebind`, carriers through the CLI, `tddy-tools` and the daemon
- [ ] **Registration**: the two new live binaries in `.config/rust-e2e.filterset` and the `rust-analyzer` group of `.config/nextest.toml`
- [ ] **Package documentation** at wrap (list under Affected Packages)
- [ ] **Testing**: `./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools` (scoped); CI for the rest
- [ ] **Code quality**: `cargo check --all-targets` on the three packages, scoped clippy `-D warnings`, `cargo fmt`. Every new file ≤ 500 production lines and every new function ≤ 60; `retarget_impl()` ≤ 70 and `parse_op` + 1 line

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

- `RefactorKind` has 25 variants. None rewrites `self` reads (`plan/refactor_kind.rs:14-162`,
  `backends/rust.rs:75`).
- The `leave_delegator` variant is accepted (`retarget_fields.rs:91-95,106`), and so is `expr`
  (`signature_fields.rs:27-32`). Both are refused by `deferred_delegator` (`retarget_impl.rs:29-33,41-48`)
  and as `UnsupportedOp` in `apply` (`:61-66`). P9 is not enforced.
- `retarget_impl()` (`:79-148`, 70 lines):
  - computes every moved member's references (`:120`), but uses only those inside the range (`:121-125`);
  - returns no notes (`:145`).
- `rewrite::Layout` (`retarget_impl/rewrite.rs:16-92`) assembles only `[Old: P] [New: M] [Old: A]`.
- `verify`:
  - `Declared { retargets, repoints }` (`verify/retarget.rs:27-32`);
  - pass order (`verify.rs:204-208`): visibility, R1/R2, R-call, re-point, reflow;
  - no rule excuses a rebound `self`, a gained `let`, or a forwarding body.
- The proto `VerifyRequest` carries fields 1-4 (`code_index.proto:280-288`).
- Reusable helpers:
  - `masked_to_code` (`early_return.rs:271`);
  - `did_change` (`rust.rs:977`);
  - `unresolved_names` (`rust.rs:1900`);
  - `request_settled` (`rust.rs:894`);
  - `settled_outline` (`rust.rs:1698`);
  - `sites_of` (`item_move.rs:144`).
  Hover contents are never parsed today.

### State B (Target)

- A T3 method body is prepared for extraction by one plan line, with no hand substitution.
- A `RustBackend` operation body can be rebound wholesale for `extract_method`.
- A retarget can keep its callers compiling with forwarding methods the engine writes, and it names the
  ones nothing needs.
- `verify` holds over all three outputs under declarations.

### Delta (What's Changing)

#### `tddy-code-restructuring`

- **New** `backends/rust/read_fields_through.rs` (~120: the run, `findings`) and its children:
  - `range.rs` (~170: sites, statement boundary, shadowing, RS1-RS4);
  - `typing.rs` (~110: hover parsing, agreement);
  - `edits.rs` (~90: the `let` and the substitutions).
- **New** `backends/rust/retarget_impl/delegator.rs` (~200: S7, forwarding text, dead-delegator notes).
- **`retarget_impl.rs`**:
  - remove `deferred_delegator` and its two call sites, with the `TODO(retarget-impl)`;
  - extract `the_replacement` (lines 121-132), which chooses `Layout::assemble` or `Layout::with_delegators`;
  - `Resolution.notes` gets the dead-delegator notes.
- **`retarget_impl/rewrite.rs`**: `Layout::with_delegators`.
- **`backends/rust.rs`**: `mod read_fields_through;`, `SUPPORTED` 25 → 26, one `check` arm, one `resolve` arm.
- **`plan/refactor_kind.rs`** (variant), **`plan/codec.rs`** (one `mod`, one call),
  **new `plan/codec/rebind_fields.rs`**, **`retarget_fields.rs`** (P9), **`signature_fields.rs`**
  (admit `expr` on `ReadFieldsThrough`, and its message).
- **`verify/rebind.rs`** (new, ~120), **`verify/retarget.rs`** (R3 ~90, `Declared.rebinds`,
  `with_rebinds`), **`verify.rs`** (one pass, re-export).
- **`restructure_args.rs`**, **`runner/options.rs`**, **`runner/comparison.rs`**: the `--rebind` carrier.

#### `tddy-index-daemon`

- `proto/code_index.proto`: `repeated string rebinds = 5;`.
- `src/cli.rs:387`: `rebinds: verify.rebind`.
- `src/queries.rs:290`: `rebinds: request.rebinds`.
- Struct literals in tests: `cli.rs:685`, `tests/dual_transport_acceptance.rs:453`.

#### `tddy-tools`

- `src/index_client.rs:375`: `rebinds: args.rebind`.

## Implementation milestones

- [ ] **M1** Plan surface (variant, `rebind_fields.rs`, P9); tests 1-5, 20
- [ ] **M2** Lexical rules as static findings; test 6
- [ ] **M3** Field mode, live (trial text, typing, `&` rule, note); tests 7-12, 15-16
- [ ] **M4** Self mode; tests 13-14
- [ ] **M5** Delegator (S7, text, placement, dead note, `the_replacement`); tests 21-26, 28
- [ ] **M6** `verify` R3 and R-rebind, plus the carriers; tests 17-19, 27, 29-30
- [ ] **M7** Register the two live binaries; docs staged; scoped gate; length gate

## Testing plan

### Testing Strategy

- **Library level, no server:**
  - plan lines and static findings, through `RestructurePlan::parse` and `RustBackend::check`;
  - `verify::compare_with` over before/after maps;
  - text functions (`forwarding_method`, `self_sites`, `agreement`, `field_type` over recorded hover
    JSON) as unit tests in their modules.
- **Two thin live binaries** apply through the runner, with `assert_compiles_with_its_tests` /
  `assert_lints_clean` as the oracle. They are the only places the trial text, hover typing, the
  compile gate and the notes are exercised together.

#### Option 1 (chosen): library tests in `tests/read_fields_through_plan_lines.rs`, `tests/retarget_impl_plan_lines.rs`, `tests/verify_accounts_for_a_rebind.rs`, `tests/verify_accounts_for_a_retarget.rs`
Fast and exact on text. They do not prove the tree compiles.

#### Option 2 (chosen, thin): `tests/read_fields_through_acceptance.rs`, `tests/retarget_impl_delegator_acceptance.rs` (new live)
Fixtures:

- `same_crate::an_app_holding` (`tests/same_crate/mod.rs:32`) for one crate;
- `an_app_over_a_kernel` (`:196`) for a state type in **another crate**;
- `the_impl_blocks_of` / `blocks` (`:281`, `:315`) for placement.

The delegator cases are not added to `tests/retarget_impl_acceptance.rs` (588 lines).

#### Option 3 (rejected): typing through `syn` declarations
`retarget_impl/fields.rs:29-43` reads only same-package types, and the T3 state type is in another
crate.

### Coverage Requirements

- [ ] Happy:
  - field mode, same crate and cross crate;
  - the `&` drop and the kept `&` of a `Copy` value;
  - self mode followed by `extract_method`;
  - delegators for a whole block and a subset, `async`, an associated function, attributes.
- [ ] Refusals: RP1-RP4, RS1-RS7, P9, S7.
- [ ] Untouched: comments and strings in the range; callers byte-identical; `retarget_impl` without the variant.
- [ ] Parity: static `check` and `check --deep` against `apply`.
- [ ] Actual effects: bytes on disk; `cargo check --all-targets`; clippy `-D warnings`.

## Acceptance tests

Names read as behaviour specifications. 1-27 and 29-30 are **red on `master`**: `read_fields_through`
is an unknown variant, `leave_delegator` is refused as unimplemented, `--rebind` is an unexpected
argument, and `VerifyRequest` has no `rebinds`. 28 is a **green pin**.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/read_fields_through_plan_lines.rs` (new; library, no server)

1. `a_read_fields_through_line_parses_and_reads_back_without_its_defaults`
2. `a_read_fields_through_without_name_or_expr_is_malformed` (RP1, RP2)
3. `a_binding_that_is_self_a_keyword_or_not_one_identifier_is_malformed` (RP1)
4. `a_symbol_an_items_or_a_rangeless_item_anchor_names_no_statements_to_rebind` (RP3)
5. `every_field_read_fields_through_does_not_define_is_refused_naming_it`: a table of `to`, `to_type`,
   `variant`, `callee`, `type`, `order`, `reexport`, `also`, `to_file`, `canonical_paths`,
   `with_private_deps` (RP4).
6. `a_static_check_refuses_a_mid_statement_start_a_range_without_self_a_method_call_in_field_mode_and_a_shadowing_binding`
   (RS1-RS4, each naming its line; self mode admits the method call).

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/read_fields_through_acceptance.rs` (new; **live**; register in `.config/nextest.toml` `rust-analyzer` group and `.config/rust-e2e.filterset`)

7. `rebinds_every_field_read_of_the_range_through_the_state_value_and_the_tree_compiles`: `Host` with
   `fn state(&self) -> HostState<'_>`; the range reads two fields. Afterwards `let state = self.state();`
   precedes the range, the range holds no `self`, and `assert_compiles_with_its_tests` passes.
8. `a_state_value_declared_in_another_crate_is_read_through_and_the_tree_compiles`
   (`an_app_over_a_kernel`; the T3 shape).
9. `a_borrow_of_a_field_the_state_holds_by_reference_loses_its_ampersand_and_a_copy_field_keeps_it`:
   `&self.config` becomes `state.config`; `&self.interval` (a `Duration` in both) stays
   `&state.interval`; `assert_lints_clean`.
10. `a_field_the_state_value_lacks_is_refused_naming_every_one_and_nothing_is_written` (RS6; file
    byte-identical).
11. `a_field_whose_type_differs_by_more_than_one_reference_is_refused_naming_both_types` (RS7: a state
    `&'a Arc<T>` against a host `Arc<T>` is accepted; a state `Vec<u8>` against a host
    `Arc<Vec<u8>>` is refused).
12. `a_range_in_a_free_function_is_refused_as_having_no_self_receiver` (RS5).
13. `self_mode_rebinds_method_calls_fields_and_the_self_type_and_a_following_extract_method_writes_a_free_function`
    (a `Self::helper()` call and a `Self { .. }` literal in the range become `Host::helper()` and `Host { .. }`):
    a group of `read_fields_through` (`expr: "self"`) and `extract_method` over the same range. The new
    function takes `backend: &mut Host` and is not a method; the result compiles.
14. `self_mode_on_a_shared_receiver_binds_a_shared_reference_and_compiles` (`&self` with `expr: "self"`).
15. `comments_and_strings_in_the_range_that_mention_self_are_byte_identical`.
16. `a_deep_check_reports_the_refusal_an_apply_gives_and_notes_what_an_apply_rebinds`: one RS3 plan and
    one clean plan. The refusal text is equal in both, and the note lines are equal in both.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/verify_accounts_for_a_rebind.rs` (new; library, `verify::compare_with`)

17. `a_declared_rebind_accounts_for_the_rebound_statements_the_dropped_borrow_and_its_one_let`.
18. `without_the_declaration_the_rebind_is_reported_and_with_it_a_second_let_or_a_changed_argument_still_is`.
19. `a_rebind_declaration_must_be_one_identifier_other_than_self`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/retarget_impl_plan_lines.rs` (existing; library)

20. `a_delegator_variant_without_an_expression_is_malformed_and_an_expression_without_it_too` (P9,
    at plain `check`).

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/retarget_impl_delegator_acceptance.rs` (new; **live**; register as above)

21. `leaves_a_forwarding_method_on_the_old_type_so_callers_keep_compiling`: a whole block. `caller.rs`
    calls `h.get()` and `h.put(1)`; afterwards it is byte-identical and the tree compiles with its
    tests.
22. `a_proper_subset_leaves_its_delegators_in_place_and_the_new_block_follows_the_old`
    (`the_impl_blocks_of` equals `[Host: get, put(delegator), size] [Roster: put]`).
23. `forwards_an_async_method_with_await_and_an_associated_function_through_the_new_type`.
24. `a_delegator_keeps_the_signature_and_outer_attributes_and_the_doc_comment_stays_on_the_moved_member`.
25. `refuses_a_delegator_for_a_pattern_parameter_an_associated_const_and_a_const_fn_naming_each` (S7,
    tree untouched).
26. `a_delegator_with_no_caller_left_is_noted_by_the_deep_check_and_the_apply`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/verify_accounts_for_a_retarget.rs` (existing; library)

27. `a_declared_retarget_accounts_for_its_delegators_and_a_forwarding_method_with_other_arguments_is_reported` (R3).

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/retarget_impl_acceptance.rs` (existing; live)

28. **Green pin:** the whole existing binary passes unchanged. `retarget_impl` without the variant
    writes no delegator and no note.

### `tddy-index-daemon` — `packages/tddy-index-daemon/tests/dual_transport_acceptance.rs` (existing; already in `.config/rust-e2e.filterset`)

29. `verify_carries_a_declared_rebind_through_the_cli_and_the_daemon_and_both_render_the_same_lines`:
    with `--rebind state`, both front ends hold. Without it, both report the result.
30. (unit, `packages/tddy-index-daemon/src/cli.rs`)
    `verify_carries_the_rebinds_it_is_told_of_beside_the_repoints_and_the_retargets`.

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

**Taken by the developer (2026-10-09, binding):**

- The PRD is approved with every recommendation (F1-F7).
- Self mode is in scope; node 18 consumes it.
- F4 is a new `--rebind` flag.
- No function on the nodes 16/19 list may grow.
- Node 1 registers the pre-existing unregistered live suites; this node registers only its own two.

**Decided (recommendations accepted):**

- **F1:** `name` + `expr`. No `fields` list (any field left out would be refused, so the list carries
  no information) and no `builder` (`expr` generalises it). No `RefactorOp` field, so no 22-literal
  churn.
- **F2:** self mode, selected by `expr` being `self` / `&*self` / `&mut *self`, rather than by a
  `variant`. The mode follows from what the binding *is*.
- **F3:** field types come from hover on a trial text, which works across crates. The alternative,
  compile gate only, leaves clippy red with `needless_borrow`.
- **F4:** a new `--rebind NAME` declaration (proto field 5), not the reuse of `--retarget self=NAME`
  that already parses.
- **F5:** delegators go in place, and `impl New` follows the old block.
- **F6:** every moved method gets a delegator, and dead ones are noted from the existing reference set.
- **F7:** R3 is keyed on the declared `--retarget`, with no delegator carrier.

**Open (for the developer; each has a recommendation):**

- **F8: a hover format the parser cannot read.** (a) **Refuse as `rust-analyzer's answer was
  unusable`**: *recommended*, since a refusal is not a fallback. (b) Skip typing for that field and
  keep the `&`: that is a silent fallback, which CLAUDE.md forbids without consent.
- **F9: the RS4 shadowing check's reach.** (a) **The whole enclosing function body, lexically**:
  *recommended*, conservative and cheap. (b) Only the code from the range start to the end of the block,
  which needs block structure the lexical scan lacks.
- **F10: a dead delegator.** (a) **Write it and note it**: *recommended*, because a `pub` method of a
  library may have callers outside the workspace. (b) Omit it, which needs a second rule for `pub`.

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

- [x] Record initial discovery (`2026-10-09-reshape-methods-leave-type-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-09-reshape-methods-leave-type.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append point: not edited while planning)
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
- [ ] Linting and formatting (`cargo clippy -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools --all-targets -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review. It also deletes `2026-10-09-reshape-methods-leave-type-initial-discovery.md` and both claimed todo entries, and adds the history row to `oversized-file-backends-rust.md`
- [ ] USER REVIEW — work complete, decide next steps
