# 2026-09-24 — `restructure apply` gaps from the first real run of the lifecycle destructure plans

**Category:** Future enhancement
**Source:** `#carve` 13/15 `/green`, [#527](https://github.com/uppin/tddy-coder/pull/527), changeset
[`2026-09-23-restructure-engine-fixes`](../1-WIP/2026-09-23-restructure-engine-fixes.md)

#527's engine was run for real against #524's plans
(`docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/` on the `feature/carve/lifecycle-wiring`
branch). Every plan passed `restructure check --deep` against a healthy warm index, and then `apply`
went like this:

| Plan | Result | Gap |
|---|---|---|
| `03`, `10a`, `04`, `08` | applied, compiles | — |
| `06` ports files | 3 of 3 applied, **does not compile** | G |
| `07` host builders | 1 of 1 applied, **does not compile** | H |
| `05` spawn_split_agent | 5 of 5 applied, **does not compile** | L |
| `02` cli_session_manager (9 seams) | 9 of 9 applied, **does not compile** | G, I, J |
| `09` without op 6 | 7 of 7 applied, **does not compile** | K |
| `01` connection_service (10 seams) | 10 of 10 applied, **the test build does not compile** | M |

In every row the code was moved, and #527's compile gate failed the run with the compiler's errors,
leaving the edits on disk. Before #527 each of these rows would have been reported as a success.

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

### H — relative paths in the moved body are not rebased

Plan `07`. The assist does not rebase `super::`/`self::` in the code it moves. Gap A fixed only the
`use` lines that the import pass reconstructs.

```rust
// before: connection_service/svc_resolve_tddy_tools_path.rs, line 163
let handler: Arc<dyn tddy_sandbox_runner::HostRpcHandler> =
    Arc::new(super::DaemonRpcHandler { conn: Arc::downgrade(self) });

// after: connection_service/svc_resolve_tddy_tools_path/svc_host_builders.rs, one module deeper
    Arc::new(super::DaemonRpcHandler { conn: Arc::downgrade(self) });
//           ^^^^^^^ now names svc_resolve_tddy_tools_path; must become `super::super::`
```

```text
error[E0422]: cannot find struct, variant or union type `DaemonRpcHandler` in module `super`
```

A `super::` inside a nested module of the moved code that stays within the moved code must **not**
change:

```rust
mod moved {                // the seam
    fn helper() {}
    #[cfg(test)]
    mod tests {
        use super::helper; // still `super::`: it points inside the moved code
    }
}
```

### I — `use Trait as _;` is not carried

Plan `02`. Both parents bring `prost::Message` in only for its methods:

```rust
// before: cli_session_manager.rs, lines 16, 1203 and 1247
use prost::Message as _;

let frame = TerminalOutput { data: replay }.encode_to_vec();
if let Ok(input) = TerminalInput::decode(&msg.payload[..]) { … }

// after: cli_session_manager/livekit_bridge.rs has both calls and no `use prost::Message as _;`
```

```text
error[E0599]: no method named `encode_to_vec` found for struct `TerminalOutput` in the current scope
error[E0599]: no function or associated item named `decode` found for struct `TerminalInput`
```

A trait used only for method resolution never shows up as an unresolved name. Since E1, `as _` also
binds no name, which is right for the name check but hides the import. A sound signal is still to
be found. Possibilities are an unresolved-method diagnostic, a parent `use … as _` that goes unused
after the move, or the design candidate below.

### J — a sibling seam's private helper goes unresolved after plan `02`'s nine seams

```rust
// before: cli_session_manager.rs
impl PtyHandle {
    pub fn send_input(&self, data: bytes::Bytes, input_offset: u64) {
        let (resize, remaining) = strip_resize(&data);   // line 115
        …
    }
}

fn strip_resize(data: &[u8]) -> (Option<(u16, u16)>, Bytes) { … }   // line 1126, private

// after: cli_session_manager/pty_handle.rs
use super::strip_resize;
```

```text
error[E0432]: unresolved import `super::strip_resize`: no `strip_resize` in `cli_session_manager`
error[E0282]: type annotations needed        (pty_handle.rs:104, follows from the above)
```

The cause is not investigated. The suspicion is cross-op composition. Another of the nine seams also
moves `strip_resize` into a module of its own, and narrows it back to private because nothing
outside needed it at the time. The glob facade `pub use that_module::*;` then does not re-export it,
and a later op's reconstructed `use super::strip_resize;` names a parent that no longer has it.

### K — an `extract_method` signature names a type the file never imports

Plan `09`. The import pass never runs for `extract_method`.

```rust
// before: connection_service/svc_start_sandboxed_claude_cli_session.rs
managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,   // line 87, the only spelling
…
if let Some(recipe) = managed_recipe.clone() { … }                    // line 382, in `managed_jail_env`'s range
…
recipe: managed_recipe.as_ref().map(|r| r.name().to_string()),        // line 631, in `write_jail_session_metadata`'s

// after (shape): each new fn takes the recipe under the short name, and nothing imports it
fn managed_jail_env(…, managed_recipe: Option<Arc<dyn WorkflowRecipe>>, …) -> … { … }
fn write_jail_session_metadata(…, managed_recipe: Option<Arc<dyn WorkflowRecipe>>, …) -> … { … }
```

```text
error[E0405]: cannot find trait `WorkflowRecipe` in this scope     (lines 465 and 642)
```

The fix is either to qualify the path as the file does, or to import it with the server-offered
import checked by the occurrence count.

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

The import pass sees only the text it produced. The names that go unresolved are in **other files**,
the out-of-line child modules whose `super` just changed, and nothing opens those. Two fixes are
possible. One is to carry the parent's glob-visible bindings the child files use into the new module.
The other is to include out-of-line children of moved `mod x;` declarations in the unresolved-name
scan. Also open: whether cfg(test)-only code is tokenised at all.

## Design candidates, undecided

- **Compiler-guided import repair.** After the gate fails, take rustc's own "not found in this scope"
  and "trait … not in scope; consider importing" errors in the files the plan wrote. Restore only
  bindings the **original parent file** declared, rebased for the new module's depth, then re-check.
  This would close G, I, K and M in one mechanism with the compiler as the oracle. It is a new
  mechanism beside the RA-driven pass, so it needs the developer's decision.
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

Plans `01`, `05` and `07` had to be re-anchored after #508 edited their files; see
[2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan](./2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md).
