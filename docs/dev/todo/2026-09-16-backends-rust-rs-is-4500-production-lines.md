# 2026-09-16 — `backends/rust.rs` is 4,571 production lines and every restructuring change grows it

**Category:** Deferred refactor
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, wave-2 structural pass

Measured on **production** lines (everything before the first `#[cfg(test)]`, at line 4,572):

| File | Prod | Test |
|---|---:|---:|
| `tddy-code-restructuring/src/backends/rust.rs` | **4,571** | 2,007 |
| `tddy-code-restructuring/src/backends/lsp_bridge.rs` | 99 | 72 |
| `tddy-code-restructuring/src/backends/mod.rs` | 8 | 0 |

**Update 2026-09-17:** now **4,757** production lines. The
`2026-09-17-restructure-refusal-truth-and-authoring-gates` changeset added +186 — two refusal
constructors, a third tier in `choose_import` with two helpers, two helpers behind
`restore_visibility`, and `refuse_partial_relocation` with its own declaration reader. It moved and
reorganised nothing, deliberately, so the seam table below still holds — though the line numbers in
it have shifted by that much. This is the third consecutive change to grow the file without adding
an operation.

Nine times the ~500-line guideline, and it is the file every change to this crate lands in: the
warm-daemon work grew it by 60 lines without adding a single operation, purely from publishing
`ServerChatter` and threading the cancellation token to the bridge. `runner.rs` had the same shape
and was split during the same change; this one was left because splitting it is a large mechanical
diff that would have buried a reviewable one, which is the trade
[the planning skill](../../../.agents/skills/planning/references/planning-phase.md) names explicitly.

## It is not one thing, and the seams are already visible

The top-level structure reads as seven clusters with almost no interleaving, which is what makes
this a move rather than a redesign:

| Lines | Cluster | Depends on |
|---|---|---|
| 143–323 | handshake capabilities, negotiated encoding, server settings, the assist catalog | nothing in the file |
| 324–519 | `ServerChatter` — `$/progress` + `experimental/serverStatus` folding, `how_far()` | nothing in the file |
| 520–1131 | `RustBackend` construction, the server process, toolchain discovery, `Drop` | the two above |
| 1132–1356 | the `LanguageBackend` and `ModuleReferences` impls | the whole file |
| 1357–2235 | the authored transformations (`extract_class`, the facade `use`, `move_module_to_crate`) | the helper tail |
| 2236–2500 | refusal constructors and seam tracing | little |
| 2500–4571 | ~70 free helpers: LSP geometry (`LspPoint`, `offset_of`, `apply_lsp_edit`), import analysis (`choose_import`, `expand_use`, `bound_names`), placeholder refusals, `minimal_edits` | mostly pure, mostly self-contained |

The last row is the largest and the cheapest to move: those helpers are free functions over `&str`
and `Value` with no access to `RustBackend`, and their tests move with them. `ServerChatter` is the
next cheapest and is now **public** — `tddy-index-daemon`'s `graph.rs` and `warm.rs` consume it, so
it already has an out-of-file consumer and reads as its own module.

## Why it was deferred rather than done

Three reasons, and the third is the one that decides it:

- It is orthogonal to anything the warm-daemon change asked for. No item in that changeset needed a
  line of it moved.
- `rust.rs` is the single most-touched file in this crate, so a 4,500-line reshuffle conflicts with
  any concurrent restructuring work by construction.
- **This crate's own tooling is the right instrument for it.** `extract_module` with `to_file` is
  exactly this operation, and a 70-helper tail is the kind of mechanical carve the operation exists
  for — so this entry is also a candidate live exercise of it, the way `#carve` used
  `tddy-workflow-recipes`. Doing it by hand would be doing by hand the thing the crate automates.

What that needs first is a warm index, because an iterative carve of one file at a cold-start cost
per attempt is what made the earlier spike unaffordable. That is now available
(`./run-index-daemon`), which is why this entry is worth writing now rather than filing as
perennial debt: the reason it was impractical has been removed.

## Not to be confused with

[2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md](./2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster.md)
— that is about moving between *crates*, and this is a within-crate module split. The helper tail is
not entangled, which is the whole reason it is the place to start.
