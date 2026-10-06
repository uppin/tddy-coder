# Changeset: `retarget_impl` moves impl members from one type to another, and `restructure verify` accounts for it

**Date**: 2026-10-05
**Status**: 🚧 In Progress
**Type**: Feature (a new restructure operation, and the `verify` teaching that must come with it)
**Stack**: `#sharpen` 6/8, branch `feature/sharpen/retarget-impl`, wave 2. PR title:
`feat(code-restructuring,tools,index-daemon): retarget_impl moves impl members to another type, and verify accounts for it (#sharpen 6/8)`.
Draft PR: https://github.com/uppin/tddy-coder/pull/593. Base in the linear stack: `feature/sharpen/plan-header` (K=5). **Real edges**: from `feature/sharpen/tidy-engine-files` (K=1, file overlap and the new home of `RefactorKind`), and to `feature/sharpen/repoint-call` (K=7), which extends the `verify` declaration carrier this node builds.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-05-sharpen-retarget-impl-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

The scan followed `deferred-work/references/planning-cross-check.md`. `grep -rl 'Claimed by:'` over `tddy-code-restructuring`, `tddy-tools` and `tddy-index-daemon` finds one file
(`broken-restructure-anchors-empty-outline.md`) whose value is `none` (#537 merged 2026-10-02): **no 🚧 claimed issue is in this change's path, so there is no wait-or-proceed fork.**
All three packages edited here have a `docs/code-issues/`; `tddy-lsp` is not touched. The four `tddy-tools/docs/code-issues/*` records are against `cli.rs` and `server.rs`; this node edits
`index_client.rs`, which has none.

| Item | Verdict | What this change does about it |
|---|---|---|
| `docs/dev/todo/2026-10-05-restructure-no-operation-retargets-an-impl-to-another-type.md` — **exists only on `feature/carve/lifecycle-ports-agents` (PR #532)**; whichever of that PR and this node lands second deletes it at wrap | ✅ **RESOLVED HERE** (planned) | `retarget_impl` (milestones M1-M3): the header rewrite, the block split, the re-pointed paths, the field refusal named by `check --deep`, comments kept. Closed when the acceptance tests pass. Reference by branch and file, not by link: it is not on `master` |
| `docs/dev/todo/2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator.md` — **exists only on `feature/carve/lifecycle-ports-agents` (PR #532)**; whichever of that PR and the nodes that claim it lands second deletes it at wrap | ✅ **RESOLVED HERE for the delegator half** if M4 lands; ⚠ **DURING (narrowed, not deleted)** if M4 is cut | This node, the lowest of the two that touch it, claims it. The **receiver half** (`repoint_call`) is `feature/sharpen/repoint-call`'s: that node keeps a reference and the entry is deleted only when both halves have landed. If M4 is cut to a follow-up the entry is edited to say so, and the verdict stays ⚠ (a partly addressed entry is never deleted) |
| [2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md](../todo/2026-09-25-restructure-has-no-operation-to-read-a-methods-fields-through-a-state-parameter.md) | ⚠ **DURING**, deliberately **not closed** | `self.field` -> `state.field` is a field read, not a header retarget and not a call. This change **refuses** a moved member that reads a field the new type lacks (S4 below) rather than rewriting it. The entry stays open |
| `packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md` (2,853 production lines; `Restructure: required`) | ⚠ **During** | Wiring only in `backends/rust.rs`: `mod retarget_impl;`, `SUPPORTED` 22 -> 23, one `check` arm, one `resolve` arm (about 12 lines). **All logic in `backends/rust/retarget_impl/`.** A history row is appended at wrap. The issue stays open; `retarget_impl` retargets **between types**, it does not supply the impl-member *move* seam the record says is missing |
| [2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md](../todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md) | ⚠ **During** | Same constraint, same remedy |
| [2026-10-05-restructure-engine-files-past-the-500-line-budget.md](../todo/2026-10-05-restructure-engine-files-past-the-500-line-budget.md) | ⚠ **During** (resolved by `tidy-engine-files`, not here) | `plan/refactor_kind.rs` (where `tidy-engine-files` puts `RefactorKind`) gains one variant; `plan.rs` gains one `RefactorOp` field; `plan/codec.rs` gains one `mod` line and one call. New rules go in `plan/codec/retarget_fields.rs`. **`plan.rs`, `plan/refactor_kind.rs` and `plan/codec.rs` must still be <= 500 production lines after this node**, measured with `restructure check --budget 500` once at the end; the variant's and the field's doc comments are kept to the lines they need |
| [2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md](../todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md) § 6 (`verify` reports one `)` lost when a call is reflowed) | ⚠ **During** | The new `verify` pass must not make the "one real loss keeps the reflow of the other leftovers reported" behaviour worse; the entry is not closed here |
| [2026-10-04-restructure-move-item-copies-the-whole-use-header.md](../todo/2026-10-04-restructure-move-item-copies-the-whole-use-header.md), [2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md](../todo/2026-10-05-restructure-same-crate-moves-limits-found-moving-lifecycle.md) | — Unrelated | `retarget_impl` copies no `use` header: it writes **one** `use` for the new type when the file does not already bind it |
| `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md`, `…/complexity-warm-narrate-until-loaded.md` | — Unrelated | This node touches `proto`, `cli.rs` and `queries.rs::serve_verify`'s request, not the warm or readiness path (those are `apply-heartbeat`'s) |
| `docs/dev/1-WIP/2026-09-17-restructure-refusal-truth-and-authoring-gates.md` | ℹ not this stack's to wrap | All milestones `[x]`; looks unwrapped rather than active; ask whether it is stale before wrap |

## Affected Packages

- **`tddy-code-restructuring`**: [README.md](../../../packages/tddy-code-restructuring/README.md) (operation list, the `verify` line),
  new `src/backends/rust/retarget_impl.rs` and `src/backends/rust/retarget_impl/` (all logic), wiring in `src/backends/rust.rs`, `src/plan/refactor_kind.rs` (variant), `src/plan.rs` (the `to_type` field; and a `to_type: None` in the 15 files that hold a full `RefactorOp` struct literal, see O3),
  `src/plan/codec.rs` (one `mod`, one call) and new `src/plan/codec/retarget_fields.rs`, `src/verify.rs` and new `src/verify/retarget.rs`, `src/restructure_args.rs`,
  `src/runner/options.rs`, `src/runner/comparison.rs`, `src/backends/rust/item_move/text.rs` (two visibilities widened). Docs at wrap: a new
  `docs/retarget-impl.md` (the behaviour page, in the manner of [same-crate-moves.md](../../../packages/tddy-code-restructuring/docs/same-crate-moves.md)),
  [oversized-file-backends-rust.md](../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md) (history row).
- **`tddy-index-daemon`**: [README.md](../../../packages/tddy-index-daemon/README.md); `proto/code_index.proto` (`VerifyRequest` gains `repeated string retargets = 3;`, and `= 4` for delegators if M4 lands),
  `src/cli.rs` (the request carries them; the unit test that says it carries "only the tree and the ref" changes), `src/queries.rs` (hands them to `runner::verify`).
- **`tddy-tools`**: [README.md](../../../packages/tddy-tools/README.md); `src/index_client.rs` (`verify` fills the request). No new subcommand; one flag on an existing one (defined in `tddy-code-restructuring`'s `RestructureVerifyArgs`).
- **`.agents/skills/code-restructuring/`** (dev-only): [SKILL.md](../../../.agents/skills/code-restructuring/SKILL.md) (count "twenty-two" -> "twenty-three", a paragraph, the `verify` flag),
  [plan-schema.md](../../../.agents/skills/code-restructuring/references/plan-schema.md) (table row and a `### retarget_impl` section).
- **`.config/nextest.toml`, `.config/rust-e2e.filterset`**: two new live binaries.

## Related Feature Documentation

- [PRD-2026-10-05-sharpen-retarget-impl.md](../../ft/coder/1-WIP/PRD-2026-10-05-sharpen-retarget-impl.md) (this PRD)
- [Rust code restructuring](../../ft/coder/rust-code-restructuring.md) — `## Rust operations (v1)` / `### Same-crate moves`, `## Verify`, the CLI table

## Summary

A new Rust operation, `retarget_impl`, rewrites an inherent `impl`'s self type to another type of the same crate: the whole block, or the run of members it is anchored on (the block is split at
the run). It re-points the paths the old type named for the moved members, adds one `use`, refuses before any write when a moved member reads a field the new type does not declare, and keeps every comment.
`restructure verify` is taught to account for it, on a declaration the author makes. An optional forwarding delegator that keeps callers compiling is a separable milestone.

## Background

Moving 22 methods from `impl DaemonSessionHost` to `impl AgentRoster` (`#carve` 17/21, stage B1) was an `impl` header edit per file and a `use`, and no operation does it. The closest probe,
`change_param_type` on `self`, is refused (`self is not a parameter of the function the anchor names`); `move_item` moves only a **whole** impl block, keeping its self type (plan-schema: "a member of an
`impl` cannot move alone"). The engine's guarantees (compile gate, comments kept) did not reach the hand edit, and `restructure verify` then reported the hand-made retargets as a loss it cannot excuse.

## Responsibility

- A new operation `retarget_impl`: plan-line schema, parse-time refusals, `check` arm, `resolve` arm, `SUPPORTED` entry, in `backends/rust/retarget_impl/`.
- The header rewrite, the block split at the anchored members, the path re-points inside the moved members, the `use` of the new type, comments and attributes kept.
- The field refusal (S4), reported by `check --deep` and raised by `apply` before anything is written.
- `restructure verify` accounting for a declared retarget (M3), through a declaration carried by the library, the CLI, the daemon request and the proto.
- (M4, separable) a forwarding delegator on the old type, and its accounting in `verify`.
- Docs staged for wrap; two live test binaries registered.

## Boundaries

- **Inherent impls only.** A trait impl (`<Old as Trait>`) is refused at parse time (P5).
- **Same crate only.** `to_type` in another package is refused (P7). A move between crates is `move_module_to_crate`.
- **Callers are not re-pointed.** An outside caller of a moved member (`x.m()`) breaks (`E0599`) unless a delegator (M4) keeps it, or a later operation (`repoint_call`, `feature/sharpen/repoint-call`) re-points it in the same `group`. This
  operation never edits a file other than the one holding the block (and no file at all besides it unless the `use` goes elsewhere: it does not).
- **Fields are refused, not rewritten.** `self.f` -> `state.f` is the open `…state-parameter` todo.
- **Type-position mentions of the old type** in a moved member (`other: &Old`) are left as written; only `Old::<moved member>` paths are re-pointed (see Decisions O6).
- **No widening.** A private field or method of the new type that a moved member reaches is a compile-gate failure (`E0616`/`E0624`), exactly as for `move_item`; the operation does not widen it.
- **The block stays in its file.** To put the new block in the new type's module, run `move_item` over `<New>` afterwards.
- **No new crate edge, no proto RPC**: one field on one existing request.
- **Not re-done here** (see Dependencies): the length split of `plan.rs`, `plan/codec.rs`, `item_anchor.rs`.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| **`tidy-engine-files`** (K=1, `feature/sharpen/tidy-engine-files`) | `plan.rs`, `plan/codec.rs` and `item_anchor.rs` each <= 500 production lines, by child modules, made with the engine's own `move_item`/`extract_module`; no behaviour change. In particular `RefactorKind` and its `impl` (the variants and the predicates `edits_a_call_site`, `moves_across_crates`) now live in `plan/refactor_kind.rs` (its decision D1, developer-approved 2026-10-05), kept reachable at the old path by `pub use refactor_kind::RefactorKind;`; `RefactorOp` stays in `plan.rs`. **File overlap and layout only**: no signature is consumed | The `RetargetImpl` variant is added in `plan/refactor_kind.rs`; the `to_type` field is added to `RefactorOp` in `plan.rs`; the `mod` line and the codec call go into the smaller `plan/codec.rs`; the new rules go in a new child module beside the ones tidy made. This PR rebases onto it before its own first commit | re-split `plan.rs`, `plan/codec.rs` or `item_anchor.rs`; move, rename or re-order existing `parse_op` rules or tests; re-measure the budget or claim `2026-10-05-restructure-engine-files-past-the-500-line-budget.md`; add logic to those files beyond the variant, the field, one `mod` line and one call |

Order-only overlaps in the line (not edges): `plan-header` (K=5) shares `plan/codec.rs` (a `mod` line and a call each); `move-fidelity` (K=2) reads `item_move/text.rs`'s `use_insertion` and `scope_of` for its B1 import insertion, where this node widens two visibilities, so this node rebases onto it. This node **builds the declaration carrier** (`--retarget`, `VerifyRequest.retargets`, `verify::Declared`, rules R1 and R2); `repoint-call` consumes it (an edge), see Successor PRs.

## Draft PR contract

Published with the wave-2 contract commit. Signatures below are proposals that green may reshape, recording the change here; **the tests bind to the JSON plan line, the runner, the CLI flag text and the public `verify` function, not to private names.**

- **Plan surface**: `RefactorKind::RetargetImpl` (serde `retarget_impl`), in `plan/refactor_kind.rs`; `RefactorOp.to_type: Option<String>` (serde `to_type`, default and skip-if-none), in `plan.rs`, with `to_type: None` on the 20 full `RefactorOp` struct literals in 15 files (this node's first commit). Plan line: see "Plan-line schema".
- **Backend** (crate-private, `backends/rust/retarget_impl.rs`): `pub(super) fn findings(op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Vec<String>>` (static, no server, like `item_move::findings`);
  `impl RustBackend { pub(super) fn retarget_impl(&mut self, op: &RefactorOp, workspace: &Workspace<'_>) -> Result<Resolution> }` (like `move_items`); `SUPPORTED: [RefactorKind; 23]`.
- **`verify`** (public): `verify::compare_with(before: &BTreeMap<String,String>, after: &BTreeMap<String,String>, declared: &verify::Declared) -> Comparison` where `Declared { retargets: Vec<Retarget> }` and
  `Retarget { from: String, to: String }`; `verify::compare(before, after)` stays and is `compare_with(.., &Declared::default())`. `Options.retargets: Vec<String>`; `RestructureVerifyArgs.retarget: Vec<String>`
  (`--retarget OLD=NEW`, repeatable); `VerifyRequest.retargets: repeated string = 3`.
- **Failing tests that specify it** (red on `master` today for the reason stated in "Acceptance tests"): `tests/retarget_impl_plan_lines.rs` (library), `tests/retarget_impl_acceptance.rs` (live),
  `tests/verify_accounts_for_a_retarget.rs` (library), one case in `tddy-index-daemon/tests/dual_transport_acceptance.rs`; and, only if M4 lands in this PR, `tests/retarget_impl_delegator_acceptance.rs` (live).

## Green wave

**Wave:** 2 of 2.
**Greenable independently:** yes, once `feature/sharpen/tidy-engine-files` is on its base. No test depends on another node's behaviour.
**Concurrent with:** `feature/sharpen/plan-header`, `feature/sharpen/repoint-facade` (same wave, no edge between them; the line serialises them because they share `plan.rs`, `plan/codec.rs` and `backends/rust.rs`).
**Blocks:** `feature/sharpen/repoint-call` — it extends the `verify` declaration carrier built in M3 (`verify::compare_with`, `Declared`, `verify/retarget.rs` rules R1 and R2, `--retarget`, `Options.retargets`, `VerifyRequest.retargets = 3` and the plumbing). That is an edge by surface, and a real one: whichever node lands without M3 would have to build the carrier itself.
Real dependency edges (whole stack): `tidy-engine-files -> plan-header, retarget-impl, repoint-call, repoint-facade`; `move-fidelity -> repoint-facade`; `retarget-impl -> repoint-call`. Nothing else is an edge: `spawn-record` and `apply-heartbeat` consume nothing and nothing consumes them (`spawn-record` lands after open draft PR #586, a merge-order fact, not a stack edge).
Conditional, not an edge in the list above: `feature/sharpen/repoint-facade` would consume the carrier only if its decision F3 chose to copy attributes onto split statements; its recommendation (refuse) consumes nothing.

## Successor PRs

Forward references only. Branches above this one that are expected to build on its surface:
- `feature/sharpen/repoint-call` (K=7): reuses `verify::Declared`, `--retarget`'s plumbing as the pattern for its own declaration (a repointed receiver), and keeps a reference to the delegator todo.
- `feature/sharpen/repoint-facade` (K=8): **only if** its decision F3 is taken as "copy attributes" (not its recommendation); otherwise it consumes nothing from this node.

## Scope

**High-level deliverables tracking progress throughout development:**

- [ ] **Probe (M0)**: confirm, with one live rust-analyzer run against the fixture, the three unverified premises (outline of an impl and its members; references to an associated function written `Old::f(..)` inside the block; hunks `minimal_edits` returns for a split)
- [ ] **Plan surface**: `RetargetImpl` (in `plan/refactor_kind.rs`), `to_type` (in `plan.rs`) and its 20 literals, `plan/codec/retarget_fields.rs`, parse-time refusals P1-P8, `SUPPORTED` 23, `check` arm (static), `resolve` arm
- [ ] **Whole-block retarget** (header rewrite, `use`), comments and attributes kept
- [ ] **Block split** at the anchored members, header repeated, in place
- [ ] **Path re-points** for `Old::<moved member>` inside the moved members, from the server's reference set
- [ ] **Field refusal** S4 named by `check --deep` and raised by `apply` before any write; S1-S3, S5, S6
- [ ] **`verify` accounting** (M3): `Declared`, the two pairing rules, `--retarget`, the request field, the proto, the daemon and CLI plumbing
- [ ] **Delegator (M4, separable)**: `variant: "leave_delegator"` + `expr`, the emitter, its refusals, its `verify` accounting — or **cut to a follow-up** at the M3 decision point (Decision O2)
- [ ] **Registration**: both live binaries in `.config/nextest.toml` (`rust-analyzer` group) and `.config/rust-e2e.filterset`
- [ ] **Package documentation** staged for wrap (counts 22 -> 23, plan-schema, SKILL.md, feature doc, README, `retarget-impl.md`, the code-issue history row)
- [ ] **Testing**: all acceptance tests pass; `./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools`, scoped
- [ ] **Code quality**: `cargo clippy -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools --all-targets -- -D warnings`, `cargo fmt`; `plan.rs`, `plan/refactor_kind.rs`, `plan/codec.rs`, `verify.rs`, every new file <= 500 production lines
- [ ] **Stack bookkeeping**: the two #532 todos handled at wrap as stated in Prerequisites

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical changes

### State A (Current)

On `a77bca29`:

- 22 operations (`backends/rust.rs:66`); none changes the self type of an `impl`. `change_param_type` on `self` is refused. `move_item` moves a whole inherent block keeping its type and refuses a range inside one.
- Item anchors `c::m::<Stack>` (a block) and `c::m::Stack::f` (a member) resolve through the server's outline; a run of members is a valid `items` anchor. They are lowered to a line `Range` before an operation resolves.
- `RefactorOp` has `to` ("Destination path, for moves"), `variant`, `type`, `expr` and no field for a destination *type*. Every `RefactorOp` is built by a literal naming every field in 20 places (15 files).
- `verify` pairs a lost and a gained statement through visibility, lowercase module qualifiers and token-multiset equality; `impl Host {` -> `impl Roster {` and `Host::build(` -> `Roster::build(` pair through none. It takes a git ref and nothing else, in the library, the CLI, the daemon request and the proto.

### State B (Target)

#### Plan-line schema

```jsonl
{"op":"retarget_impl","anchor":{"kind":"items","file":"src/host.rs","items":["app::host::Host::put","app::host::Host::last"],"fingerprints":["sha256:…","sha256:…"]},"to_type":"app::roster::Roster"}
{"op":"retarget_impl","anchor":{"kind":"item","item":"app::host::<Host>","file":"src/host.rs","fingerprint":"sha256:…"},"to_type":"app::roster::Roster"}
{"op":"retarget_impl","anchor":{…},"to_type":"app::roster::Roster","variant":"leave_delegator","expr":"self.roster()"}      // M4
```

| Field | Type | Required | Default | Meaning |
|---|---|---|---|---|
| `op` | `"retarget_impl"` | yes | | |
| `anchor` | `item` or `items` | yes | | **Whole block**: one `item` anchor on `<Type>` (or `<Type>#N`, the Nth in source order), no relative range. **Member run**: one `items` anchor whose items are members `c::m::Type::member` of **one** inherent impl and contiguous (only blank lines between), or one `item` anchor on a single member. A `range` or `symbol` anchor, or an `item` anchor with a relative range, is refused |
| `to_type` | string | yes | | The new self type: an item path **rooted at the package name** (`-` read as `_`) with optional generic arguments on the last segment — `app::roster::Roster`, `app::roster::Pair<T>`. Exactly one `syn::Type`, and a path type (no `&`, `dyn`, tuple, array, `impl`). The engine writes `crate::…` in the `use` it adds and the bare last segment (generics as written) in the header |
| `variant` | `"leave_delegator"` | no (M4) | absent | Leave a forwarding method on the old type for every moved member (see "The delegator"). No other value is defined |
| `expr` | string | only with `variant: "leave_delegator"` | absent | One `syn::Expr` (the existing `one_expr` rule: no statement inside it) that reaches the new type from the old one's `self`, e.g. `self.roster()` |
| `id`, `group` | as on every operation | | | `retarget_impl` joins a transactional `group`, which is how it composes with call re-points |

Refused as fields `retarget_impl` does not define: `to`, `name`, `with_private_deps`, `variant` other than `leave_delegator` (new, in `retarget_fields.rs`); `reexport`, `also`, `to_file`, `type`, `order` (**existing** refusals already say so); `expr` without `variant: "leave_delegator"` (the existing `expr` rule is extended for M4).

#### Refusals, by class (the class says who fixes what)

`plan is malformed:` — edit the plan; reported by plain `check`, `check --deep` and `apply`, before any server starts.

| # | Message stem | When |
|---|---|---|
| P1 | `` `retarget_impl` needs `to_type`: the type the members move to `` | field absent |
| P2 | `` `to_type` must be one path type, `app::roster::Roster` with optional generic arguments; `{text}` is not `` | not a single path type |
| P3 | `` `to_type` names the type an `impl` retargets to, which only `retarget_impl` honours — `{kind:?}` cannot `` | `to_type` on another operation |
| P4 | `` `retarget_impl` anchors by item (`items`, or a single `item`): a range or a symbol names no `impl` or member to retarget `` | wrong anchor kind, or an item anchor with a relative range |
| P5 | `` `retarget_impl` moves members of an inherent `impl`; `{path}` is a trait impl, and its members cannot change type without breaking the trait's contract `` | an anchor path with a `TraitImpl` segment |
| P6 | `` `{field}` is not a field of `retarget_impl`: the new type is named by `to_type` `` | `to`, `name`, `with_private_deps`, an unknown `variant` |
| P7 | `` `to_type` is in package `{a}`, and the anchor is in `{b}`: `retarget_impl` does not leave its crate `` | static, from manifests (the rule `move_item` states for `to`) |
| P8 | `` `{module}` declares no struct, enum or union named `{Name}` `` / `` the package has no module `{module}` `` | static, from the module's text |
| P9 (M4) | `` `variant: "leave_delegator"` needs `expr`: the expression that reaches the new type from `self` `` / `` `expr` on `retarget_impl` is the delegator's receiver, so it needs `variant: "leave_delegator"` `` | field pairing |

`this seam cannot be cut here:` — the plan is well formed and the code will not permit it; **resolved (`check --deep`, `apply`) before anything is written**.

| # | Message stem | When |
|---|---|---|
| S1 | `` the anchor names no `impl` of this file `` / `` the members anchored sit in more than one `impl` block `` / `` `{name}` is not a member of an `impl` `` | the lowered range does not lie in one impl |
| S2 | `` the range cuts `{member}` in half: it ends at line {n} and the member goes on `` | partial member (the rule `move_item` states for items) |
| S3 | `` `{Old}` is already the self type of this `impl`: there is nothing to retarget `` | `to_type`'s last segment equals the block's self type |
| S4 | `` `{member}` reads `self.{field}`, which `{New}` does not declare (its fields: {a, b}) `` — one refusal naming **every** member/field pair | a moved member uses `self.f` or `Self { f: .. }` and `New` lacks `f`; also `` `{New}` is an enum / a union, which has no field `{field}` `` |
| S5 | `` `{New}` is declared by a macro or as a type alias in `{file}`, so its fields cannot be read: retarget only members that read no field `` | the declaration is not a plain struct the engine can read, and a moved member reads a field |
| S6 | `` `{Name}` is already bound in `{file}` to `{other}`: a `use` of `{path}` would clash (`E0255`) `` | the file binds the name to something else |
| S7 (M4) | `` `{member}` takes `{pattern}` as a pattern, so a forwarding method cannot name it `` / `` `{member}` is an associated const or type: it has no body to forward `` / `` `{member}` is a `const fn`, which cannot forward to a non-const call `` | the delegator cannot be written |

`rust-analyzer's answer was unusable:` — retry against a warm server: the outline holds no impl at the range, or the reference request fails.
`the tree does not compile before the plan runs:` / `… were applied, and the tree no longer compiles:` — the compile gate, unchanged; **this is what a caller left pointing at the old type produces (`E0599`)**, and it is the documented outcome, not a defect.

#### The block-splitting rule

The anchor lowers to a line range `R` in file `F`; the outline gives the impl block `I` (kind 19) holding it and `I`'s member children. `M` = the members whose first line (their doc comments, attributes and the comments attached to them
included, by the rule `attached_trivia_starts_at` already encodes) lies in `R`; a member cut in half is S2. `P` = the members of `I` before `M`, `A` = those after. **Edits are assembled as a whole new text of `F` and handed to `seam_survey::minimal_edits`.**

1. **`M` is all of `I`'s members** (a whole-block anchor, or a run covering every member): the self type in the header is replaced by `to_type`'s last segment as written (generic arguments included). The `impl<…>` parameter list, the `where` clause, every
   attribute and every member are left as bytes.
2. **`M` is a proper subset**: the block becomes up to three blocks, **in place, in source order**: `[Old: P] [New: M] [Old: A]`, an empty piece omitted.
   - An `Old` block's header is the **original header text, copied by byte range** (attributes included, the block's `///` doc comments on the first block only).
   - The `New` block's header is the original header with the self type replaced; its attributes are copied; no doc comment.
   - Cuts fall on the member boundaries: the closing `}` and the next header go **before** the first line of `M`'s first member (its attached trivia included) and **after** the last line of `M`'s last member. Blank lines and free-standing comments between members stay with the piece they follow.
   - If `P` is empty the first block (`New`) takes the original header's place and `A`'s `Old` block gets a repeated header; if `A` is empty there is no trailing block.
3. **Whole-block with a delegator (M4)** is the same geometry: the old header is kept around the delegators (`Old: delegators`), then `New: M`.
4. **No code is retyped**: members, attributes and comments are byte ranges of the source; the only text the engine authors is the header's self type, the repeated `impl` headers and the `use` line (and, in M4, the forwarding bodies).

Before and after, a proper subset:

```rust
impl Host {                                   impl Host {
    pub fn get(&self) -> u32 { self.n }           pub fn get(&self) -> u32 { self.n }
    /// Replace the count.                    }
    pub fn put(&mut self, v: u32) { … }       impl Roster {
    pub fn last(&self) -> u32 { self.n }          /// Replace the count.
    pub fn size(&self) -> u32 { … }               pub fn put(&mut self, v: u32) { … }
}                                                 pub fn last(&self) -> u32 { self.n }
                                              }
                                              impl Host {
                                                  pub fn size(&self) -> u32 { … }
                                              }
```

#### Re-pointing paths inside the block

For each moved member `m` with a name, the server's `textDocument/references` at `m`'s name position (the `item_move::sites_of` request, `includeDeclaration: false`) returns every site that names it. A site is **re-pointed** when:
(a) its byte offset lies inside the moved members' range in `F`, **and** (b) the text before it is `Old::`, optionally preceded by a module qualifier chain (`a::b::Old::m`). The whole `[chain::]Old::` prefix is replaced by `New::`.

What is **not** touched: a site outside the moved members (a caller anywhere — delegator or `repoint_call`); a site written as a method call (`self.m()`, `x.m()`: the method now belongs to `New`, the text is right); `Old::other` where `other` was **not** moved (it still resolves on `Old`); a bare `Old` in type position or in
a struct literal; sites in comments, strings and doc comments (the server does not report them, and `///` links are `move-fidelity`'s problem); sites inside a macro invocation the server does not report (the compile gate catches them).

`to_type`'s module is added to `F` as `use crate::<module path>::<Name>;` placed by `item_move::text::use_insertion`, **unless** `F` already binds `<Name>` to the same path (nothing written), `F` is the module that declares `New` (nothing written), or `F` binds `<Name>` to something else (S6). The old type's `use`, if now unused, is removed by the apply's tidy at the end of a **complete** run, as for `move_item`.

#### The field refusal (S4), named before apply

`check --deep` rehearses the operation through the same `resolve` an `apply` runs (as for every operation), and the field check is the first thing `resolve` does after the outline, **before any edit is built**, so the finding appears in `check --deep` and the refusal in `apply`, with the tree untouched.

1. Parse the moved members' text with `syn` (`ItemImpl` over the block, header included, so `Self` is meaningful) and `syn::visit` it: collect `self.<ident>` and `self.<index>` field expressions (a receiver `self`, not a method call), and the field names in `Self { f: .. }` expressions and patterns.
2. Read `New`'s declaration with `syn::parse_file` over the module file `to_type` names (inline modules followed by the module chain): a struct yields its field names (named) or indices (tuple); an enum or a union is S4's second form; anything the file does not declare plainly as a struct (a macro, an alias) is S5 **only if** a field is read.
3. Every field read that `New` does not declare is one entry of S4. **Methods are not checked**: a moved member that calls `self.helper()` where `helper` stays on `Old` is not refused (see Decisions O4); the compile gate names it.

#### The delegator (M4, separable)

`"variant":"leave_delegator","expr":"<recv>"` keeps, in the **old** type's block, one forwarding method per moved **method** (a member with a receiver):

```rust
impl Host {
    pub fn get(&self) -> u32 {
        self.roster().get()
    }
}
impl Roster {
    pub fn get(&self) -> u32 { self.n }
}
```

- Signature: the member's text from its visibility to the body's opening `{`, **verbatim** (generics, `where`, `async`, `unsafe`, return type); outer attributes copied (`#[cfg]`, `#[must_use]`, `#[inline]`, `#[allow]`), **doc comments not** (they stay on the real method). Visibility is the member's own.
- Body: `<recv>.<name>(<argument names, in order>)`, followed by `.await` for an `async fn`. An associated function with no receiver forwards through the new type: `New::<name>(<argument names>)`.
- Every moved member gets one (Decision O2's alternatives: only those with outside callers, which needs the reference survey). A member that cannot forward is S7: a pattern parameter, an associated const or type, a `const fn`.
- `expr` is evaluated in the old type's `self`; the engine does not check that it yields the new type (the compile gate does).
- **Not done**: reporting a delegator with no caller left (the todo's last line) — a follow-up.

#### Accounting in `restructure verify` (M3; the delegator rule is M4)

`verify` takes a **declaration**: `--retarget OLD=NEW`, repeatable, one per `retarget_impl` (bare type identifiers; the last segment of `to_type` and the old self type; generics stripped). Without a declaration `verify` behaves exactly as today.

With `Declared { retargets }`, two rules join the passes **between visibility pairing (pass 3) and re-point pairing (pass 4)**:
- **R1, rename pairing.** A lost statement and a gained statement pair 1:1 when the gained one equals the lost one with every whole-identifier token `OLD` (outside strings, comments and lifetimes) replaced by `NEW`, compared on the same key pass 4 uses (visibility and lowercase qualifiers ignored). This pairs `impl OLD {` -> `impl NEW {` (whole block) and `OLD::build(…)` -> `NEW::build(…)`.
- **R2, header accounting.** After R1, each declared retarget excuses at most **two** further gained statements, each an **inherent** `impl` header (line starts `impl`, ends `{`, no ` for `): one whose self type is `NEW`, and one whose self type is `OLD` and which equals (visibility ignored) a statement the ref already has (the repeated header of the trailing piece). Nothing else is excused by it.
- Both count into `Excused::repointed` (**no new response field**: `VerifyResponse` says its counts are the same three everywhere).
- What stays reported: a rename that was not declared, any changed argument, any lost statement or comment, a retarget to a different type than declared.
- **Honest limit, to be written in the docs**: verify proves "the differences are only of the shape a declared retarget produces", not that the plan did them; a hand edit that renames `OLD` to `NEW` is excused too, because the author declared it.
- **M4 adds R3**: with a declared delegator, a gained `fn` signature statement equal to one the ref already has (the duplicate the delegator is) pairs with the gained forwarding statement `<recv>.<name>(<the parameter names>)` / `….await` / `NEW::<name>(…)`, 1:1, counted into `repointed`.

The declaration travels as `RestructureVerifyArgs.retarget: Vec<String>` -> `Options.retargets` -> `runner::verify` -> `verify::compare_with`, and as `VerifyRequest.retargets` (proto field 3) through `tddy-tools`'s `index_client.rs::verify` and the daemon's `cli.rs` / `queries.rs`.

### Delta (What's Changing)

#### `tddy-code-restructuring`
- **`plan/refactor_kind.rs`**: `RetargetImpl` (short doc). **`plan.rs`**: `to_type` field (short doc). **`plan/codec.rs`**: `mod retarget_fields;` and one call in `parse_op`. **New `plan/codec/retarget_fields.rs`**: P1-P6, P9 and the `to`/`name` refusals, `one_type` + path check for P2.
- **`backends/rust.rs`**: `mod retarget_impl;`, `SUPPORTED` 22 -> 23, `check` arm -> `retarget_impl::findings`, `resolve` arm -> `self.retarget_impl`.
- **New `backends/rust/retarget_impl.rs`** and children (proposed): `impl_run` (the member-level outline reading, S1-S3), `fields` (S4/S5, `syn`), `split` (the block-splitting rule and the header text), `repoint` (the reference filter and the prefix replacement), `imports` (the `use`, S6), `delegator` (M4). **Each <= 500 production lines, each function <= 150.**
- **`backends/rust/item_move/text.rs`**: `use_insertion` and `scope_of` widened to `pub(in crate::backends::rust)`. **Nothing else of `item_move` is touched.**
- **`verify.rs`**: `compare` delegates to `compare_with`; a closure-taking `pair_by` variant. **New `verify/retarget.rs`**: `Declared`, `Retarget`, R1, R2 (R3 in M4).
- **`restructure_args.rs`** (`--retarget`), **`runner/options.rs`** (`retargets`), **`runner/comparison.rs`** (hands the declaration to `compare_with`).
- **The 20 full `RefactorOp` struct literals in 15 files** gain `to_type: None`, as **this node's own first commit** (pay as you go, developer-approved 2026-10-05; mechanical; `cargo check -p tddy-code-restructuring --all-targets` lists any left out). Measured on `a77bca29` with `git grep -n 'RefactorOp {' -- packages | wc -l` (80 textual sites: 2 definitions, 41 signatures, 17 `..base` forms that need no edit, 20 full literals); cross-check `git grep -n 'order: Vec::new()' -- packages/tddy-code-restructuring | wc -l` is 20 in 15 files.

#### `tddy-index-daemon`
- `proto/code_index.proto`: `repeated string retargets = 3;` on `VerifyRequest`. `src/cli.rs`: the request carries `--retarget` values; the unit test `verify_carries_only_the_tree_and_the_ref_it_holds_it_against` is renamed and extended. `src/queries.rs`: `comparison_against` puts them in `Options`.

#### `tddy-tools`
- `src/index_client.rs::verify`: fills `retargets` from `RestructureVerifyArgs`.

## Implementation milestones

- [x] **M0** Probe (a live test binary run once, kept as `retarget_impl_acceptance`'s first test): outline of `impl Host { fn a; fn b }` is kind 19 with two member children; references at `Host::build` (an associated function) include `Host::build(..)` inside the same impl; `minimal_edits` over a split returns hunks. **All three premises held, see "M0 probe results" below** (run 2026-10-05 against the live rust-analyzer)
- [ ] **M1** Plan surface and the whole-block retarget (the `to_type` literal commit already landed first, ahead of M0); `RetargetImpl`; `retarget_fields.rs`; `SUPPORTED`; static `check` (P1-P8); `resolve` for a whole block; the `use`; comments kept. Tests 1-10 (the plan lines), 11-13 and 17 (the probe, the whole-block retarget, comments kept, the generic header) pass
- [ ] **M2** The block split, the path re-points, S1-S6: tests 14-16 and 18-23 (splits, re-points, the field refusal, the clash, the caller left behind) pass
- [ ] **M3** `verify` accounting for a declared retarget: `Declared`, R1, R2, `--retarget`, the request field, the proto, the daemon and CLI plumbing; tests 24-30 (the library tests, the CLI-plus-daemon test, the daemon unit test) pass
- [ ] **M3 decision point**: measure the diff (`git diff --stat` against the base) and `backends/rust/retarget_impl/` production lines. **If the diff exceeds ~1,400 lines or the module ~350 production lines, cut M4 to a follow-up node** and narrow the delegator todo; otherwise continue (Decision O2)
- [ ] **M4** (separable) the delegator: `variant: "leave_delegator"` + `expr`, P9, S7, the emitter, R3; tests 31-35 (the delegator live tests, the P9 and R3 library tests) pass
- [ ] **M5** Registration (`.config/nextest.toml`, `.config/rust-e2e.filterset`), docs staged for wrap, changeset updated
- [ ] **M6** Scoped gate: `./test -p tddy-code-restructuring -p tddy-index-daemon -p tddy-tools`; clippy and fmt on the three; the length gate (`restructure check --budget 500`) over `plan.rs`, `plan/refactor_kind.rs`, `plan/codec.rs`, `verify.rs` and the new files, once at the end

## Testing plan

### Testing Strategy

**Primary Test Approach: a live rust-analyzer fixture crate with `cargo check` as the assertion**, for everything that edits source (the whole-block retarget, the split, the re-points, the refusals that need the outline, the caller left behind).
This is what `move_item` does and for the same reason: no edit that merely looks right passes a compiler, and a caller left pointing at the old type, a path left naming the old type, a header with the wrong self type, or a `use` missing all fail it. The price is
tens of seconds per test (`SKILL.md:105`), so the **parse-time and static refusals, and `verify`'s accounting, are library tests in milliseconds**, and only the declaration's travel through the CLI and the daemon is a process-level test.

#### Option 1: live fixture crate (chosen for editing behaviour)
**Test Level**: Integration / E2E against a live rust-analyzer (`#[tokio::test(flavor = "multi_thread")]`, one server at a time by the harness mutex).
**Assertions**: the file's exact text for the affected blocks (headers, which members sit in which block, comments), `cargo check` clean, the file byte-identical after a refusal.
**Implementation Location**: `packages/tddy-code-restructuring/tests/retarget_impl_acceptance.rs`, `tests/retarget_impl_delegator_acceptance.rs`. **Both are new live binaries: add each to `.config/nextest.toml` (the `rust-analyzer` test group) and to `.config/rust-e2e.filterset`.**

#### Option 2: library level, no server (chosen for refusals and `verify`)
**Test Level**: Unit / integration over public entry points (`Plan::parse`, `runner::check` static, `verify::compare_with`).
**Implementation Location**: `tests/retarget_impl_plan_lines.rs`, `tests/verify_accounts_for_a_retarget.rs`.

#### Option 3: CLI + daemon (chosen, thin)
**Test Level**: E2E over the real binaries, the style of `tddy-index-daemon/tests/dual_transport_acceptance.rs` (which already drives `verify` through both).
**Trade-off**: slower than Option 2; one case only, to prove the declaration crosses the wire.

#### Option 4: a `plan.rs` unit test per field (rejected)
**Why not**: `plan.rs`'s inline test module is ~1,150 lines and the file budget is the reason this stack exists; the plan-line tests go in a new library file.

### Coverage Requirements

- [ ] **Happy path**: whole block; a proper subset in the middle, at the start and at the end
- [ ] **Error scenarios**: P1-P8, S1-S6 (S7, P9 with M4)
- [ ] **Edge cases**: a generic header, a block with `#[cfg]`, a banner comment between members, doc comments and `#[must_use]` on a moved member, `Old::build` (moved) and `Old::LIMIT` (not moved) in one body
- [ ] **Integration points**: the compile gate on a caller left behind; the declaration through CLI and daemon
- [ ] **Actual effects**: file text and a compiling tree, never only a return value

## Acceptance tests

Names read as behaviour specifications. Each is **red on `master` today**; the reason is given. **Contract commit, what each test does today** (the "fails today" lines below are relative to `master`; the published surface makes some of them pass, as the wave-2 contract requires: the codec, the flag and the field exist). **Red on this branch:** 8 (P7), 9 (P8), 12-23 (live; refused as unimplemented), 24, 26, 27a, 34, 35 (library), 29 (CLI + daemon). **Green on this branch, by design:** 1-7 (the published codec), 10, 11 (a probe of the server, not of the operation), 25 and the guards in 27/28 (they pin what `verify` must keep reporting), 30 (the carrier). **Differences from the list below:** tests 24-26 use a re-pointed call and a `where` header, not `impl Host {` -> `impl Roster {` (see "Findings the tests surfaced"); test 20 anchors on `<Host>#1` and `<Host>#2` because the anchors command refuses members of different blocks; tests 12-23 spell members as full item paths; test 11's third premise is a unit test beside `retarget_impl.rs`; test 30 keeps its name from the plan body (`verify_carries_the_tree_the_ref_and_the_retargets_it_is_told_of`).

Fixture: one committed package `app` built with `same_crate::an_app_holding`, `lib.rs` = `pub mod host;\npub mod roster;\n`; `roster.rs` declares `pub struct Roster { pub(crate) n: u32 }`; `host.rs` declares `pub struct Host { pub(crate) n: u32 }` and an `impl Host` of the members each test names.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/retarget_impl_plan_lines.rs` (new; library level, no server)

1. `a_retarget_impl_line_parses_and_reads_back_without_its_defaults` — `{"op":"retarget_impl","anchor":<items>,"to_type":"app::roster::Roster"}` parses to `RefactorKind::RetargetImpl` with `to_type`, and `Plan::to_jsonl` writes it back with no empty field. *Fails today*: `unknown variant retarget_impl`.
2. `a_retarget_without_to_type_is_malformed` (P1) / 3. `to_type_must_be_one_path_type` (P2: `&Roster`, `dyn T`, `A, B`, `(A, B)`) — each `plan is malformed:`. *Fail today*: unknown variant.
4. `to_type_on_another_operation_is_refused_naming_it` (P3, on `move_item`) — *fails today*: `unknown field to_type` (the refusal reads differently, and a test asserting on `retarget_impl` in the text fails).
5. `a_range_or_symbol_anchor_names_no_impl_to_retarget` (P4) and 6. `a_trait_impl_path_is_refused_at_parse_time` (P5, item path `app::host::<Host as Display>::fmt`) — *fail today*: unknown variant.
7. `to_and_name_are_not_fields_of_retarget_impl_and_the_refusal_points_at_to_type` (P6).
8. `a_to_type_in_another_package_is_refused_by_a_static_check` (P7) and 9. `a_to_type_whose_module_declares_no_such_type_is_refused_by_a_static_check` (P8) — through `what_a_static_check_finds_in`, **no server** (as `check_precondition_parity.rs`). *Fail today*: unknown variant.
10. `a_plain_check_reports_an_item_anchored_retarget_as_unexamined_rather_than_passing_it` — the existing rule for item anchors (`unresolvable_without_a_server`) applies to the new operation. *Fails today*: unknown variant.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/retarget_impl_acceptance.rs` (new; **live rust-analyzer**, `cargo check` as the assertion; register in `.config/nextest.toml` and `.config/rust-e2e.filterset`)

11. `probe_the_outline_the_references_and_the_hunks_of_a_split` (M0) — asserts the three premises the changeset names as unverified. *Fails today*: unknown operation.
12. `retargets_a_whole_impl_block_to_another_type_and_the_tree_compiles` — anchor `<Host>` (emitted by the `anchors` command), `to_type: app::roster::Roster`. The file reads `impl Roster {` and no `impl Host {`, gained `use crate::roster::Roster;`, `Host`'s struct untouched, and `cargo check` passes. *Fails today*: unknown variant `retarget_impl`.
13. `keeps_every_comment_attribute_and_doc_line_of_the_moved_members` — a `/// doc`, a `#[must_use]`, an inner `// banner` survive byte for byte. *Fails today*: same.
14. `splits_the_block_at_the_anchored_members_when_they_are_a_proper_subset` — `get, put, last, size`, run `put, last`: three blocks `impl Host { get }`, `impl Roster { put, last }`, `impl Host { size }`, the two `Host` headers identical to the original, and the tree compiles (`size` calls `self.get()`, which stayed). *Fails today*: same.
15. `a_run_at_the_start_or_the_end_of_the_block_leaves_no_empty_block` — two geometries, `P` empty and `A` empty. *Fails today*: same.
16. `a_banner_comment_between_members_stays_with_the_side_it_follows` — a free-standing `// ---` between `get` and `put` stays in the `Host` block; one attached to `put` moves. *Fails today*: same.
17. `keeps_the_generic_parameter_list_and_where_clause_of_the_header` — `impl<T> Wrapper<T> where T: Copy`, retargeted to `app::roster::Pair<T>`; both headers keep `<T>` and the `where`. *Fails today*: same.
18. `re_points_a_path_that_names_a_moved_associated_function_through_the_old_type` — `rebuilt` calls `Host::build(self.n)`, both moved: reads `Roster::build(self.n)`; and `Host::LIMIT` (not moved) is left as written; compiles. *Fails today*: same.
19. `names_a_field_the_new_type_lacks_before_anything_is_written` — `bump` reads `self.count`, `Roster` declares `n`: `check --deep` returns one finding naming ``bump``, ``self.count`` and `` (its fields: n) ``; `apply` returns the `this seam cannot be cut here:` refusal and `host.rs` is **byte-identical** to before. *Fails today*: same.
20. `refuses_an_anchor_whose_members_sit_in_two_impl_blocks` (S1), 21. `refuses_to_retarget_an_impl_to_the_type_it_already_is` (S3), 22. `refuses_a_use_that_would_clash_with_a_type_the_file_already_binds` (S6: `use crate::other::Roster;` in `host.rs`). *Fail today*: same.
23. `a_caller_left_pointing_at_the_old_type_fails_the_compile_gate_and_is_left_alone` — `caller.rs` calls `h.get()` and is not in the plan: `apply` fails with `no longer compiles` naming `E0599`, `caller.rs` byte-identical (the operation edits no other file). *Fails today*: same.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/verify_accounts_for_a_retarget.rs` (new; library level, no server)

24. `a_whole_block_retarget_is_accounted_for_once_it_is_declared` — before `impl Host {\n    fn get(&self) -> u32 {\n        self.n\n    }\n}`, after the same with `impl Roster {`, declared `Host=Roster`: `holds()`, `excused.repointed >= 1`. *Fails today*: no `compare_with`/`Declared`.
25. `the_same_change_is_reported_when_no_retarget_is_declared` — same texts through `compare`: `!holds()` (the guard against a verify that excuses an `impl` rename unconditionally). *Passes today, and must keep passing* (it pins the guard).
26. `a_split_retarget_accounts_for_the_two_headers_it_adds` — before one block, after `impl Host { get } impl Roster { put } impl Host { size }`: holds with the declaration, and **reports** the two added headers without it.
27. `a_path_re_pointed_through_the_old_type_pairs_with_its_original` — `Host::build(1)` -> `Roster::build(1)`; and `a_changed_argument_is_still_reported` (`Host::build(1)` -> `Roster::build(2)`); and `a_rename_to_a_type_other_than_the_declared_one_is_not_excused`.
28. `a_declared_retarget_does_not_excuse_a_lost_comment` — a `// banner` missing after the retarget is still reported.

### `tddy-index-daemon` — `packages/tddy-index-daemon/tests/dual_transport_acceptance.rs` (existing; CLI and daemon, already registered in `.config/rust-e2e.filterset`)

29. `verify_carries_a_declared_retarget_through_the_cli_and_the_daemon_and_both_render_the_same_lines` — in a git tree where `impl Host` became `impl Roster`, `restructure verify --against HEAD --retarget Host=Roster` through the CLI cold path and through the daemon prints the same lines and holds; without the flag both report it. *Fails today*: `unexpected argument '--retarget'`.
30. (unit, `packages/tddy-index-daemon/src/cli.rs`) `verify_carries_the_tree_the_ref_and_the_retargets_it_is_told_of` replaces `verify_carries_only_the_tree_and_the_ref_it_holds_it_against`. *Fails today*: no `retargets` field on `VerifyRequest`.

### `tddy-code-restructuring` — `packages/tddy-code-restructuring/tests/retarget_impl_delegator_acceptance.rs` (M4 only; new; **live**; register as above)

31. `leaves_a_forwarding_method_on_the_old_type_so_callers_keep_compiling` — `caller.rs` calls `h.get()`; with `variant: "leave_delegator"`, `expr: "self.roster()"`, `Host` keeps `pub fn get(&self) -> u32 { self.roster().get() }`, `caller.rs` byte-identical, compiles. *Fails today*: unknown variant.
32. `forwards_an_async_method_with_await_and_an_associated_function_through_the_new_type`.
33. `refuses_a_delegator_for_a_member_whose_parameter_is_a_pattern_a_const_fn_and_an_associated_const` (S7).
34. `a_delegator_variant_without_an_expression_is_malformed_and_an_expression_without_it_too` (P9; library level, in `retarget_impl_plan_lines.rs`).
35. `a_declared_delegator_is_accounted_for_and_an_undeclared_forwarding_method_is_reported` (R3; library, in `verify_accounts_for_a_retarget.rs`).

## Technical Debt & Production Readiness

(empty; populated during development)

## Decisions & Trade-offs

Decisions already taken by the developer, from the stack brief (quoted):

- "8-node decomposition approved (2026-10-05)." This is node 6 of 8.
- "Prep node: ADD the mechanical node first (the developer overrode the recommendation to decline)." That is `tidy-engine-files`, this node's only parent.
- The brief's scope for this node: "MUST teach `restructure verify` to account for it (`verify.rs` only excuses deleted lowercase `::` qualifiers today)" and "if the block split alone is already large, the delegator is its own milestone that may be cut to a follow-up."
- "C2a: calls and receivers only" (`repoint-call`'s scope): so this node does not rewrite `self.field`.
- **`RefactorOp` fields: pay as you go (developer-approved 2026-10-05).** This node adds `to_type` and edits the 20 full `RefactorOp` struct literals in 15 files in its own first commit, checked with `cargo check -p tddy-code-restructuring --all-targets`; `tidy-engine-files` does not migrate them. See O3 for the measurement.
- **`RefactorKind` lives in `plan/refactor_kind.rs` (developer-approved 2026-10-05, `tidy-engine-files` D1).** The `RetargetImpl` variant is added there; `RefactorOp` and its new field stay in `plan.rs`.

**OPEN decisions** (not settled by the brief; each has a recommendation, and the developer decides):

- **O1 — how a declaration reaches `verify`** (verify takes a git ref and nothing else, and is routed through the daemon).
  (a) **`--retarget OLD=NEW`, repeatable** — *recommended*: stateless, auditable, one line per operation, safe in the failure direction (a typo excuses less, never more). Cost: a request field on `VerifyRequest` and plumbing in three packages.
  (b) `--plan PLAN` (repeatable): the plan is the declaration of intent and the natural source, but the daemon would read plan files, and **it is unverified that an applied plan still carries a completed operation's anchor as written** (`plan-store.md` says only pending operations are refreshed, SKILL.md step 9 says each operation's anchors are written back): the old type would be unrecoverable if it does not.
  (c) the journal under `.restructure/`: no flag, but untracked, cleared between plans (SKILL.md's two-plan recipe), and absent on another machine, so a CI `verify` could not reproduce a local one.
  (d) infer from the two trees: cannot tell a retarget from a hand edit; weakens the one check that exists to show hand edits.
  (e) document that `verify` reports retargets and make no change. **Rejected by the brief ("MUST teach").**
- **O2 — the delegator: in this PR or cut.** Recommendation: **build M1-M3, then decide at the M3 decision point** (diff over ~1,400 lines or the module over ~350 production lines: cut). If cut, the delegator todo is narrowed, not deleted, and `verify`'s R3 goes with it.
  Its schema, if built: **`"variant":"leave_delegator"` plus the existing `expr`** — recommended, because the todo says "`leave_delegator` as a **variant** of `retarget_impl`", `variant` already means "which of several actions an engine offers for the same operation", and it adds **no `RefactorOp` field** (a second 20-literal churn).
  The brief writes "`leave_delegator`" as if a field; a boolean field is the alternative. Which members get one: **every moved method** (recommended; deterministic, no server) vs only those with outside callers (needs the reference survey, and the todo's "report any wrapper with no caller left" is that survey's other half, so it belongs with a follow-up).
- **O3 — the field that names the new type.** (a) **a new field `to_type`** — recommended and the brief's word (the developer's 2026-10-05 note on `RefactorOp` fields names it): explicit, and distinct from `to` (a module path for moves) and `type` (a signature type). Cost: 20 full struct literals in 15 files (measured: `git grep -n 'order: Vec::new()' -- packages/tddy-code-restructuring | wc -l`), done as one mechanical commit first in this node and listed by `cargo check -p tddy-code-restructuring --all-targets`.
  (b) reuse `to`: no new field, no churn, and `plan.rs` stays shorter (relevant to the 500-line budget), but `to` means "a module" for every other operation that has one, and the plan would not say a type is meant.
  (c) reuse `type`: gets `one_type` for free, but `type` is documented as a signature type for three other operations.
- **O4 — methods the moved members call on `self` that stay on the old type.** (a) **do not refuse** — recommended: the compile gate names them, and refusing would make `retarget_impl` unusable with `repoint_call` in one `group`, the pairing the todo is about; the brief asks only for a field refusal. (b) refuse (`this seam cannot be cut here:` naming each). Costs a method-resolution pass the engine does not have.
- **O5 — trait impls.** **Refuse at parse time** (P5), recommended. A trait impl cannot change type without moving the trait's contract (`E0119` if `New` already implements the trait); out of this node. (The alternative is a later node.)
- **O6 — a bare `Old` in type position inside a moved member** (`fn merge(&self, other: &Old)`). **Leave as written** (recommended): after a retarget the author may mean either, and `Old::<moved member>` is the only path the todo names. Re-pointing every `Old` would turn an explicit `Old` constructor into `New`.
- **O7 — placing the new block in the new type's module.** **Not done here** (recommended): compose with `move_item` over `<New>`; the operation changes a type, not a file.
- **O8 — whether the new `impl` block takes the original `impl`'s doc comments.** **First block only** (recommended): duplicating prose onto two blocks is a documentation change the plan did not ask for.

Decisions taken in the contract commit (2026-10-05), each the changeset's recommendation:

- **O1** — the declaration is `--retarget OLD=NEW` plus `VerifyRequest.retargets = 3` (recommended option (a)). Published: `RestructureVerifyArgs.retarget`, `Options.retargets`, the daemon CLI's `VerifyArgs.retarget`, `verify::Declared`/`Retarget` (`FromStr` reads `OLD=NEW`, two different bare identifiers) and `verify::compare_with`, which carries the declaration and does not read it yet.
- **O2** — **not decided here**: the decision point is M3. The delegator's tests exist (31-35) and fail; the codec already accepts `variant: "leave_delegator"` with `expr` (so the live tests reach the engine and are refused for the missing implementation), and P9 (the pairing) is not enforced. If M4 is cut, delete `tests/retarget_impl_delegator_acceptance.rs`, its two registrations, test 34 and test 35, and narrow the delegator todo. The declaration of a delegator for test 35 is the retarget itself; a separate carrier (`VerifyRequest` field 4) is for M4 to add.
- **O3** — `to_type` (recommended (a)).
- **O4-O8** — taken as recommended; nothing in the published surface depends on them.
- The unimplemented operation is refused with `UnsupportedOp` (backend text names node `retarget-impl`), not a `SeamRefused`/`MalformedPlan`, so no test that waits for an `S`- or `P`-class refusal can pass on the wrong refusal.

Decisions taken by this plan (a reviewer can check them):

- Edits are built as a whole new text and given to `seam_survey::minimal_edits`, as `move_item` does, so **no `Edit` type is widened**; only `use_insertion` and `scope_of` change visibility.
- The member-level outline reading is new code (the existing function refuses exactly that range); the header is read **lexically**, not from the outline's name text (unverified).
- References are requested at each moved member's name; no `definition` request exists or is added.
- `verify` folds the retarget pairs into `repointed`: no response field.
- A caller left behind is **documented as the compile gate's outcome** (test 23), not pre-checked.

### M0 probe results (2026-10-05, live rust-analyzer, `probe_the_outline_the_references_and_the_hunks_of_a_split`)

- **Outline.** `textDocument/documentSymbol` answers the hierarchical form. An `impl Host { .. }` is one item with **`kind` 19**, **`name` `"impl Host"`** (the type as written, generics presumably included: only the non-generic form was probed), `range` over the whole block (line of `impl` to its closing `}`, 0-based), `selectionRange` over the type name only. Its **children are its members**: a method with a receiver is **`kind` 6**, an associated function with none is **`kind` 12** (Function), each with its own `range` and a `detail` carrying the signature (`fn(&self) -> Host`). Mind that members of **kind 12** sit under an impl too: the member-level outline reader must not filter on kind 6. The name text of the impl is *not* needed (the header is read lexically, as decided).
- **References.** `textDocument/references` at the name of an associated function (`build`) with `includeDeclaration: false` **does** return the call written `Host::build(self.n)` inside the same `impl`; the site's range covers the bare name `build`, so the `Host::` before it is the text before the site, exactly as the re-point rule reads it.
- **Hunks.** `seam_survey::minimal_edits` over a split of a four-member block returns **four insertion-only hunks, two at each cut** (`}\n` then `impl Roster {\n`; `}\n` then `impl Host {\n`), never a hunk over a member. Pinned by the unit test `splitting_a_block_gives_insertions_at_the_two_cuts_and_leaves_every_member_alone` (a pure function of two texts, so a unit test beside the module rather than a live one).

### Findings the tests surfaced (the green wave must read these)

- **`verify` does not read `impl Host {` as a statement.** `is_structural` (`verify/statements.rs`) drops every line starting `impl ` with the others `use`, `mod` and bare braces, so the self type of a **non-generic header changing, and a block being split, are already invisible** to `verify` on `master`. The changeset's State A ("`impl Host {` -> `impl Roster {` pairs through none") and the whole of **R2 as written** (excusing two `impl` headers ending in `{`) rest on a premise that is false for that shape. What `verify` does read, and reports today: (1) **the `Host::` of a re-pointed path** (`Host::build(..)` -> `Roster::build(..)`): R1 is the rule that is needed; (2) a **generic or `where`-bearing header**: `impl<T> Host<T>` does *not* start with `impl ` (no space), and the lines of a `where` clause are statements, so a split repeats `impl<T> Host<T>`, `where` and `T: Copy,` once per added block. R2 must be rewritten to excuse **those** (the repeated header's own lines), not a `{`-terminated line. The tests are written to that truth: tests 24/25/26 use a re-pointed call and a `where` header, and the changeset's literal texts for 24-26 (`impl Host {` -> `impl Roster {`) were replaced because they pass on `master` without any declaration.
- **`RefactorKind` still lives in `plan.rs`** on this branch: `plan/refactor_kind.rs` (`tidy-engine-files` D1) does not exist at `26c5e488`. The variant was added where the enum is; moving it is `tidy-engine-files`'s.
- **The `to_type: None` literals are in this node's second commit**, not a first one: this node's first commit is the plan, and the wave-2 contract is one commit. 21 literals in 16 files (20 in the changeset's count, plus one inside `plan.rs`'s own tests); `cargo check --all-targets` lists any left out.
- **The anchors command wants full item paths** for members: `app::host::Host::put`, `app::host::<Host>` (a bare `Host::put` is refused: "is not a bare item name"). Two impl blocks of one type are `<Host>#1` and `<Host>#2`; items in *different* blocks are refused by the command as "not adjacent" before the engine sees them, so S1 ("the members anchored sit in more than one `impl` block") is reachable only through an anchor over whole blocks, which is how test 20 exercises it.
- **A static finding blocks the deep rehearsal** (`check_plan` skips `resolve` for an operation whose static check found something). The published `findings` therefore reports "not implemented" for every `retarget_impl`, which is honest and stops a `check --deep` from rehearsing; when green implements P7/P8 it must return an empty list for a sound plan or the deep check never reaches S4.

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

## References

- Exemplar of a new-operation vertical slice: `cb50ab5c` (#584), wrapped into `packages/tddy-code-restructuring/docs/same-crate-moves.md`; the codec precedent is `plan/codec/signature_fields.rs`.
- Stack brief and whole-work discovery: `docs/dev/1-WIP/2026-10-05-engine-fixes-whole-work-initial-discovery.md` on the planning branch (copied in full into this node's companion as Exploration 1).

## TODO

- [x] Record initial discovery (`2026-10-05-sharpen-retarget-impl-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation (`docs/ft/coder/1-WIP/PRD-2026-10-05-sharpen-retarget-impl.md`)
- [x] Create changeset (this document)
- [ ] Add the PRD reference to `docs/ft/coder/1-OVERVIEW.md` **at wrap** (a shared append-point: not edited while planning, eight nodes would conflict)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-05-sharpen-retarget-impl-initial-discovery.md`, and handles the two #532 todos as stated in Prerequisites (delete only what has reached `master` and is fully resolved)
- [ ] USER REVIEW — work complete, decide next steps
