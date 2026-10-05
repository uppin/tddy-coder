---
name: code-restructuring
description: Restructure Rust code without writing moved code by hand — split modules, extract methods, rename symbols. Plans intents in a Refactor changeset, then executes a JSONL plan via tddy-tools restructure driving rust-analyzer through tddy-lsp.
---

# Code Restructuring (Rust)

**You never write the moved or extracted code.** You write a plan of *intents*. `tddy-tools restructure` resolves each intent through rust-analyzer (via `tddy-lsp`).

**v1 scope:** Rust only — twenty-two operations, ten subcommands. No TypeScript.

## CLI

```bash
tddy-tools restructure apply  <plan.jsonl> [--dry-run] [--resume] [--from N|ID] [--stop-after N]
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check  <plan.jsonl> [--deep] [--budget LINES]   # LINES counts production lines (before `#[cfg(test)] mod`)
tddy-tools restructure snapshot <plan.jsonl>          # item anchors: answered by the warm daemon when TDDY_INDEX_SOCKET is set
tddy-tools restructure anchors <file.rs> --items A,B,C
tddy-tools restructure anchors <file.rs> --at LINE:COL[-LINE:COL]
tddy-tools restructure verify --against <git-ref>
tddy-tools restructure load   <plan.jsonl>...        # needs the index daemon (TDDY_INDEX_SOCKET)
tddy-tools restructure unload <plan.jsonl>... | --all
tddy-tools restructure plans
tddy-tools restructure warm                          # load this tree's crate graph into the index daemon; needs it
```

`load` reads a plan into the daemon once, gives every operation an `id`, and keeps it current: `apply`
runs the loaded plan rather than the file, rewrites the pending operations' anchors after each
operation, and writes the plan back. `apply` loads a plan that is not loaded; `load` is for batches.
`--from ID` is accepted by a run with no daemon; with one, name the index. See
[plan-schema.md](references/plan-schema.md).

## Workflow (abbreviated)

1. **Green baseline** — `./test -p <crate>`, **once, here**; record the counts *and the names of every
   failing test* (a changeset from `/plan-red` has red tests by design; the names are what a later run
   is held against, not the totals). It is the only full test run before the plan is finished — see
   [Testing cadence](#testing-cadence).
2. **Targeting** — run [`analyze-code-issues`](analyze-code-issues/SKILL.md); put CRAP note in changeset.
3. **Understand shape** — LSP outline, references, cohesion; write `docs/dev/1-WIP/{slug}-initial-discovery.md`.
   Decide which operation each seam needs: items into a module of the same crate is `move_item`, a module
   under another parent is `reparent_module`, a cut inside one file is `extract_module`; see
   [Gathering a topic module](#gathering-a-topic-module).
4. **Changeset** — `Type: Refactor` at `docs/dev/1-WIP/YYYY-MM-DD-<name>.md`; see `references/restructure-changeset.md`.
5. **Anchor** — `restructure anchors <file.rs> --items A,B,C` emits an `items` anchor over whole
   items (over a module's `mod` declaration, for `reparent_module`: name the module and pass its old
   parent's file), and `restructure anchors <file.rs> --at 188:9-198:11` emits an `item` anchor for the
   innermost item enclosing the lines you read, with a range relative to it. **Do not hand-write line
   numbers, and paste the emitted JSON as it is.** An item anchor names the item by its path and
   resolves through rust-analyzer's outline, so an edit anywhere *outside* that item leaves it correct;
   an edit *inside* it is refused (`the item … changed since the plan was written`) and the remedy is
   to re-anchor. A hand-counted range routinely clips a helper the moved code needs, and the assist
   then relocates part of the range and rewrites the rest in place. A trait-impl member shared by two
   impls (`fmt`) is refused until the anchor names the trait: `crate::m::<Stack as Debug>::fmt`.
6. **Snapshot** — a v1 plan (`{"v":1,"snapshot":…}`, `range`/`symbol` anchors) is refused on any
   hash drift: `restructure snapshot plan.jsonl` rewrites the header from the working tree, and must
   be re-run after **every** edit to a snapshotted file. A v2 plan (`{"v":2,"files":…}`, item
   anchors) treats its per-file hashes as hints: drift is reported, never refused, so there is nothing
   to re-run after an unrelated edit. `snapshot` of a plan **with item anchors** re-resolves them
   through a language server: with `TDDY_INDEX_SOCKET` set it is asked of the warm daemon (its
   `Snapshot` RPC), and without one it starts a cold rust-analyzer of its own. A plan with no item
   anchors is answered in process either way.
7. **Plan** — JSONL intents only; see `references/plan-schema.md`.
8. **Prove seams** — `restructure check plan.jsonl --deep`. **`--deep` is the gate, not an option.**
   A plain `check` reads text; only `--deep` resolves each operation through the same path `apply`
   uses, so it is the only form that reports an assist or import refusal — and it writes nothing. A
   plain `check` returning `no findings` says nothing about whether the apply will run.
9. **Apply** — `--dry-run` first, then apply; `verify --against HEAD` after. An apply ends with
   `cargo check --all-targets` over every package it touched, and **a tree that does not compile is a
   failed run**, never "applied N of N": the edits are left on disk for inspection and the message says
   how to roll them back. A fresh apply first checks the packages the plan names, and refuses to write
   anything into a tree that already does not compile. An apply also **consumes its plan file**: it
   writes each operation's anchors back for the tree it left, so a plan that has been applied (or that
   failed part-way and was rolled back) is stale — regenerate it from `anchors` rather than re-running it.

## Gathering a topic module

A topic module gathered from items that live in several files, or from a module that sits under the
wrong parent, is moves, not extractions: `extract_module` cannot re-point callers, so it leaves a
facade only a hand edit can remove.

1. **Create and fill in one step.** A `move_item` line that carries `name` makes `to` the **parent**,
   declares a new empty module `name` in it and moves the first items in. Without `name`, `to` is an
   existing module, and a missing one is refused, so a typo cannot grow a file.
2. **Move the rest in** with further `move_item` lines whose `to` is the new module, each anchored in
   its own file (one file per line; the plan may hold lines for several files).
3. **Re-parent whole modules** with `reparent_module`: the module's file and directory move under the
   new parent, with its `mod` declaration, and the paths that named it follow.
4. **Choose `reexport` by who calls.** `none` re-points every caller; `outside` re-points the callers
   inside the library crate and leaves a facade only for what another package, a `tests/`, `examples/`
   or `benches/` file, or a `src/bin` of the same package reaches (use it for a crate with consumers);
   `glob` and `named` leave a facade and re-point nothing (use them to keep every old path).
5. **`check --deep` is still the gate**, for these operations as for every other: it resolves each
   move through the same path `apply` uses and writes nothing. A refusal there is a defect in the plan
   or in the engine, and a hand edit after an apply is neither: record it as a backlog entry or fix the
   engine. The one thing a move does not do is widen a private field or method of a type it splits,
   or the private items of a re-parented tree; the compile gate names those.

## Testing cadence

**Build and test once, after the whole plan is finished — not between plans, not between operations.**
A restructure is a sequence of behaviour-preserving moves, and what proves a move correct is already
inside the loop; a full test run per move adds minutes (every acceptance suite that boots its own
rust-analyzer, tens of seconds each) and tells you nothing the loop did not.

Between plans, and after each one, the evidence is only what the tool produces:

- `restructure apply` — its own `cargo check --all-targets` before and after (the compile gate), the
  tidy of imports the compiler reports unused, and `rustfmt` over every file it wrote. That gate is part
  of the apply: do not skip it, and do not add a build, `cargo clippy`, `cargo fmt` or `./test` of your
  own beside it.
- `restructure verify --against HEAD` — every statement accounted for. No cargo.
- Commit that plan on its own, so a failure found later bisects to one move.

After the **last** plan of the changeset, one gate, in this order:

1. `cargo fmt --check` and `cargo clippy -p <every touched package> --all-targets -- -D warnings`;
2. `./test -p <crate>` (every target runs, however many fail) — the **failing set must equal the
   baseline's by name**; a test that was red before must still fail for the same reason;
3. the comment-line multiset across the touched sources before and after (a lost `//` comment is the one
   defect no compiler and no test sees), and `restructure verify --against <ref before the first plan>`.

A failure here is fixed forward in a new commit; the per-plan commits are how you find which move did it.
A refusal or compile failure **inside** an apply is different: it is the tool telling you, so stop, roll
back what the run touched, and fix the plan (or the tool) before the next plan — never carry a broken
tree forward to be found by the final gate.

**Prove before you pay.** Against a warm index a `--deep` check costs seconds and an apply costs
seconds; against a cold one an apply costs six to ten minutes before it can refuse. Every refusal
`--deep` reports is one an apply would have reported after paying for the index.

## Rules

- **No code in plans** — fields `text`, `code`, `content` are refused.
- **No `create_file`** — a file appears only because an operation caused it: an assist, `to_file`,
  `extract_module_to_file`, a cross-crate move, or a `move_item` that carries `name`.
- **Unsupported ops are hard errors** — never skip silently.
- **Order `extract_method`s bottom-up.** Several in one function compose only last-range-first, so
  no anchor has to be translated through another extraction; see `references/plan-schema.md`.
- **An `extract_method` range holds no early `return`.** A `return` that exits the function around
  the range is refused (`check`, `check --deep` and `apply` alike): the assist would copy it into the
  new function, which returns from itself instead. A `return` inside a closure, `async` block or nested
  `fn` in the range is fine. End the range before the first early exit.
- **Read the refusal's class before its text.** `plan is malformed:` means edit the plan. `this seam
  cannot be cut here:` means the plan is fine and the code will not permit this cut — move the seam or
  change the code. `rust-analyzer's answer was unusable:` means retry against a warm server — except when it says
  the index is **degraded** (rust-analyzer's own health is `warning` or `error`, typically "Failed to
  run build scripts"): then no retry helps, and the message quotes what to fix. `the tree does not
  compile before the plan runs:` means repair the tree first; nothing was written. `… were applied, and
  the tree no longer compiles:` means the edits are on disk and the compiler rejects them. Every
  refusal already ends with its own remedy.
- **Waiting** — a run waits until the server is ready or until you stop it; there is no
  `--indexing-budget` any more (it derived a per-operation ceiling of a twentieth of itself, which
  refused large files at 45s). `^C` cancels, and the refusal says how far the index got.
- **A warm index** — a cold start is minutes per run, so for an iterative carve start the daemon once
  and point the CLI at it:

  ```bash
  eval $(./run-index-daemon | grep '^export ')   # exports TDDY_INDEX_SOCKET
  tddy-tools restructure check plan.jsonl        # now costs the assist, not the index
  ```

  `./run-index-daemon` **warms its checkout by default**: after the daemon answers it runs
  `tddy-tools restructure warm` for the checkout root and returns when the crate graph is queryable
  (more than five minutes on a cold start of this workspace), so the first request does not pay for it.
  `--no-warm` (first argument) skips that. A warm that fails is reported on stderr and changes
  nothing else. `tddy-tools restructure warm` does the same on its own, for a daemon started another way, and
  is refused naming `./run-index-daemon` when `TDDY_INDEX_SOCKET` names none.

  With `TDDY_INDEX_SOCKET` unset the CLI spawns its own rust-analyzer exactly as before. A set but
  unreachable socket is an error, not a silent fall back to the cold path.

  **A daemon that died:** read its log *before* restarting, or after — a start no longer destroys it.
  The live log (`tddy-index-<tag>.log`, path printed on start) is the current run's alone, because
  readiness is read from it; the script first appends the previous run's log to
  `tddy-index-<tag>.history.log` (one header per run: its pid and when its log was last written; capped
  at 10 MiB, then kept as `.history.log.1`). Both sit in `TDDY_INDEX_RUNTIME_DIR`.

## References

- [`references/plan-schema.md`](references/plan-schema.md)
- [`references/restructure-changeset.md`](references/restructure-changeset.md)
- [`tddy-code-restructuring` README](../../packages/tddy-code-restructuring/README.md)
