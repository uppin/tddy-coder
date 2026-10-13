# 2026-09-24 — `restructure apply` gaps from the first real run of the lifecycle destructure plans

**Category:** Future enhancement
**Source:** `#carve` 13/15 `/green`, [#527](https://github.com/uppin/tddy-coder/pull/527), changeset
[`2026-09-23-restructure-engine-fixes`](../changesets/2026-09-23-restructure-engine-fixes.md)

#527's engine was run for real against #524's plans (the destructure's restructure plans, listed in
[its change history](../changesets/2026-09-23-carve-lifecycle-destructure.md) and kept in git at
`52e621a3:docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/`). Every plan passed
`restructure check --deep` against a healthy warm index, and then `apply`
went like this:

| Plan | Result | Gap |
|---|---|---|
| `03`, `10a`, `04`, `08` | applied, compiles | — |
| `06` ports files | 3 of 3 applied, **does not compile** | G |
| `05` spawn_split_agent | 5 of 5 applied, **does not compile** | L |
| `02` cli_session_manager (9 seams) | 9 of 9 applied, **does not compile** | G |
| `09` without op 6 | 7 of 7 applied, **does not compile** | — (K closed) |
| `01` connection_service (10 seams) | 10 of 10 applied, **the test build does not compile** | M |

In every row the code was moved, and #527's compile gate failed the run with the compiler's errors,
leaving the edits on disk. Before #527 each of these rows would have been reported as a success.

**Closed since:** H (relative paths in a moved body are rebased by `inline_paths`), J (a sibling
seam's private helper goes unresolved: a plan now shows the server every file an earlier seam wrote,
so the item stays widened and reported), I (an `extract_module` carries every `use <path> as _;` of the
parent into the new module) and K (an `extract_method` signature naming a type the file does not import
is respelled as the function the range came from spells it, or refused naming the type). Their sections
are removed. What remains is G, L, M, W and the first half of X.

## Why this was deferred

The developer's call (2026-09-24): **the engine's job is to move the code; the compile errors it
leaves can be fixed by hand.** #527 already guarantees that a run which leaves them says so. It also
fixed E1–E4, gaps A–C and the index-daemon environment, and closing G–M was not needed to wrap it.
**None of these gaps block #524**: each leaves a mechanical, compiler-named fix.

**`check --deep` sees none of them.** Only `apply`'s compile gate does, so a clean check still does
not mean a clean tree.

## The gaps

`before` lines are quoted from the real file, trimmed with `…`. `after` shows what `apply` wrote;
where the exact text was not captured it is marked **(shape)**. Error lines are rustc's, from the
compile gate.

### G — attribute macros are not imported

Plans `06` and `02`. The import pass is driven by rust-analyzer's unresolved-identifier semantic
tokens, and an attribute path is apparently not reported that way.

```rust
// before: connection_service/svc_session_agent_ports.rs, lines 24 and 124–126
use async_trait::async_trait;

#[async_trait]
impl AgentCatalog for DefsResolvableFromThisDaemon {
    async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status> { … }
}

// after: svc_session_agent_ports/svc_session_agent_port_adapters.rs, lines 39–41, and no
// `use async_trait::async_trait;` anywhere in the file
#[async_trait]
impl AgentCatalog for DefsResolvableFromThisDaemon {
    async fn record_for(&self, agent_id: &str) -> Result<SessionAgentRecord, Status> { … }
}
```

```text
error: cannot find attribute `async_trait` in this scope
error[E0195]: lifetime parameters or bounds on method `record_for` do not match the trait declaration
```

Derive macros and bang macros bound by a `use` are probably the same, but this is unverified:

```rust
use serde::Serialize;   #[derive(Serialize)] struct Row { … }
use tracing::info;      fn f() { info!("…"); }
```

### L — extract-methods after an `extract_module` in the same plan get `()` types

Plan `05`, one file, with the anchors shown as the plan was written, before #508 moved them:

```jsonl
{"op":"extract_module","anchor":{…"start":{"line":366},…},"name":"svc_paired_codebase_teardown","to_file":true}
{"op":"extract_method","anchor":{…"start":{"line":259},…},"name":"write_split_agent_metadata"}
{"op":"extract_method","anchor":{…"start":{"line":203},…},"name":"split_agent_context_and_args"}
{"op":"extract_method","anchor":{…"start":{"line":188},…},"name":"split_agent_withdrawals"}
{"op":"extract_method","anchor":{…"start":{"line":129},…},"name":"join_split_livekit_room"}
```

```text
svc_spawn_split_agent.rs:175  error[E0308]: mismatched types: expected `Vec<String>`, found `()`
svc_spawn_split_agent.rs:182  error[E0308]: mismatched types: expected `()`, found `Arc<PtyHandle>`
svc_spawn_split_agent.rs:291  error[E0308]: mismatched types: expected `()`, found `Vec<String>`
svc_spawn_split_agent.rs:344  error[E0609]: no field `pid` on type `()`
```

**The four `extract_method`s applied without op 0 compile**, with real signatures:

```rust
async fn split_agent_withdrawals(&self, codebase_instance_id: &str, codebase_session_id: &str,
    req: &StartSessionRequest) -> Result<Vec<(String, Vec<String>)>, Status> { … }
```

So this is composition. Either the server had not re-analysed op 0's edit when the next assist ran
(stale inference), or the `PositionLedger` mis-mapped the ranges. The two are not yet told apart.

**Not seen again after `51211cd8`** (#524, 2026-09-24). That commit makes the backend close every
document an operation opens. Before it, a shared server went on answering for a file with the last
text an earlier run had sent. The same plan, applied once through the warm daemon, applied 5 of 5
with real signatures and passed the compile gate. One clean run does not establish that stale
documents caused L, so the gap stays open until it is reproduced or explained.

### M — a moved `mod x;` declaration changes what the test file's `use super::*` means

Plan `01`, test build only.

```rust
// before: connection_service.rs
use std::path::{Path, PathBuf};                                            // line 3
use tddy_service::proto::session::{SplitAgentPlacement, StartSessionResponse};   // line 16
…
#[cfg(test)]
mod stack_child_spawn_tests;                                               // line 823, a file of its own

// connection_service/stack_child_spawn_tests.rs, unchanged by the run
use super::*;              // reached `Path` through connection_service's own `use`
… &Path …

// after (shape): the seam carries the `mod stack_child_spawn_tests;` declaration into the new
// module, so the same `use super::*;` now names that module, which never imported `Path`
```

```text
stack_child_spawn_tests.rs:92  error[E0425]: cannot find type `Path` in this scope
workspace_start_request_unit_tests.rs:168  error[E0422]: cannot find struct `SplitAgentPlacement`
workspace_start_request_unit_tests.rs:182  error[E0422]: cannot find struct `SessionAttachment`
workspace_sandbox_roster_dispatch_unit_tests.rs:58  error[E0425]: cannot find type `Path`
```

**Re-run on #524 (2026-09-24, after `51211cd8`)**: the same 10 of 10, and the same 9 errors. No seam
of plan `01` carries a `mod x;` declaration, so the shape above is not the mechanism. What happened is
that the **parent's own `use` groups** lost every name that only the moved code used outside tests.
The assist drops a name from a group when the seam held its last use, and it does not count the
test children that reach the name through `use super::*`:

```rust
// before: connection_service.rs
use std::path::{Path, PathBuf};
use tddy_core::Changeset;
use tddy_service::proto::session::{SplitAgentPlacement, StartSessionResponse};
…
pub use service_util::{await_supervised_with_timeout, spawn_blocking_with_timeout};

// after the apply
use std::path::{PathBuf};
use tddy_core::Changeset;                    // kept, but unused in the lib build (N1)
…
pub use service_util::{await_supervised_with_timeout};   // a `pub` re-export narrowed

// by hand (#524): into the parent's existing block "Bound for the extracted test modules, which
// reach the code under test through `use super::*`"
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
use tddy_core::Changeset;                    // moved here when N1 removed the lib-level line
#[cfg(test)]
use tddy_service::proto::session::{SessionAttachment, SplitAgentPlacement};
…
pub use service_util::{await_supervised_with_timeout, spawn_blocking_with_timeout};   // restored
```

The dropped **`pub use`** member is worse than a build error. The build stays green, since in-crate
callers still reach the name through the `pub(crate) use service_util::*;` glob, but
`tddy_session_lifecycle::connection_service::spawn_blocking_with_timeout` stops being public. Nothing
outside the crate names it today, which is why no dependent crate failed. The engine should never
narrow a `pub use` it did not write.

The import pass sees only the text it produced. The names that go unresolved are in **other files**,
the out-of-line child modules whose `super` just changed, and nothing opens those. Two fixes are
possible. One is to carry the parent's glob-visible bindings the child files use into the new module.
The other is to include out-of-line children of moved `mod x;` declarations in the unresolved-name
scan. Also open: whether cfg(test)-only code is tokenised at all.

### W — an apply right after a daemon start skips its warm-up and is refused with a wrong diagnosis

#524, plan `12`, the first `apply` after `./run-index-daemon --stop` and a restart. The run logged

```text
   indexing (+0ms): warming crate index (until ready, or until you stop waiting)
   indexing (+0ms): no indexable symbols in file; skipping warm-up
```

`svc_start_session_core.rs` holds 1,105 lines of symbols, so the check that decided to skip is wrong
about the file. The assist then answered against a server that had loaded nothing, and the run was
refused before anything was written:

```text
rust-analyzer's answer was unusable: rust-analyzer left `modname` behind in 1 place(s) its rename could
not reach, so the extraction would report success over source that resolves nowhere — line(s) 478.
That call sits inside an `impl`, and the module was written outside it, so no ordering of this plan
makes the path resolve — an `impl` body cannot hold a `mod`. Either grow the seam to carry the whole
`impl`, or cut it where nothing crosses.
```

The advice is about the plan, and the plan was fine. A `check --deep` of the same plan, which warms
the index, followed by the same `apply`, moved 5 of 5. The warm-up should not be skipped on a server
that has loaded no crate, whatever the file's symbol count reads as.

### X — a moved `pub(super)` keeps its spelling one module deeper, which narrows it

#524, plan `12`. The engine widens a moved **private** item to `pub(crate)` (and logs it), but moves a
`pub(super)` item as written. One level deeper, `pub(super)` names a different module:

```rust
// before: connection_service/svc_start_session_core.rs, reachable from all of connection_service
pub(super) struct ToolSpawnPlan { pub(super) purpose: ToolSpawnPurpose, … }
    pub(super) async fn spawn_tddy_coder(&self, plan: ToolSpawnPlan) -> …

// after: connection_service/svc_start_session_core/tool_spawn_plan.rs, now reachable only from
// svc_start_session_core
pub(super) struct ToolSpawnPlan { pub(super) purpose: ToolSpawnPurpose, … }
```

```text
svc_resume_session.rs:5:57: error[E0603]: struct import `ToolSpawnPlan` is private: private struct import
svc_spawn_split_agent.rs:61:14: error[E0624]: method `attached_initial_prompt` is private: private method
svc_resume_session.rs:132:14: error[E0624]: method `spawn_tddy_coder` is private: private method
```

By hand (#524): `pub(in crate::connection_service)`, which is what `pub(super)` meant where the item
was. The engine should rewrite a relative visibility the way `inline_paths` rewrites a relative path.

The widening of a moved private item to `pub(crate)` where `pub(super)` suffices is its own entry, `2026-10-09-restructure-extract-module-widens-to-pub-crate-where-pub-super-suffices.md`.

## Design candidates, undecided

- **Compiler-guided import repair.** Deferred to its own entry,
  [`2026-10-09-restructure-compiler-guided-import-repair.md`](./2026-10-09-restructure-compiler-guided-import-repair.md);
  it would close G and M with the compiler as the oracle.
- **Re-index between ops** for L, if stale inference is the cause, waiting on the server's own
  quiescence signal as the readiness wait already does.

## Reproducing

On a local branch cut from #524 and rebased onto #527, apply each plan through the index daemon.
Restore after every run. The engine **stages** the files it creates, so a plain checkout leaves
them behind:

```bash
git reset -q -- packages/tddy-session-lifecycle
git checkout -q HEAD -- packages/tddy-session-lifecycle
git clean -fdq packages/tddy-session-lifecycle
rm -rf .restructure
```

Plans `01`, `05` and `07` had to be re-anchored after #508 edited their files; `restructure snapshot` of an item-anchored plan now re-resolves its anchors
(change history `docs/dev/changesets/2026-10-03-index-daemon-live-plans.md`).
