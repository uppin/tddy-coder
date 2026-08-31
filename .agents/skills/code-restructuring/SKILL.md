---
name: code-restructuring
description: Restructure Rust code without writing moved code by hand — split modules, extract methods, rename symbols. Plans intents in a Refactor changeset, then executes a JSONL plan via tddy-tools restructure driving rust-analyzer through tddy-lsp.
---

# Code Restructuring (Rust)

**You never write the moved or extracted code.** You write a plan of *intents*. `tddy-tools restructure` resolves each intent through rust-analyzer (via `tddy-lsp`).

**v1 scope:** Rust only — seven operations, five subcommands. No TypeScript.

## CLI

```bash
tddy-tools restructure apply  <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N] [--indexing-budget SECONDS]
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check  <plan.jsonl> [--deep] [--indexing-budget SECONDS]
tddy-tools restructure anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]
tddy-tools restructure verify --against <git-ref>
```

## Workflow (abbreviated)

1. **Green baseline** — `./test -p <crate>`; record counts.
2. **Targeting** — run [`analyze-code-issues`](analyze-code-issues/SKILL.md); put CRAP note in changeset.
3. **Understand shape** — LSP outline, references, cohesion; write `docs/dev/1-WIP/{slug}-initial-discovery.md`.
4. **Changeset** — `Type: Refactor` at `docs/dev/1-WIP/YYYY-MM-DD-<name>.md`; see `references/restructure-changeset.md`.
5. **Snapshot** — `sha256:` hashes in plan header line 1.
6. **Plan** — JSONL intents only; see `references/plan-schema.md`.
7. **Prove seams** — `tddy-tools restructure check plan.jsonl [--deep]` before apply.
8. **Apply** — `--dry-run` then apply; `verify --against HEAD` after.

## Rules

- **No code in plans** — fields `text`, `code`, `content` are refused.
- **No `create_file`** — files appear via assists only.
- **Unsupported ops are hard errors** — never skip silently.
- **Indexing budget** — raise `--indexing-budget` on slow machines; indexing timeout is not a plan defect.

## References

- [`references/plan-schema.md`](references/plan-schema.md)
- [`references/restructure-changeset.md`](references/restructure-changeset.md)
- [`tddy-code-restructuring` README](../../packages/tddy-code-restructuring/README.md)
