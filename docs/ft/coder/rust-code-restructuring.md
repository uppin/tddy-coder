# Rust Code Restructuring (plan-driven refactors)

**Product area:** Coder / tddy-tools  
**Status:** Active  
**Updated:** 2026-10-03

## Summary

`tddy-tools restructure` replays a JSONL **plan of named intents** (never source text) against rust-analyzer through `tddy-lsp`. The library crate is `tddy-code-restructuring`; there is no separate binary.

**v1 scope:** Rust only — twelve operations, five subcommands. No TypeScript sidecar. Agents use [`.agents/skills/code-restructuring`](../../../.agents/skills/code-restructuring/SKILL.md) after [analyze-code-issues](rust-code-analysis.md).

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
tddy-tools restructure apply <plan.jsonl> [--dry-run] [--resume] [--from N|ID] [--stop-after N]
tddy-tools restructure load <plan.jsonl>...
tddy-tools restructure unload <plan.jsonl>... | --all
tddy-tools restructure plans
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check <plan.jsonl> [--deep] [--budget LINES]
tddy-tools restructure snapshot <plan.jsonl>
tddy-tools restructure anchors <file.rs> --items A,B,C
tddy-tools restructure anchors <file.rs> --at L:C[-L:C]
tddy-tools restructure verify --against <git-ref>
```

**`--indexing-budget` was withdrawn.** A run now waits until it succeeds or its caller stops it, so
there is no budget to state. See [Waiting](#waiting).

| Subcommand | Role |
|---|---|
| `apply` | Execute the plan; `--dry-run` rehearses in an overlay; `--resume` continues from the journal |
| | Run state is **keyed by the plan** — `<root>/.restructure/<plan stem>-<digest>/` — so a completed plan does not block the next one under the same root, and `--resume` resumes the plan it was given rather than whichever ran last. Every multi-layer restructuring is several plans in one repository, which is why this is not an implementation detail. A journal left at `<root>/.restructure/` by an older run is adopted when resuming, and otherwise refused by name; it is never silently taken over by a different plan |
| `load` / `unload` / `plans` | Hold plans in the index daemon's [plan store](#plan-store): `load` reads each plan once and gives every operation an id, `unload` writes changed plans back and drops them (`--all` for every plan of the tree), `plans` lists what is held. All three need the index daemon (`TDDY_INDEX_SOCKET`) and are refused without one |
| `status` | completed / in_flight / pending / failed |
| `check` | All findings, no writes; `--deep` resolves through the same path as apply; `--budget LINES` additionally reports which of the files the plan names — every member of a cluster, not only its anchor — exceed that many **production lines** — lines before the first `#[cfg(test)]` that opens a `mod`, so a file's inline tests are not counted; other languages count every line — a report, never a gate |
| `snapshot` | Rewrite the plan's line-1 header from the working tree. A plan of range and symbol anchors keeps every operation line byte-identical and needs no index. A plan of **item** anchors is also re-resolved against the current tree through rust-analyzer: hints and the fingerprints of unchanged items are rewritten, and each operation whose item changed or is gone is reported and left as written (see [Live plans](#live-plans)) |
| `anchors` | Emit an anchor a plan can carry. `--items A,B` emits an `items` anchor covering the named adjacent items (trivia included); `--at L:C[-L:C]` emits an `item` anchor for the innermost item enclosing that position, with its relative range, fingerprint and hint filled in. See [Item anchors](#item-anchors) |
| | `--items` names are bare (`Alpha`), module-qualified (`krate::module::Alpha`), or `<Type>` / `<Type>#N` for an inherent `impl` block (the Nth when a type has several); a comma inside `<…>` belongs to the type, so `<Pair<A, B>>` is one item. One rule (`item_anchor::parse_item_list`) reads the list for the in-process CLI, the daemon's command line and `tddy-tools` alike |

**A plain `check` is not a rehearsal.** It reads text. `--deep` resolves every operation through the
same path `apply` takes and writes nothing, so it is the only form that reports an assist or import
refusal before an index has been paid for. `no findings` from a plain `check` has been followed by an
apply that refused more than once — see
[§ Known limitations](#known-limitations).
| `verify` | Logical-statement comparison against a git ref; see [Verify](#verify) |

## Plan format

Line 1 is the header. Schema v1 is a snapshot, `{ "v": 1, "snapshot": { … } }` with `sha256:` content
hashes, and a drifted file is refused (`snapshot mismatch`). Schema v2, which a plan using item anchors
is written with, is a per-file hint: `{ "v": 2, "files": { "<path>": { "sha256": …, "modified": … } } }`.
A v2 header never refuses a run; a file whose hash drifted, or which is gone, is reported on the
progress line and the plan runs, because an item anchor does not depend on the rest of the file.
`restructure snapshot` rewrites the header of whichever version the plan has.

Subsequent lines are one `RefactorOp` each; every operation carries an opaque `id` (see [Plan store](#plan-store)) and may carry a `"group"` (see [Transactional groups](#transactional-groups)). A field an operation does not define is refused as malformed, naming it, so a misspelt `group` cannot run its operations ungrouped. Plans must not contain `text` / `code` / `content`, `create_file`, or `insert_text` — the parser refuses them. Unsupported operations are hard errors, not skips. Files appear because an operation caused them (`to_file` / `extract_module_to_file`), never because a plan declared them.

See [`.agents/skills/code-restructuring/references/plan-schema.md`](../../../.agents/skills/code-restructuring/references/plan-schema.md).

## Plan store

A plan is a command log that the executor reads through a **plan store** and writes back as it runs.
The store is a library type with two lifetimes: the index daemon keeps one per workspace root across
requests, and a one-shot `restructure apply` or `check` keeps one for the length of the run.

- **Operation ids.** Every operation has a stable `id` (`"id":"op-3"`). Loading a plan gives one to any
  operation without it, and the next write-back puts it in the file; two operations sharing an id are
  refused as malformed. The journal and the apply events carry the id beside the index, and `--from`
  accepts either. Reordering or inserting operations by hand is safe.
- **Execution by reference.** `Check`, `Apply` and `PlanStatus` run the loaded operations, not whatever
  the file says now. `Apply` of a plan that is not loaded loads it and leaves it loaded; `load` exists to
  load a batch, `unload` to release one. Loading is all or nothing.
- **The applied plan stays current.** After each applied operation the store rewrites the *pending*
  operations of that plan: item-anchor hints and relative ranges are translated through the operation's
  edits, fingerprints of items the operation edited are recomputed, range anchors move with the lines
  inserted above them, and `file` hints follow a file the operation moved. A pending item anchor the edit
  left unresolvable is kept as written, for the next run to refuse. Only the plan being applied is
  refreshed here; every *other* loaded plan is kept current by [Live plans](#live-plans).
- **Write-back.** A changed plan is written (temporary file and rename) within a second of changing, and
  always at the end of a run, on `unload`, and on shutdown of a served daemon (`^C` or `SIGTERM`). After
  an operation the plan is written synchronously, so the file never lags the journal by more than that
  one step. A dry run, and a run refused before its first operation, write nothing.
- **The file stays the human's.** A write-back onto a plan whose file changed since it was loaded is
  refused, naming the plan and saying to unload and reload; the file is untouched. `unload` of such a
  plan drops it without writing, which is how the refusal's remedy works.
- **Resume is verified.** Per operation the run journals `completed`, refreshes the plan in memory,
  journals `plan_synced` (a digest of the pending operations' ids, anchors and `also`), then writes the
  plan. A `--resume` or `--from` recomputes the digest from the plan it holds and compares it with the
  last completed operation's record, so every crash point refuses and none redoes or skips an
  operation. See [Known limitations](#known-limitations).
- **Without a daemon**, `apply` and `check` still work over a store that lives for that one invocation
  and flushes at exit; `load`, `unload` and `plans` are refused as needing the daemon. `--from <id>`
  works only on runs with no daemon.
- **Run state is keyed by the plan** in the daemon as on the command line, so a second plan under a root
  where a first completed through the daemon applies without `--resume`.

How the crate delivers this: [plan-store.md](../../../packages/tddy-code-restructuring/docs/plan-store.md).

## Live plans

A carve is several plans applied in sequence under one root, and applying the first moves the lines,
files and items the others anchor into. Every plan the index daemon holds for a root is therefore kept
current, not only the one being applied.

- **A foreign operation is folded in.** After each applied operation, the store folds that operation's
  edits and renames into every **other** loaded plan of the root. Ranges are translated through the
  edit's line deltas, and `file` hints follow a file the operation renamed or moved. An edit that
  *overlaps* an anchored range is never translated, since it could have removed what the range names:
  the operation is marked stale, `edited by <plan>#<op id>`.
- **A change nobody here made is re-resolved.** When files change underneath the daemon — a checkout,
  a rebase, a hand edit — each loaded plan's item anchors in those files are re-resolved through the
  item path. An intact item rewrites its hint; an item whose text changed is stale as `item changed`;
  one that is gone is stale as `item not found in <file>`. The files the daemon's own runs wrote are
  skipped, because the run that wrote them already carried every held plan through the edit.
- **Per-file hints follow.** The v2 `files` hints (`sha256`, `modified`) of a folded or re-resolved
  plan are rewritten on a refresh, and the plan is flushed through the store's ordinary write-back.
- **Stale operations are reported.** `ListPlans` and `PlanStatus` list each stale operation with its
  reason, and `check` reports one as a finding. A stale operation is never silently re-targeted; the
  author re-anchors it (`restructure anchors`).
- **`apply` refuses a stale operation** at or after the run's start, before any write, with
  `FailedPrecondition`, naming the operation and why. Only the operations the run would reach count:
  with `--stop-after N` a stale operation beyond the stop is not refused, and a `--dry-run`, which
  writes nothing, is not refused at all.
- **`restructure snapshot` of an item-anchored plan** does the same re-resolution once for a plan
  that is not loaded, so a plan written before a rebase runs again without hand re-anchoring; a v1
  range plan is told to convert with `anchors --at`.
- **Derived, held for a load, never touched unloaded.** Staleness is worked out by re-resolution and
  kept in memory for as long as the plan is loaded; the flushed plan stays a plan. A plan nobody
  loaded is never touched, byte for byte. An unloaded item-anchored plan is judged when it is applied,
  by the resolver's fingerprint refusal; a v2 plan's drift is reported and never refused, as ever.

One renderer prints the stale operations for the in-process CLI, the daemon and `tddy-tools`.

How the crate delivers this: [plan-store.md](../../../packages/tddy-code-restructuring/docs/plan-store.md#live-plans).

## Transactional groups

Some refactors cannot compile step by step: change a type, then adapt every use. A plan marks the
operations that must compile only together by giving them the same `"group"`:

```jsonl
{"op":"rename_symbol","id":"op-1","group":"retype","anchor":{ … },"name":"Level"}
{"op":"rename_symbol","id":"op-2","group":"retype","anchor":{ … },"name":"Depth"}
```

- **Authoring.** A group is `"group": "<name>"` on **consecutive** operations. Members that are not
  next to each other are refused as malformed, naming the group, so a group is always one run of lines.
  An operation without `group` is ungrouped.
- **Judged at its end.** `cargo check --all-targets` runs over the packages owning every file the
  group's members edited, once, after the last member. A group that compiles is kept; its members then
  reach the plan store together, in plan order (they are not refreshed while the group is open).
- **Rolled back exactly.** A group that does not compile is undone byte for byte — edited files
  rewritten, files the group created removed, renames reversed — from the contents the journal holds,
  and the run stops with ``group `<name>` does not compile at its end, so it was rolled back:`` followed
  by the compiler's errors. Everything before the group stays applied. **Any** failure inside a group
  (a member that cannot be resolved or committed, a check that cannot run) rolls the whole group back
  and reports that failure unchanged. A run that is cancelled leaves the group open instead, for a
  resume to roll back.
- **Resume inside a group.** `--resume` finds a group the journal started and never closed, rolls it
  back, and applies it again from its first member.
- **Reported once kept.** A group's members are reported as applied only after the group compiled, each
  followed by a `   group: <name>` line. A rolled-back group reports none of its members.
- **`check --deep`.** A group with a refused member is one finding naming the group and listing each
  refused member's reason; the rehearsal does not advance past a refused group.
- **`stop_after`** is judged where a group would begin, and every member of a group counts toward it.
- **Ungrouped operations are unchanged.** They are gated once at the end of the run, and a run that
  fails there leaves its edits on disk.

How the crate delivers this: [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md#transactional-groups).

## Item anchors

A plan names what an operation acts on in one of three ways:

| Anchor | Names | Survives an edit elsewhere in the file |
|---|---|---|
| `symbol` | a bare name — the **first** outline node matching it | no: two `impl` blocks both defining `new` are whichever comes first |
| `range` | absolute line and column, trusted exactly | no: it points at whatever now sits on those lines |
| `item` | a crate-rooted item path plus a range relative to that item | yes |

An `item` anchor looks like this:

```jsonc
{"kind":"item",
 "item":"tddy_core::workflow::Stack::new",           // authoritative: the enclosing item
 "file":"packages/tddy-core/src/workflow/stack.rs", // where to look; the resolver trusts it for lookup only
 "start":{"line":4,"col":9},"end":{"line":14,"col":11}, // relative: line 1 = the item's first line
 "fingerprint":"sha256:…",                           // the item's text when the anchor was written
 "hint":{"line":188,"col":9}}                        // absolute, orientation only; never read
```

`item` is the crate's name, the module path, then the item and member segments. A member of a trait
impl that two impls share is addressed with the trait, `…::<Stack as Display>::fmt`. `start`/`end`
count from the item's first line, outer attributes and doc comments included; both omitted means the
item itself, at its name, which is what `rename_symbol`, `move_module_to_crate` and `inline_method` act
on. An `items` anchor, `{"kind":"items","file":…,"items":[…],"fingerprints":[…]}`, is a contiguous run
of sibling items for `extract_module`, resolved to the span from the first item's first line to the
last item's last line.

**Resolution.** Every item anchor is resolved once, at run open, against the tree the run starts on,
by walking rust-analyzer's document outline. A plan written against an older tree therefore runs as
long as its items are intact: an edit anywhere outside an anchored item leaves the anchor correct.
Resolution happens before the baseline compile gate, so a refusal costs no compile and leaves no
`.restructure/` behind.

**Refusals.** Nothing is guessed and nothing searches another file. An item is refused when its
crate or module prefix does not match `file`, when a segment is absent from the file, when a segment
matches more than one node (a trait-impl collision is refused until `<T as Trait>::m` is used), when a
relative range reaches outside its item, and when the items of an `items` anchor are not adjacent. An
edit **inside** the anchored item changes its fingerprint and is refused as `FailedPrecondition`,
naming the item and its file; the message does not name the operation, because it is raised while the
plan is resolved as a whole, before any operation runs. An item anchor in a file no backend can
resolve items for is refused by name (`NoBackend`, or `UnsupportedOp` for a backend without an item
resolver). Item anchors are Rust only.

**Authoring.** Do not write these by hand. `anchors --at` turns a line the author read into an anchor
that survives, and `anchors --items` does the same for a run of items. Applying an `extract_method`
through an `--at` anchor produces the same edit as the equivalent range anchor.

How the crate delivers this: [item-anchors.md](../../../packages/tddy-code-restructuring/docs/item-anchors.md).

## Rust operations (v1)

| Operation | Notes |
|---|---|
| `extract_method` | Range → new function. A function-local `use` the range needs is carried into the new function |
| `extract_variable` | Subexpression → binding. A place selected under a `&` / `&mut` is bound as the borrow (`let x = &self.v;`) |
| `rename_symbol` | LSP rename, applied to **every** document rust-analyzer returns edits for, not only the anchor's own file |
| `extract_module` | `reexport`: glob / named / none; optional `to_file` |
| `extract_module_to_file` | Move items to new file |
| `extract_trait` | Extract trait from impl |
| `inline_method` | Inline callee |
| `remove_unused_param` | Remove a parameter the body never reads, from the declaration and from **every call site in every file**. The anchor names the function and `name` is the parameter. rust-analyzer offers the removal only for an unused parameter, so naming a used one is refused — the refusal names the parameter and says it is used, and nothing is written. A `name` that is not a parameter of the function is refused as well |
| `convert_tuple_return_to_struct` | `-> (A, B)` becomes `-> Name`, a new tuple struct that keeps the function's visibility, and every destructuring caller is rewritten (`let (a, b) = f()` becomes `let Name(a, b) = f()`). The anchor names the function and `name` is the struct. rust-analyzer names the struct after the function; the engine has the server rename it to `name`, declaration and callers together. A function with no return type is refused |
| `move_module_to_crate` | Move `<crate>/src/<module>.rs` into another crate: `git mv` the file, rewrite every path it names — in `use` items at any depth and in bodies — from its [path survey](#path-survey), re-point every caller found by `textDocument/references`, and edit both `Cargo.toml`s. `to` is the destination crate's directory and is required. `reexport: "glob"` leaves one grouped `pub use <dest_crate>::{a, b};` per destination in the origin, naming the modules that moved there across the whole plan, which gives a **zero caller diff**; `"named"` is refused, because a named re-export puts items at the destination's crate root while a caller writes `crate::<module>::Item` |
| `move_cluster_to_crate` | Move a **set** of modules into another crate as one unit. `anchor` is the first member and `also` names the rest; `to` and `reexport` behave as above. The whole set moves or none of it does, in a single edit, so the tree is never half-moved. A path reaching a **co-moving** member stays `crate::` — the destination *is* `crate` once the file has arrived — while a path reaching a module staying behind is re-pointed at the origin. Every member's paths are read by the same survey as a single module's. This is what makes a mutually-referencing group movable; a set of one is refused, because that is `move_module_to_crate` |
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

**Progress is throttled where it is printed.** A server reports far more often than a person reads, so
the printed stream shows one line per token every two seconds. The daemon's structured warm stream
does not throttle: a client that wanted the 50% it was never sent cannot ask for it again, so every
phase reaches it.

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
| snapshot / journal / anchor mismatches, `the item … changed since the plan was written` | The tree is not in the state the plan was written against, or an anchored item's text is no longer the text its fingerprint names | Repair the tree, re-snapshot, or re-anchor the item with `restructure anchors` | `FailedPrecondition` |
| `the tree does not compile before the plan runs …` | `apply`'s baseline `cargo check` failed; nothing was written | Make the tree compile, then apply again | `FailedPrecondition` |
| `N of M operation(s) were applied, and the tree no longer compiles …` | Every operation was accepted and the compiler rejects the result. The edits are left on disk and in the journal | Fix the compiler-named errors by hand, or roll back as the message says (restore the touched paths from git, remove the journal) | `Internal` |
| ``group `<name>` does not compile at its end, so it was rolled back: …`` | A transactional group's members were accepted and the compiler rejects the tree they leave together. The group was undone exactly; earlier operations stay applied | Fix the plan so the group compiles at its end, or the code it depends on | `FailedPrecondition` |

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
| A name the parent binds by `use` that shadows a prelude name (`use crate::Result;` over `std::result::Result`) | carried by a lexical pass over the parent's `use` declarations and the moved text. The server never reports such a name unresolved — the prelude resolves it, to a different type — so the compiler would be the first to notice. The pass over-imports on purpose and the [tidy](#the-tidy) removes what is unused |
| A type widened by the move that a widened item's signature names | stays widened, so the item's signature does not name a type less visible than itself |
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

### Moved code that changes meaning one module deeper

`extract_module` puts the items in a new child of the module that held them, and text that was correct
there can mean something else one level down. Three rewrites keep the meaning:

- **Relative visibility is rebased.** `pub(super)` becomes `pub(in super::super)` and `pub(self)`
  becomes `pub(super)`, on a moved item and on the fields of a moved struct; `pub`, `pub(crate)` and
  `pub(in crate::…)` mean the same anywhere and are unchanged.
- **Inline `super::` and `self::` paths are re-rooted** in the moved code — calls, types, struct
  literals, patterns — so `super::f()` still names the original parent's `f`.
- **A path through a module the parent declares** (`visibility::WIDENED`, written next to `mod
  visibility;`) is written `super::visibility::WIDENED` in the new module, unless the moved code could
  bind that name itself.

A run that re-roots paths says so in one note line, `paths: N path(s) re-rooted for the new module`.

## The tidy

A successful `apply` ends with a **tidy** over the files it wrote, once the tree compiles:

1. **Unused imports are removed.** The evidence is rustc's own `unused_imports` report — trait-aware,
   so `use std::fmt::Write;` is never judged by its name — and only machine-applicable suggestions are
   applied, over the files the run wrote, in a bounded number of rounds (removing one import can orphan
   another).
2. **An import only the parent's tests use is gated** `#[cfg(test)]` instead of removed; a member of a
   group becomes its own gated item.
3. **Both compile units are read.** A library checked with its tests is built twice and each build
   reports its own unused set. They are composed per `use` statement: unused in both, removed; unused
   only in the library build, gated; otherwise kept. The compiler's own member spans are never applied
   to a statement the units disagree on, because they overlap.
4. **Formatting.** `rustfmt` runs over every file the run wrote, with its package's edition.
5. **A failed check is repaired while it improves.** A round that does not compile is undone and
   redone with the names the errors quote gated, and again while each redo leaves fewer names
   failing (at most five times). Overlapping edits are an error, never a silent skip.
6. **Whatever the compiler still warns about is reported**, never fixed.

A tidy that cannot repair a round undoes it and fails the run, saying so; it never leaves a broken
tree, and never touches the plan's own edits.

## Verify

`restructure verify --against <git-ref>` compares the working tree with a ref **statement by
statement**, as a multiset across the crate, so the one thing nothing else sees — a comment attached to
no item, which rust-analyzer carries nowhere — surfaces. What an `extract_module` always causes is
excused, counted, and summarised in one line, so the exit status is a signal:

- a multi-line `use` item is compared as its first line, and a `#[cfg(test)]` directly above a `use`
  is dropped;
- a leading visibility (`pub`, `pub(crate)`, `pub(super)`, `pub(in …)`) is ignored;
- a call re-pointed through a lowercase module qualifier (`f(` becoming `m::f(`) pairs with its
  original, and a statement `rustfmt` wrapped is joined back before comparing;
- what is left is compared last as a **token multiset**; equal multisets mean reflow only.

`VerifyResponse` carries the counts as `repointed`, `visibility_normalised` and `cfg_test_gates`. A
real loss prints `verify: tokens lost: …; tokens gained: …`, and a lost comment is still reported. One
real loss anywhere keeps the reflow of the other leftovers reported beside it.

## Workflow

1. Green baseline — `./test -p <crate>`.
2. Run [analyze-code-issues](rust-code-analysis.md); record CRAP targeting in the changeset.
3. Author intents in JSONL; `restructure check` (optionally `--deep`) before apply.
4. `apply --dry-run`, then apply; `verify --against HEAD` after successful extract operations.
   `apply` runs `cargo check --all-targets` over the touched packages before and after, and fails the
   run when the result does not compile, and ends with [the tidy](#the-tidy). Operations that cannot
   compile one by one go in a [transactional group](#transactional-groups), gated and rolled back as a unit.
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

## Path survey

A cross-crate move decides everything it writes about the moved file from one **path survey**, taken
before any edit: the rewrite of the file's own paths, the test for a dependency back on the crate it
left, and the destination's manifest. The three cannot disagree about what the file names.

- **What is read.** Every path the file writes — in `use` items at any depth (groups expanded) and in
  bodies — whose first segment is `crate`, `self`, `super` or a crate. A `use` item's first segment is a
  crate unless the file binds the name itself (a module it declares, something it imports, an item it
  defines — `use Kind::*;` inside a function over the file's own `enum Kind`). In code a path counts
  only when the origin's manifest declares its first segment.
- **How it is resolved.** `self::`, `super::` (any depth) and `crate::` are resolved by segment against
  the file's module path, never by string prefix; a `super::` that climbs above the crate root is
  refused. The result is followed through the origin's re-exports — explicit `use` and glob, chained —
  to the crate and item that **define** it.
- **How it is rewritten.** A path to something the destination defines becomes `crate::…`, whether it
  was written as `destination::…` or as a `crate::…` the origin only forwards there; one to something
  staying in the origin names the origin; one to a third crate names that crate; a path to a co-moving
  member stays `crate::`. Headers and bodies are rewritten alike. A `use` leaf whose last segment
  changes keeps the name the body goes on using with `as` (`use crate::records as roster;`). A `use`
  group whose members would need different qualifiers is refused with "write one `use` per path", since
  a group has one prefix to write.
- **What is an edge.** Only a path to an item the **origin** defines that stays behind makes the
  destination depend on the crate it left — in a header or a body. A path the origin merely forwards from
  another crate, a `super::` into the moved set, and anything under `#[cfg(test)]` are not edges: cargo
  allows a dev-dependency cycle.
- **`check` reports what `apply` cannot build, from the same reading.** Two static findings, needing no
  index, so `check` and `check --deep` both give them. A moved module whose **body** reaches
  `crate::host::f(…)`, with `host` staying behind in the origin, is reported with the file, the path as
  written and its line; the remedy is to cut the body's dependency, never `move_cluster_to_crate`, since a
  body's reach into the code that hosts it is not a sibling that can come along. A module staying behind
  is read at the operation's point in the plan: one an earlier operation already moved to the same
  destination has left, one a later operation moves has not, and one moved elsewhere has not. A destination
  whose root already declares the module's name, or already holds a file at the target path, is reported as
  a **merge**, which no operation performs.
- **The manifest.** Each crate the survey names is copied from the origin's manifest (a path dependency re-anchored) into the
  destination's `[dependencies]`, or `[dev-dependencies]` when only `#[cfg(test)]` code names it. The
  destination is never in its own manifest; the move asserts it, and a survey that reports one is
  refused as inconsistent rather than filtered. A crate declared nowhere it could be copied from refuses
  the move.
- **Real read errors refuse.** A module file the walk through the re-exports must read and cannot is an
  error naming it; only "no such file" means a name is not there.

How the crate delivers this: [path-survey.md](../../../packages/tddy-code-restructuring/docs/path-survey.md).

## Known limitations

- **`remove_unused_param` reports every refusal from the server as a used parameter.** The refusal
  names the parameter and says it is used, with the server's own reason in parentheses; a parameter
  rust-analyzer declines to remove for another reason (a trait method whose impls would diverge, for
  instance) is reported the same way, and the parenthesis is where the cause shows. Only a parameter
  bound by a plain identifier (`name`, `mut name`) can be named; a destructuring pattern in the
  parameter list is refused as not a parameter of the function.
- **Both signature operations use the crate's byte columns**, as every other anchor does, so a
  non-ASCII character earlier on the parameter's line moves the caret off the name the server expects.
- **An assist may relocate less than the anchor asked for, and rewrite the remainder in place.**
  rust-analyzer decides the extraction's real extent; when it moves part of the anchored range, it
  rewrites what it left behind to reach the new module through qualified `module::Item` paths. The
  run reports which lines stayed. Author the anchor with `anchors --items` or `anchors --at` rather than by
  hand — a range that clips a helper is the usual way into this — and read the widening report the run prints.
- **A continued run refuses a plan the journal cannot vouch for.** `--resume` and `--from` over a partly
  applied plan read anchors the plan store wrote back after each operation, and check them against a
  digest the run journalled first. A plan whose pending anchors differ (the run stopped before the
  write-back, or an anchor was edited by hand) is refused as out of sync, and an item-anchored plan
  over a journal from before write-back cannot be verified; both say to run the remainder from a new
  plan file. A plan of ranges and symbols over such an older journal resumes as before.
- **A crash between a committed operation and its plan write-back is detected, not repaired.** The next
  resume refuses as out of sync; the lost refresh is not reconstructed, so the remainder runs from a new
  plan file. A hand edit of a pending *anchor* between runs is refused the same way; an edit to anything
  else (`name`, `variant`) is not.
- **Staleness does not outlive a load.** After an unload and reload, or a daemon restart, an operation
  that was `edited by` another plan is not reported stale. An item anchor is still caught at
  apply by the fingerprint refusal; a `range` or `symbol` anchor in a v2 plan is not fingerprinted and
  runs at its old coordinates, which is how v2 plans behave (drift reported, never refused).
- **An item anchor in a file deleted underneath the daemon does not go stale**, and hand-edit
  staleness is picked up by the next `Check`, `Apply` or `Anchors` request, not by `ListPlans` or
  `PlanStatus`.
- **A foreign edit overlapping an operation the plan already ran** marks that operation stale in
  `ListPlans`; `apply` ignores it, since it refuses only operations at or after its start. The
  applied plan's own v2 `files` hints are not rewritten by its own apply.
- **`tddy-tools restructure snapshot` of an item-anchored plan starts a cold language server**, even
  with a daemon running; there is no `Snapshot` RPC.
- **A rolled-back group leaves an empty directory** it created a file in; the file is removed, the directory is not.
- **An apply consumes its plan file.** A run rewrites the plan as it goes, so a plan whose apply
  failed and was rolled back is stale; re-running it is refused as an item that changed. Regenerate
  the plan.
- **`--from <id>` does not reach the daemon.** `ApplyRequest` carries no field for it, so the daemon path
  refuses it, naming the workaround. Journal records and `OperationApplied` events carry `op_id`.
- **`check` without `--deep` cannot examine item anchors.** A static check has no server to resolve
  them with, so it reports each item-anchored operation as a finding that says to run `check --deep`;
  a plan of item anchors never passes a static check green.
- **An item anchor's refusal names the item, not the operation.** It is raised while the plan is
  resolved as a whole.
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
  lines, by `check`, `check --deep` and `apply` alike. The one exception is a range that runs to the
  end of a function that returns a value, ending with its tail expression; a function returning `()`
  has no such tail, so a range holding a `return` there is refused too. A `return` inside a closure,
  an `async` block or a nested `fn` does not count. The scan is lexical: a `return` a macro expands to (`bail!`) and a
  `break`/`continue` leaving the range are not seen, and only `apply`'s compile gate catches them.
- **An `extract_variable` does not hang.** The type probe asks at the first position of the range
  that can carry a hover, skipping a leading `&`, `&mut`, `*`, `!`, `-` or `(`, and a hover that stays
  silent for 30 seconds once the index is ready ends the operation as `rust-analyzer's answer was
  unusable:`, naming the position, so the warm workspace answers the next request.
- **An `extract_variable` never binds a borrowed place by value.** A place selected inside `&place`
  or `&mut place` is widened to the borrow when both are on one line; otherwise the selection is
  refused as `rust-analyzer's answer was unusable:`, advising to select the borrow including its `&`.
  The refusal reads the text, not the type, so a `Copy` place under a borrow is refused as well.
- **An `extract_method` carries a function-local `use`** the range names into the new function. The
  origin keeps its own `use`, which [the tidy](#the-tidy) removes when rustc reports it unused.
  `check` reports the carried items on its progress line, not as a finding.
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
- **The survey reads tokens, not types.** In code a path is read as a crate only when the origin's
  manifest declares its first segment, so `mpsc::channel` and `PermissionMode::Plan` add no dependency
  line. Items a macro generates and paths written inside a macro's arguments are not seen, `pub(in …)`
  visibility is not interpreted, and a `#[path = "…"]` module is not followed. The `cfg(test)` marker is
  recognised as `#[cfg(test)]` and `#[cfg(all(test, …))]`; any other spelling (`any(test, …)`,
  `cfg_attr`) reads as ordinary code, which puts a crate in `[dependencies]` instead of leaving it out.
  The build after the move surfaces whatever the survey could not see.
- **`check` does not read a nested `use`, and a cluster's stranded-sibling finding reads the header only.**
  A body path to a module staying behind is a finding of its own, but a `use` nested inside a function is
  read by neither, and the stranded-sibling finding takes only the file's top-level `use` header from the
  survey. So `check --deep` can pass a move `apply` refuses on a path inside a nested `use`, or on a body
  path that only that finding would have named with a cluster as the remedy.
- **A caller that imports the module rather than the item is not re-pointed.** The survey asks
  rust-analyzer for references per *item*, so a caller written `use crate::host_registry;` and then
  `host_registry::X` is outside the reference set. Covering it needs a second engine call on the
  `mod` declaration.
- **A rewritten caller path keeps the module segment**: `crate::host_registry::HostRegistry` becomes
  `tddy_host_service::host_registry::HostRegistry`, never `tddy_host_service::HostRegistry`. That is
  what makes the facade free — it re-exports the module, so the origin's own paths keep
  resolving.
- **A facade names what moved.** A root glob would re-export the destination's whole root, shadowing
  any name the origin already binds (`hidden_glob_reexports`) and repeating once per operation
  (`unused import`). The grouped line names the modules only, sorted, in the order destinations were
  first moved into. A line an earlier operation of the plan wrote for the same destination is extended
  in place; a `pub use` that re-exports an item, a glob or an alias is the author's and is left alone.
- **The destination declares a moved module in sorted position** among the `mod` lines already in its
  root; modules landing in the same gap keep the order the plan named them.
- **A parent's re-export of the moved module follows it.** With no facade, `use <module>::*;` and
  `use <module>::{…};` at the top level of the declaring file are rewritten to name the destination,
  keeping their visibility, and the declaring crate then counts as depending on it. Indented `use`
  lines, and lines inside an inline module or function body, are not rewritten.
- **A `crate::` path in the moved file's `mod tests` is re-pointed like any other**, and a crate first
  named under `#[cfg(test)]` is a `[dev-dependencies]` entry, never an edge back; a `super::` that stays
  inside the moved file is left alone.
- **A test-binary move sees through a crate-root `pub use <crate>::*;`.** The crate it names is found
  by the `path` of the origin's dependency of that name (its `[lib] path` when the manifest sets one)
  and confirmed by that crate's own root declaring the module.
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
