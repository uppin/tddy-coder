# Rust Code Restructuring (plan-driven refactors)

**Product area:** Coder / tddy-tools  
**Status:** Active  
**Updated:** 2026-08-31

## Summary

`tddy-tools restructure` replays a JSONL **plan of named intents** (never source text) against rust-analyzer through `tddy-lsp`. The library crate is `tddy-code-restructuring`; there is no separate binary.

**v1 scope:** Rust only — seven operations, five subcommands. No TypeScript sidecar. Agents use [`.agents/skills/code-restructuring`](../../../.agents/skills/code-restructuring/SKILL.md) after [analyze-code-issues](rust-code-analysis.md).

A green baseline is required; a red tree is a stop.

## CLI

```text
tddy-tools restructure apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]
                                       [--indexing-budget SECONDS]
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check <plan.jsonl> [--deep] [--indexing-budget SECONDS]
tddy-tools restructure anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]
tddy-tools restructure verify --against <git-ref>
```

| Subcommand | Role |
|---|---|
| `apply` | Execute the plan; `--dry-run` rehearses in an overlay; `--resume` continues from the journal |
| `status` | completed / in_flight / pending / failed |
| `check` | All findings, no writes; `--deep` resolves through the same path as apply |
| `anchors` | Emit a correct range covering named items (including trivia) |
| `verify` | Statement-multiset comparison against a git ref |

## Plan format

Line 1 is a snapshot header: `{ "v": 1, "snapshot": { … } }` with `sha256:` content hashes.

Subsequent lines are one `RefactorOp` each. Plans must not contain `text` / `code` / `content`, `create_file`, or `insert_text` — the parser refuses them. Unsupported operations are hard errors, not skips. Files appear because an operation caused them (`to_file` / `extract_module_to_file`), never because a plan declared them.

See [`.agents/skills/code-restructuring/references/plan-schema.md`](../../../.agents/skills/code-restructuring/references/plan-schema.md).

## Rust operations (v1)

| Operation | Notes |
|---|---|
| `extract_method` | Range → new function |
| `extract_variable` | Subexpression → binding |
| `rename_symbol` | LSP rename |
| `extract_module` | `reexport`: glob / named / none; optional `to_file` |
| `extract_module_to_file` | Move items to new file |
| `extract_trait` | Extract trait from impl |
| `inline_method` | Inline callee |

Invariants: moves that need history use `git mv`; visibility widenings are reviewable output (journal/stdout), not silent; progress goes to an injected sink, never mixed into library stdout.

## LSP integration

Restructuring uses the existing long-running rust-analyzer task (`LspRegistry::rust_only()`). The restructure crate does **not** spawn a private rust-analyzer.

`LspClientBridge` wraps `Arc<LspClient>` and exposes sync `request` / `notify` via `request_raw` / `notify_raw` on `tddy-lsp`. Typed assist APIs (`codeAction`, `rename`, `semanticTokens`, progress forwarding) remain a follow-up; v1 uses the raw RPC bridge.

`--indexing-budget SECONDS` raises rust-analyzer indexing time on loaded machines; an indexing timeout is not a plan defect.

## Workflow

1. Green baseline — `./test -p <crate>`.
2. Run [analyze-code-issues](rust-code-analysis.md); record CRAP targeting in the changeset.
3. Author intents in JSONL; `restructure check` (optionally `--deep`) before apply.
4. `apply --dry-run`, then apply; `verify --against HEAD` after successful extract operations.

## Related documentation

- [Reusable LSP](reusable-lsp.md) — client reuse; raw RPC surface for restructuring
- [Rust code analysis](rust-code-analysis.md) — prerequisite targeting pass
- [Feature prompt: agent skills](feature-prompt-agent-skills.md)
- Package: [`packages/tddy-code-restructuring/README.md`](../../../packages/tddy-code-restructuring/README.md)

## Known limitations

- rust-analyzer indexing can exceed the default budget; use `--indexing-budget`.
- Restructuring tests that start rust-analyzer are load-sensitive; run affected suites with `--test-threads=1` when binding a server.
- Typed `tddy-lsp` assist methods are not yet first-class; restructuring uses `request_raw` / `notify_raw`.
