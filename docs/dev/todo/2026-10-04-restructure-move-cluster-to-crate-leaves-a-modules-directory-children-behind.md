# 2026-10-04 — `move_cluster_to_crate` leaves a module's directory children behind

**Category:** Restructure engine defect
**Source:** `#live-plan` 12/15 (#571), moving `tddy-daemon`'s `index_daemon` cluster to `tddy-lsp-executor` — see the `## Restructuring` section of [2026-10-03-indexing-indicators.md](../1-WIP/2026-10-03-indexing-indicators.md)

## What happens

`move_cluster_to_crate` anchored on `index_daemon` (file `packages/tddy-daemon/src/index_daemon.rs`, with
`also: index_daemon_body`) moves the two lib-level files and **not** the `index_daemon/` directory
beside `index_daemon.rs` that holds the children its `mod error; mod lsp_channel; mod registry;
mod spawn;` declarations name. The destination gets `index_daemon.rs` with four `mod x;` lines and no files
for them.

## Why nothing caught it before the apply

`restructure check --deep` reported `no findings` and `apply --dry-run` reported
`resolved 1 of 1 operations … 6 file(s)`. Only the apply's own compile gate (`cargo check
--all-targets`) caught it: `E0583: file not found for module` for each of `error`, `lsp_channel`,
`registry`, `spawn`, plus `E0433` for a `libc` use in the moved `index_daemon_body.rs` — the
destination's manifest did not gain the moved code's dependencies.

## Reproduce

Self-contained: in a two-crate workspace, give crate `origin` a module `a` declared in `lib.rs`, with
`src/a.rs` containing `mod b;` and `src/a/b.rs` holding anything. Run `restructure check --deep` then
`apply --dry-run` on one `move_cluster_to_crate` naming `a` with `to: <destination crate dir>`. Both report
success; the apply moves `a.rs` and leaves `a/b.rs` behind, and the compile gate fails with `E0583` for
`b`. The original plan was `tmp/restructure/01-registry-to-lsp-executor.jsonl` (gitignored): one
`move_cluster_to_crate` with `reexport: none`, anchored on `index_daemon` with `also: index_daemon_body`.

**Two further symptoms, possibly separate defects:** the dry run said `6 file(s)` and the apply moved two,
so the dry run's count is not derived from what the apply does; and the destination's manifest gained no
`libc` although the moved `index_daemon_body.rs` names it (`E0433`), against `resolve_cluster`'s own doc
that the manifest gains every crate the set names.

## Where to fix it

`packages/tddy-code-restructuring/src/crate_move/cluster.rs`, `resolve_cluster` (L111ff). It emits one
`FileEdit::Rename { from: member.source, to: member.moved_to() }` per member, so a member moves as **one
file**. `moving::Move::of` / `module_home` (`crate_move/module_home.rs`) resolve a module to its file and do
not look for the directory a Rust 2018 module owns (`<module>/…` beside `<module>.rs`). Nothing in
`check --deep` or the dry run looks there either. This is a reading of the code, not a verified diagnosis —
confirm it against the failing test below before changing anything. Note the engine already accepts a
nested module as a member in its own right (`header.rs` L47: "a nested member is declared at the
destination's root under its own last segment"), so listing the children as members may be a workaround.

## Acceptance — write the red test first

In `cluster.rs`'s test module (see `a_workspace_with_an_entangled_pair` and `moves_every_member_of_a_mutually_referencing_set_in_one_edit`
for the fixture style, fluent-tests):

1. A cluster member whose file declares `mod child;` with `<module>/child.rs` on disk is moved **with** its
   directory children, in the same edit (or is refused with an error naming each child it would strand) —
   never moved alone.
2. `apply --dry-run`'s reported file count equals the number of files the apply renames.
3. The destination's manifest gains every crate a moved file names, including one used only by a macro or a
   fully-qualified path in a nested module (`libc` here); if that is a separate cause, split it into its own
   test and entry.
4. `restructure check --deep` reports the stranded children the way it reports other seams, before `apply`.

## Expected

A moved module's directory children move with it (or the operation is refused naming them), and a
dry run / `check --deep` reports the same. The destination's `[dependencies]` gain what the moved code names.

## Routes tried

Every engine-only route was tried before any hand move, each as `check --deep`, then `apply`, with a
rollback (`git reset`, checkout, remove the journal) when the compile gate failed. All failed.

| Route | Plan (`tmp/restructure/`) | `check --deep` | Apply |
|---|---|---|---|
| Cluster of the two lib-level modules | `01-registry-to-lsp-executor.jsonl` | no findings | moved 2 files; `E0583` x4 (children stranded) and `E0433` `libc` |
| Cluster listing the four children as `also` members, anchored on `index_daemon/<m>.rs` | `01a-explicit-children.jsonl` | no findings; dry run `15 file(s)` | children moved **flat to the destination's root** (`error.rs`, `registry.rs`, `spawn.rs`, `lsp_channel.rs` beside `lib.rs`, not under `index_daemon/`): `E0432` on `mod error`/`mod registry`, `E0282` cascade, `E0433` `libc`, and the moved `lsp_channel.rs`/`index_daemon.rs` keep `tddy_lsp_executor::` paths |
| One `move_module_to_crate` per child first, then the cluster | `01b-children-first.jsonl` | **refused**: `plan is malformed: crate::index_daemon::IndexDaemon is one of several paths a single use writes … write one use per path` | not applied (fixing it means hand-splitting a `use` in production code) |
| The cluster first, then one `move_module_to_crate` per child | `01c-parent-first.jsonl` | no findings | op 0 applied, op 1 **refused**: `no parent module file for index_daemon` — the parent had already left; `check --deep` did not see it |
| `move_module_to_crate` of `index_daemon` alone | `01d-single-module.jsonl` | no findings (`0 caller(s)` surveyed, though `runtime.rs` names it) | moved `index_daemon.rs` only; `E0583` x4 |
| Any other op | — | — | none in `plan-schema.md` carries a module's directory children: there is no inline-module counterpart to `extract_module_to_file` |

Two further findings from these runs: `check --deep` and `--dry-run` missed three different failures (above)
that only the apply's gate or its own later operation caught; and the engine left a doc comment stranded on
the next item in `tddy-daemon/src/lib.rs` when it removed the module it described.

## Worked around

**Engine:** `01-registry-to-lsp-executor.jsonl` (one `move_cluster_to_crate`, `reexport: none`) moved
`index_daemon.rs` and `index_daemon_body.rs`. Its compile gate failed as described; the edits were kept.

**By hand, with the developer's consent** (after every route above failed): `git mv` of exactly
`index_daemon/{error,lsp_channel,registry,spawn}.rs` into `packages/tddy-lsp-executor/src/index_daemon/`;
the destination's Cargo edges (`libc`, `tddy-sandbox-runner`, `tddy-daemon-kernel`, extra `tokio` features);
`use tddy_lsp_executor::index_backed::IndexChannel` becoming `use crate::index_backed::IndexChannel` in the
moved `lsp_channel.rs`; the doc comment the engine stranded in `tddy-daemon/src/lib.rs` moved to the
destination's `lib.rs`. Each is marked TODO in the changeset's `## Restructuring` section.
