---
name: code-restructuring
description: Restructure Rust code without writing moved code by hand — split modules, extract methods, rename symbols. Plans intents in a Refactor changeset, then executes a JSONL plan via tddy-tools restructure driving rust-analyzer through tddy-lsp.
---

# Code Restructuring (Rust)

**You never write the moved or extracted code.** You write a plan of *intents*. `tddy-tools restructure` resolves each intent through rust-analyzer (via `tddy-lsp`).

**v1 scope:** Rust only — seven operations, five subcommands. No TypeScript.

## CLI

```bash
tddy-tools restructure apply  <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check  <plan.jsonl> [--deep] [--budget LINES]
tddy-tools restructure snapshot <plan.jsonl>
tddy-tools restructure anchors <file.rs> --items A,B,C
tddy-tools restructure verify --against <git-ref>
```

## Workflow (abbreviated)

1. **Green baseline** — `./test -p <crate>`; record counts.
2. **Targeting** — run [`analyze-code-issues`](analyze-code-issues/SKILL.md); put CRAP note in changeset.
3. **Understand shape** — LSP outline, references, cohesion; write `docs/dev/1-WIP/{slug}-initial-discovery.md`.
4. **Changeset** — `Type: Refactor` at `docs/dev/1-WIP/YYYY-MM-DD-<name>.md`; see `references/restructure-changeset.md`.
5. **Anchor** — `restructure anchors <file.rs> --items A,B,C`. **Do not hand-write line numbers.**
   The command emits a range that covers whole items including their trivia; a hand-counted one
   routinely clips a helper the moved code needs, and the assist then relocates part of the range and
   rewrites the rest in place.
6. **Snapshot** — `restructure snapshot plan.jsonl` writes the `sha256:` header from the working tree.
   Re-run it after **every** edit to a snapshotted file, or the next command refuses on drift.
7. **Plan** — JSONL intents only; see `references/plan-schema.md`.
8. **Prove seams** — `restructure check plan.jsonl --deep`. **`--deep` is the gate, not an option.**
   A plain `check` reads text; only `--deep` resolves each operation through the same path `apply`
   uses, so it is the only form that reports an assist or import refusal — and it writes nothing. A
   plain `check` returning `no findings` says nothing about whether the apply will run.
9. **Apply** — `--dry-run` first, then apply; `verify --against HEAD` after.

**Prove before you pay.** Against a warm index a `--deep` check costs seconds and an apply costs
seconds; against a cold one an apply costs six to ten minutes before it can refuse. Every refusal
`--deep` reports is one an apply would have reported after paying for the index.

## Rules

- **No code in plans** — fields `text`, `code`, `content` are refused.
- **No `create_file`** — files appear via assists only.
- **Unsupported ops are hard errors** — never skip silently.
- **Order `extract_method`s bottom-up.** Several in one function compose only last-range-first, so
  no anchor has to be translated through another extraction; see `references/plan-schema.md`.
- **Read the refusal's class before its text.** `plan is malformed:` means edit the plan. `this seam
  cannot be cut here:` means the plan is fine and the code will not permit this cut — move the seam or
  change the code. `rust-analyzer's answer was unusable:` means retry against a warm server. Every
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

  With `TDDY_INDEX_SOCKET` unset the CLI spawns its own rust-analyzer exactly as before. A set but
  unreachable socket is an error, not a silent fall back to the cold path.

## References

- [`references/plan-schema.md`](references/plan-schema.md)
- [`references/restructure-changeset.md`](references/restructure-changeset.md)
- [`tddy-code-restructuring` README](../../packages/tddy-code-restructuring/README.md)
