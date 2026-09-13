# Rust Code Restructuring (plan-driven refactors)

**Product area:** Coder / tddy-tools  
**Status:** Active  
**Updated:** 2026-09-09

## Summary

`tddy-tools restructure` replays a JSONL **plan of named intents** (never source text) against rust-analyzer through `tddy-lsp`. The library crate is `tddy-code-restructuring`; there is no separate binary.

**v1 scope:** Rust only — eight operations, five subcommands. No TypeScript sidecar. Agents use [`.agents/skills/code-restructuring`](../../../.agents/skills/code-restructuring/SKILL.md) after [analyze-code-issues](rust-code-analysis.md).

A green baseline is required; a red tree is a stop.

## CLI

```text
tddy-tools restructure apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]
                                       [--indexing-budget SECONDS]
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check <plan.jsonl> [--deep] [--budget LINES] [--indexing-budget SECONDS]
tddy-tools restructure anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]
tddy-tools restructure verify --against <git-ref>
```

| Subcommand | Role |
|---|---|
| `apply` | Execute the plan; `--dry-run` rehearses in an overlay; `--resume` continues from the journal |
| `status` | completed / in_flight / pending / failed |
| `check` | All findings, no writes; `--deep` resolves through the same path as apply; `--budget LINES` additionally reports which of the files the plan's **anchors** name exceed that many lines — a report, never a gate |
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
| `rename_symbol` | LSP rename, applied to **every** document rust-analyzer returns edits for, not only the anchor's own file |
| `extract_module` | `reexport`: glob / named / none; optional `to_file` |
| `extract_module_to_file` | Move items to new file |
| `extract_trait` | Extract trait from impl |
| `inline_method` | Inline callee |
| `move_module_to_crate` | Move `<crate>/src/<module>.rs` into another crate: `git mv` the file, rewrite its own `use crate::…` / `use super::…` header, re-point every caller found by `textDocument/references`, and edit both `Cargo.toml`s. `to` is the destination crate's directory and is required. `reexport: "glob"` leaves `pub use <dest_crate>::*;` in the origin, which gives a **zero caller diff**; `"named"` is refused, because a named re-export puts items at the destination's crate root while a caller writes `crate::<module>::Item` |

Invariants: moves that need history use `git mv`; visibility widenings are reviewable output (journal/stdout), not silent; progress goes to an injected sink, never mixed into library stdout.

## LSP integration

Restructuring uses the existing long-running rust-analyzer task (`LspRegistry::rust_only()`). The restructure crate does **not** spawn a private rust-analyzer.

`LspClientBridge` wraps `Arc<LspClient>` and exposes sync `request` / `notify` via `request_raw` / `notify_raw` on `tddy-lsp`. Typed assist APIs (`codeAction`, `rename`, `semanticTokens`, progress forwarding) remain a follow-up; v1 uses the raw RPC bridge.

### The handshake decides what the server will answer

A language server tailors its replies to what the client advertised, so both transports — the
backend's own child process and the bridged shared client — negotiate the **same** handshake. The
allow-list entry carries it: `LaunchSpec::with_capabilities` and `with_initialization_options`, which
`tddy-tools` fills from the restructure backend's `client_capabilities()` and `server_settings()`.

Two parts of that handshake are load-bearing:

- **`codeAction` support.** rust-analyzer returns no code actions whatsoever to a client that
  advertised none, which is indistinguishable from a range that supports no refactoring.
- **`positionEncodings: ["utf-8"]`.** The client counts bytes. A server left on the LSP default of
  utf-16 agrees on every line until one carries a character outside ASCII, and then every column is
  wrong. A handshake that settles on any other encoding is **refused** rather than resolved against.

### Budgets

`--indexing-budget SECONDS` governs every wait the run makes: the one-time crate-graph warm-up, the
per-request timeout on the shared client, and the per-operation settle budget once the first index has
succeeded (a twentieth of the run budget, floored at 30s). An indexing timeout is not a plan defect,
and the message says so along with **how far the index got** — the last phase plus the furthest
percentage reported, because a stall at 12% and a timeout at 99% want opposite responses.

### Import restoration

An extraction moves items out of the scope of their file's `use` declarations, so the backend restores
what the moved code lost. It asks rust-analyzer for an import at each unresolved name and, where that
cannot answer, reads the parent's own `use` tree:

| Case | How it resolves |
|---|---|
| One offered path | applied |
| Several offered paths | settled by an exact binding the file already has; failing that, by the one candidate whose **module** the file already imports from; otherwise **refused**, naming the candidates |
| A name the parent binds under an alias (`ProbeOutcome as ProtoProbeOutcome`) | reconstructed from the parent's declaration — rust-analyzer offers the unaliased path, which binds nothing |
| A **module** binding (`use crate::tool_engine;`) | reconstructed the same way; `Import` is offered for items, never for a bare module path |
| A name the seam's own facade will re-export | left to the facade — a named import here would be private and would shadow it |

An import that does not reduce its name's unresolved occurrences is refused rather than written,
because a `use` that resolves nothing is how a successful run lands source that will not build. The
same principle guards the rewrite itself: every `module::Ident` the assist writes must name something
the seam actually moved.

A facade is emitted at the widest visibility the relocated items carry — `pub use` only where
something moved is `pub`, `pub(crate)` otherwise, since the assist rewrites what it relocates to
`pub(crate)` and a `pub` glob over none of it re-exports nothing.

## Workflow

1. Green baseline — `./test -p <crate>`.
2. Run [analyze-code-issues](rust-code-analysis.md); record CRAP targeting in the changeset.
3. Author intents in JSONL; `restructure check` (optionally `--deep`) before apply.
4. `apply --dry-run`, then apply; `verify --against HEAD` after successful extract operations.
5. `cargo fmt --all`, then the baseline suite. Relocated bodies sit at a new indentation, and a body
   correctly wrapped at one indentation is not correctly wrapped at another — at any scale beyond a
   few items this is every run, and `cargo fmt --all --check` is the first thing CI's lint step does.

## Related documentation

- [Reusable LSP](reusable-lsp.md) — client reuse; raw RPC surface for restructuring
- [Rust code analysis](rust-code-analysis.md) — prerequisite targeting pass
- [Feature prompt: agent skills](feature-prompt-agent-skills.md)
- Package: [`packages/tddy-code-restructuring/README.md`](../../../packages/tddy-code-restructuring/README.md)

## Known limitations

- **`extract_method` is the operation most sensitive to index readiness.** It needs type inference,
  where `extract_module` needs only the syntax tree — so on a large file in a large workspace the
  module operations succeed while an extraction waits. An expired budget distinguishes the two causes:
  a range that does not support the assist, versus a server that cannot yet type it.
- **A whole method body is not extractable.** rust-analyzer declines to wrap a complete body in a
  function that adds nothing; the range has to be a proper subset, so leave the first statement behind.
- **An `impl` block moves whole or not at all.** `extract_module` is the only splitting operation and
  an `impl` body cannot hold a `mod`, so cutting one block into several is a hand edit — insert the
  `}` / `impl Type {` pair, then move each block, which is free of caller churn.
- **A facade re-exports items, not imports.** A glob cannot re-export a name the module merely
  imports, so a child module reaching names through `use super::*` loses any name that was a parent
  *import* consumed by moved code. Bind those in the child, or under `#[cfg(test)]` in the parent when
  only its test modules need them.
- **A cross-crate move is planned one op at a time, against the pre-move tree.** Each
  `move_module_to_crate` op is evaluated as if none of its siblings had run, so a multi-op plan is
  never seen as a whole: a plan that moves `host_registry` and `multi_host` to the same destination
  is refused on the first, because `host_registry` still names `tddy_daemon::multi_host::…`. Layer
  the plan by dependency depth and run it once per layer — and budget for a full rust-analyzer index
  per layer.
- **A cyclic module group cannot be moved at any layering.** The operation moves one module per op,
  so a mutually-dependent pair (`host_tooling ⇄ ssh_agent`) is unreachable: each op sees the other
  module still in the origin crate. Cut the cycle by hand first, or move the group by hand.
- **Only `<crate>/src/<module>.rs` moves.** A nested module and a crate root are **refused, not
  guessed** — a nested module's `mod` line lives in a file the operation would have to guess at.
- **Registry dependencies are not carried, only path ones.** The destination manifest gains the
  `path` dependencies the moved file needs and nothing else; a moved file that uses `chrono` or
  `futures-util` leaves the destination short of it, and the build says so.
- **The moved file's `use` header is re-pointed; its function bodies are not.** A `crate::` qualifier
  at the head of a `use` declaration changed meaning by definition when the file changed crates, and
  that is what the mechanical pass can prove. A `crate::` path inside a body is left alone, and the
  build after the move is what surfaces it.
- **A caller that imports the module rather than the item is not re-pointed.** The survey asks
  rust-analyzer for references per *item*, so a caller written `use crate::host_registry;` and then
  `host_registry::X` is outside the reference set. Covering it needs a second engine call on the
  `mod` declaration.
- **A rewritten caller path keeps the module segment**: `crate::host_registry::HostRegistry` becomes
  `tddy_host_service::host_registry::HostRegistry`, never `tddy_host_service::HostRegistry`. That is
  what makes the glob facade free — it re-exports the module, so the origin's own paths keep
  resolving.
- **A move that would make the workspace cyclic is refused up front**, on both the facade and the
  no-facade path: a facade makes the origin depend on the destination, and a re-pointed caller does
  the same, so if the moved code still names the origin, cargo would reject the pair with an error
  naming neither the module nor the operation. The refusal names every path that forced it.
- Restructuring tests that start rust-analyzer are load-sensitive; run affected suites with `--test-threads=1` when binding a server.
- Typed `tddy-lsp` assist methods are not yet first-class; restructuring uses `request_raw` / `notify_raw`.
