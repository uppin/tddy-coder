# Rust Code Restructuring (plan-driven refactors)

**Product area:** Coder / tddy-tools  
**Status:** Active  
**Updated:** 2026-09-19

## Summary

`tddy-tools restructure` replays a JSONL **plan of named intents** (never source text) against rust-analyzer through `tddy-lsp`. The library crate is `tddy-code-restructuring`; there is no separate binary.

**v1 scope:** Rust only — ten operations, five subcommands. No TypeScript sidecar. Agents use [`.agents/skills/code-restructuring`](../../../.agents/skills/code-restructuring/SKILL.md) after [analyze-code-issues](rust-code-analysis.md).

A green baseline is required; a red tree is a stop.

## Entry points

Two front ends over one engine.

| Front end | Shape |
|---|---|
| `tddy-tools restructure …` | one operation per process, printing to a console |
| `tddy-index-daemon` | a warm rust-analyzer index per workspace root, served as `code_index.CodeIndexService` over gRPC and stdio — or one operation in process, then exit |

Every library entry point takes the **workspace root** it acts on; none reads the process directory.
That is what lets one process serve several worktrees.

**One caveat on serving several at once.** The host is cheap per root — a backend registry and a
document-version map — but the rust-analyzer behind each is not, and two resident on one machine
compete for CPU. Measured on a three-crate workspace: a second request on *one* warm root is ~2,500×
faster than its cold load, but re-asking a root after a *different* root's graph loaded ranged from
880 ms to 3.6 s, the upper end exceeding that root's own 2.19 s cold load. Warm several worktrees at
once and the per-root benefit narrows sharply; warm one and it is total.

See [warm code-intelligence daemon](warm-code-intelligence-daemon.md).

## CLI

```text
tddy-tools restructure apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check <plan.jsonl> [--deep] [--budget LINES]
tddy-tools restructure snapshot <plan.jsonl>
tddy-tools restructure anchors <file.rs> --items A,B,C
tddy-tools restructure verify --against <git-ref>
```

**`--indexing-budget` was withdrawn.** A run now waits until it succeeds or its caller stops it, so
there is no budget to state. See [Waiting](#waiting).

| Subcommand | Role |
|---|---|
| `apply` | Execute the plan; `--dry-run` rehearses in an overlay; `--resume` continues from the journal |
| | Run state is **keyed by the plan** — `<root>/.restructure/<plan stem>-<digest>/` — so a completed plan does not block the next one under the same root, and `--resume` resumes the plan it was given rather than whichever ran last. Every multi-layer restructuring is several plans in one repository, which is why this is not an implementation detail. A journal left at `<root>/.restructure/` by an older run is adopted when resuming, and otherwise refused by name; it is never silently taken over by a different plan |
| `status` | completed / in_flight / pending / failed |
| `check` | All findings, no writes; `--deep` resolves through the same path as apply; `--budget LINES` additionally reports which of the files the plan names — every member of a cluster, not only its anchor — exceed that many lines — a report, never a gate |
| `snapshot` | Rewrite the plan's line-1 `sha256:` header from the working tree, leaving every operation line byte-identical. No index, no language server |
| `anchors` | Emit a correct range covering named items (including trivia) |

**A plain `check` is not a rehearsal.** It reads text. `--deep` resolves every operation through the
same path `apply` takes and writes nothing, so it is the only form that reports an assist or import
refusal before an index has been paid for. `no findings` from a plain `check` has been followed by an
apply that refused more than once — see
[§ Known limitations](#known-limitations).
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
| `move_cluster_to_crate` | Move a **set** of modules into another crate as one unit. `anchor` is the first member and `also` names the rest; `to` and `reexport` behave as above. The whole set moves or none of it does, in a single edit, so the tree is never half-moved. A path reaching a **co-moving** member stays `crate::` — the destination *is* `crate` once the file has arrived — while a path reaching a module staying behind is re-pointed at the origin. This is what makes a mutually-referencing group movable; a set of one is refused, because that is `move_module_to_crate` |
| `move_test_binary_to_crate` | Move `<crate>/tests/<name>.rs` into the crate it exercises: `git mv` the file, re-point **every** path in it that opens with the origin's extern name, and extend the destination's `[dev-dependencies]`. `to` is required; `reexport` is **refused**, because nothing can reference a test binary. There is no origin edit at all — cargo auto-discovers `tests/*.rs`, so the crate the test left never named it. Each path is resolved to the crate that **defines** what it reaches, through however many re-export facades stand in the way |

Invariants: moves that need history use `git mv`; visibility widenings are reviewable output
(journal plus the caller's sink), not silent; **nothing in the library writes to stdout** — progress
and the per-operation account go to injected sinks, and results are returned as values. Only
`restructure_cli.rs` prints, and a test enforces that by reading the crate's own sources: a server
serving this engine over its own stdin/stdout would otherwise have every RPC frame after the first
log line corrupted.

## LSP integration

Restructuring reaches rust-analyzer through `tddy-lsp`'s registry, keyed by `(workspace root,
Rust)`, so a host that outlives one request reuses one server per root.

The allow-list it uses is **not** `LspAllowList::rust_only()`. That constructor advertises no client
capabilities at all, and rust-analyzer returns no code actions and sends no `$/progress` to such a
client — so `restructure_allow_list()` attaches `client_capabilities()` and `server_settings()`
instead. The handshake is fixed at spawn, which is why a consumer needing assists cannot share a
registry entry with one that only needs definitions.

`RustBackend` also retains a **self-spawning** transport, used when no shared client is bridged in —
the static-check path and the standalone case. That path is the only one that pins the toolchain
(`CARGO_HOME`, `RUSTUP_HOME`, `RUSTUP_TOOLCHAIN`), which is what prevents a rustup proxy
channel-sync from stalling at `discovering sysroot`; a registry-spawned server sets no env, so a host
supplying its own allow-list must carry the pinning itself.

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

### Waiting

**A wait ends when the server is ready or when its caller stops waiting. Nothing else bounds it.**

There were budgets, and how they were wrong is worth recording. `--indexing-budget SECONDS` set a
one-time warm-up bound and derived the per-operation bound from it as a twentieth, floored at 30s. So
`--indexing-budget 900` produced a **45-second** ceiling on every wait after the first, and a plan
against a workspace this size was refused with *"had not finished indexing after 46s"* having already
indexed for twenty minutes. The premise behind the ratio — that once the graph is loaded the only
thing left to wait out is seconds — is false for a large file in a large workspace, which the code's
own comment said before the flag was removed.

A server should not invent a deadline its caller never stated, so the caller's own bound is the only
one: a gRPC caller's `grpc-timeout`, a streaming caller dropping its receiver, `^C` on the command
line, or the host cancelling on shutdown. Cancellation is checked **inside** the poll loops rather
than awaited, because the engine is synchronous and runs under `spawn_blocking`, where dropping the
calling future stops nothing.

A cancelled wait still reports **how far the index got** — the last phase plus the furthest
percentage — because a stall at 12% and a stop at 99% want opposite responses. A server that stays
unable to answer one method is `ServerNotSettled`, kept distinct from a malformed plan: the first is
fixed by waiting or by looking at the server, the second by editing the plan, and reporting the
second as the first sends the reader to the wrong place.

**Ready means quiescent and healthy.** rust-analyzer answers hover while it is still running build
scripts, before any `OUT_DIR` type exists for it, so a first answer is not readiness: an extraction
asked then writes `req: _`. The waits keep polling while the server's last `experimental/serverStatus`
said it was not quiescent; a server that never sends the status gets through on its first answer.
Once ready, an index whose reported `health` is anything but `ok` — **`warning` included**, because
that is how a failed build script is reported — is refused, quoting the server's own message. An
index in that state answers without the code it could not build, so nothing it says can be trusted.
The latest status is kept on the shared client, so the gate holds against a warm daemon whose status
transition another request already read.

### Refusal classes

Every refusal is fatal — the executor never falls back — and its **class** is what says who has to do
something about it. Read the class before the text.

| Reads | Class | What to do | Over the wire |
|---|---|---|---|
| `plan is malformed: …` | The plan says something this executor will not do | Edit the plan | `InvalidArgument` |
| `this seam cannot be cut here: …` | The plan is well formed and the code will not permit this cut — stranded references, an `impl` cut in half, a module name already taken, an import the file's own bindings cannot disambiguate | Move the seam, or change the code | `FailedPrecondition` |
| `rust-analyzer's answer was unusable: …` | The server answered and the answer cannot be used — an extraction produced before inference, a mangled rewrite, a response with no edits | Retry against a warm server, or look at the server | `Internal` |
| `rust-analyzer reports its index as degraded …` | The server said its own index is incomplete — usually build scripts or proc macros that failed to link in the environment it was started in | Restart the server (or `./run-index-daemon --stop && ./run-index-daemon`) from the dev shell's whole environment | `Internal` |
| `rust-analyzer would not settle …` | The wait ended before the index did | Wait, or look at the server | `DeadlineExceeded` |
| snapshot / journal / anchor mismatches | The tree is not in the state the plan was written against | Repair the tree, or re-snapshot | `FailedPrecondition` |
| `the tree does not compile before the plan runs …` | `apply`'s baseline `cargo check` failed; nothing was written | Make the tree compile, then apply again | `FailedPrecondition` |
| `N of M operation(s) were applied, and the tree no longer compiles …` | Every operation was accepted and the compiler rejects the result. The edits are left on disk and in the journal | Fix the compiler-named errors by hand, or roll back as the message says (restore the touched paths from git, remove the journal) | `Internal` |

The distinction is not cosmetic. These were one class until a live extraction was refused twice with
`plan is malformed` over a plan that was correct both times, sending its author to edit the one thing
that was not wrong. Each message already ends with its own remedy; the class is what tells a reader
whether that remedy is theirs to apply.

### Import restoration

An extraction moves items out of the scope of their file's `use` declarations, so the backend restores
what the moved code lost. It asks rust-analyzer for an import at each unresolved name and, where that
cannot answer, reads the parent's own `use` tree:

| Case | How it resolves |
|---|---|
| One offered path | applied |
| Several offered paths | settled by an exact binding the file already has; failing that, by the one candidate whose **module** the file already imports from; failing that, by the one whose **crate** the file binds that same name from; otherwise **refused**, naming the candidates |
| A re-exported item (`tddy_core::ParseError` offered, `tddy_core::error::ParseError` imported) | settled by the crate tier — one item under two paths is not two candidates. Keyed on a binding of the contested name, never on any binding from that crate, or every file importing anything from `std` would match `std::string::ParseError` |
| A name the parent binds under an alias (`ProbeOutcome as ProtoProbeOutcome`) | reconstructed from the parent's declaration — rust-analyzer offers the unaliased path, which binds nothing |
| A **module** binding (`use crate::tool_engine;`) | reconstructed the same way; `Import` is offered for items, never for a bare module path |
| A name the seam's own facade will re-export | left to the facade — a named import here would be private and would shadow it |
| A relative path in a reconstructed declaration (`use super::Failure as HostFailure;`) | rebased for the new module, which is the parent's child: `super::X` → `super::super::X`, `self::X` → `super::X`. `crate::`, `::` and extern-crate paths are unchanged |
| A name the parent bound in a group the assist emptied (`use tokio::sync::{…, mpsc, …};`) | the choice reads the file as it was before the assist together with the current text, so the binding the assist removed still decides |

Only the names **the seam lost** are weighed: every unresolved occurrence inside the new module, and
one in the parent only for a name the file resolved everywhere before the cut. A name the server
could not resolve anywhere to begin with is not something the cut did, and is left alone.

An import — offered or reconstructed — that does not reduce its name's unresolved occurrences is refused by name rather than written,
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
   `apply` runs `cargo check --all-targets` over the touched packages before and after, and fails the
   run when the result does not compile.
5. `cargo fmt --all`, then the baseline suite. Relocated bodies sit at a new indentation, and a body
   correctly wrapped at one indentation is not correctly wrapped at another — at any scale beyond a
   few items this is every run, and `cargo fmt --all --check` is the first thing CI's lint step does.

## Related documentation

- [Reusable LSP](reusable-lsp.md) — client reuse; raw RPC surface for restructuring
- [Rust code analysis](rust-code-analysis.md) — prerequisite targeting pass
- [Warm code-intelligence daemon](warm-code-intelligence-daemon.md) — the
  daemon front end, the workspace-root parameter, cancellation in place of budgets
- [Feature prompt: agent skills](feature-prompt-agent-skills.md)
- Package: [`packages/tddy-code-restructuring/README.md`](../../../packages/tddy-code-restructuring/README.md)

## Known limitations

- **An assist may relocate less than the anchor asked for, and rewrite the remainder in place.**
  rust-analyzer decides the extraction's real extent; when it moves part of the anchored range, it
  rewrites what it left behind to reach the new module through qualified `module::Item` paths. The
  run reports which lines stayed. Author the anchor with `anchors --items` rather than by hand — a
  range that clips a helper is the usual way into this — and read the widening report the run prints.
- **`check` without `--deep` cannot predict every apply refusal.** The plain form reads text and
  runs `move_module_to_crate` preconditions (parent module present, crate layout) before any server
  starts; it still has returned `no findings` on plans that `apply` then refused for reasons only a
  deep resolve sees. `--deep` resolves through the apply's own path and writes nothing, so it is the
  form to gate on.

- **`extract_method` is the operation most sensitive to index readiness.** It needs type inference,
  where `extract_module` needs only the syntax tree — so on a large file in a large workspace the
  module operations succeed while an extraction waits. An expired budget distinguishes the two causes:
  a range that does not support the assist, versus a server that cannot yet type it.
- **An `extract_method` range may not return from the function around it.** rust-analyzer copies a
  `return` verbatim into a function of another return type, so such a range is refused, naming the
  lines, by `check`, `check --deep` and `apply` alike. A `return` inside a closure, an `async` block
  or a nested `fn` does not count. The scan is lexical: a `return` a macro expands to (`bail!`) and a
  `break`/`continue` leaving the range are not seen, and only `apply`'s compile gate catches them.
- **Several `extract_method`s in one function compose only bottom-up**, last range first. The
  engine does not re-anchor a later operation through an earlier one's edit.
- **`check --deep` does not compile.** It resolves every operation and writes nothing, but a clean
  deep check can still be followed by an `apply` the compile gate fails; the gaps behind that are in
  the backlog.
- **A whole method body is not extractable.** rust-analyzer declines to wrap a complete body in a
  function that adds nothing; the range has to be a proper subset, so leave the first statement behind.
- **An `impl` block moves whole or not at all.** `extract_module` is the only splitting operation and
  an `impl` body cannot hold a `mod`, so cutting one block into several is a hand edit — insert the
  `}` / `impl Type {` pair, then move each block, which is free of caller churn. A seam that takes
  some of a type's **inherent** `impl` blocks and leaves others is fine: calls between them resolve
  through the type, and the `modname::` the assist writes into them is undone. A seam through a
  **trait** `impl` that the rest of the file references is refused (E0119/E0046).
- **A facade re-exports items, not imports.** A glob cannot re-export a name the module merely
  imports, so a child module reaching names through `use super::*` loses any name that was a parent
  *import* consumed by moved code. Bind those in the child, or under `#[cfg(test)]` in the parent when
  only its test modules need them.
- **A set that moves together must say so.** `move_module_to_crate` is evaluated as if none of its
  siblings had run, so a multi-op plan moving `host_registry` and `multi_host` to one destination is
  still refused on the first — `host_registry` names `tddy_daemon::multi_host::…`, which would make
  the destination depend on the crate it left. Declare the set with `move_cluster_to_crate` instead
  of layering the plan; a mutually-dependent pair (`host_tooling ⇄ ssh_agent`) is reachable only that
  way, because no layering of one-module ops can order a cycle. `check` names the sibling a partial
  set would strand, statically, before any index is paid.
- **A nested module moves when its parent is locatable.** Anchors at
  `<crate>/src/<parent>/<module>.rs` or `<crate>/src/<parent>/mod.rs` take the destination path from
  the plan's `path` and locate the parent's `mod` line in `<crate>/src/<parent>.rs` then
  `<crate>/src/<parent>/mod.rs`. Refused only when neither parent file exists.
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
  naming neither the module nor the operation. The refusal names every path that forced it. Paths that
  resolve through a back-compat `pub use` facade in the origin are attributed to the **defining**
  crate, not the origin, so a re-export alone does not read as an origin dependency.
- **A test binary's string literals are never rewritten.** A suite reading
  `concat!(env!("CARGO_MANIFEST_DIR"), "/../<crate>/src/<file>.rs")` resolves from its new crate only
  by coincidence, and a path written in a string is a hand edit after the move. This is deliberate:
  what a string holds is data the suite asserts on, and rewriting it would change the assertion.
- **`check` has no static preflight for `move_test_binary_to_crate`.** The plan's vocabulary refusals
  are reported; the anchor's path shape and the facade walk are resolved at apply time.
- Restructuring tests that start rust-analyzer are load-sensitive; run affected suites with `--test-threads=1` when binding a server.
- Typed `tddy-lsp` assist methods are not yet first-class; restructuring uses `request_raw` / `notify_raw`.
