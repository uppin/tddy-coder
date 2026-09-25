# 2026-09-24 — a `restructure apply` that compiles still leaves CI's lint job red

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524),
plan `06-ports-files` (and plans `03`, `10a`, `04`, `08` before it), change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)

#527's compile gate runs `cargo check --all-targets`. CI's `Rust lint` job runs
`cargo fmt --all --check` and then `cargo clippy --workspace --all-targets -- -D warnings`. Every
apply on #524 that passed the gate, and every one that was made to pass by hand
([gaps G–M](./2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md)), still failed
both of those. The four applies that compiled (`03`, `10a`, `04`, `08`) left 11 warnings on the branch,
and plan `06` added 12 more. Each fix is mechanical, but a run that says it succeeded leaves CI red.

Four shapes were seen.

### N1 — the parent keeps imports only the moved code used

```rust
// before: connection_service/svc_session_agent_ports.rs
use std::path::Path;
use async_trait::async_trait;
use prost::Message as _;
use tddy_core::SessionAgentRecord;
use tddy_discovery::subagent::SubagentSession;
use tddy_worktree_service::stream::MpscResultStream;

// after plan 06: the same six lines, and none of them is used in the parent any more
```

```text
error: unused import: `async_trait::async_trait`     (clippy -D warnings; rustc: warning)
```

The same happened in `session_coordinate_handlers.rs` after `08` (`PathBuf`, `projects_path_for_user`,
`session_deletion`, five request/response types, `SpawnOptions`, `spawn_worker`), in `split_session.rs`
after `04` (`BTreeMap`, `GitHubUser`) and in `cursor_cli_spawn.rs` after `03` (`Uuid`).

A grouped `use` whose every name moved is emptied rather than removed:

```rust
// after plan 06: svc_session_agent_ports.rs and svc_session_files_ports.rs
use tddy_service::proto::session_agents_svc::{
    
};
```

### N2 — the glob facade re-exports a module that holds only `impl` blocks

```rust
// after plan 06: svc_session_agent_ports.rs
mod svc_session_agent_port_adapters;
pub(crate) use svc_session_agent_port_adapters::*;   // unused: the module is all `impl … for …`

// after plan 08: session_coordinate_handlers.rs
mod svc_resume_session;
pub(crate) use svc_resume_session::*;                // unused: `impl DaemonSessionHost { … }`
```

### N3 — an import lands in the sibling seam that does not use it

Plan `06` split one parent into two seams. The one that calls `encode_to_vec` and `decode` got no
`prost::Message` import (gap I). The one that calls neither got `use prost::Message;`:

```rust
// after plan 06: svc_session_agent_ports/svc_peer_routed_session_agents.rs:35
use prost::Message;   // unused

// svc_session_agent_ports/svc_session_agent_port_adapters.rs:246–248 as the engine wrote it,
// with no `prost::Message` import in the file
            forwarded.encode_to_vec(),
            …
                tddy_service::proto::session_agents_svc::AgentConversationChunk::decode(
```

The cause is not investigated. The suspicion is that one op's import pass counts the parent's
tokens, not the seam's.

### N4 — neither the moved code nor the reconstructed imports are formatted

```rust
// after plan 10a: svc_start_session_core.rs, one line of 612 columns
async fn spawn_tddy_coder(&self, spawn_client: Option<Arc<spawn_worker::SpawnClient>>, spawn_mouse: bool, …, host_session_socket: Option<String>) -> Result<spawner::SpawnResult, Status> {
```

That extract-method also takes 22 parameters, so it trips `clippy::too_many_arguments`. That one is
the plan's job, not the engine's: DRY #2 folds it into a `ToolSpawnPlan`. Until then it carries
`#[allow(clippy::too_many_arguments)]`, the crate's existing pattern.

## What was done by hand on #524

Each unused import and empty group was deleted, the unused glob re-exports were dropped (the
`mod` declarations stay), and `cargo fmt -p tddy-session-lifecycle` was run. No other edit was
needed; `cargo clippy -p tddy-session-lifecycle --all-targets -- -D warnings` is clean afterwards.

## Candidates, undecided

- Run the gate as `cargo clippy --all-targets -- -D warnings` plus `cargo fmt --check` on the files
  the plan wrote, so a run that leaves lint red says so, as #527 made one that leaves the build red
  say so.
- After each op, remove the parent's `use` items rustc reports unused that were not unused before
  the op. Then `rustfmt` the files the plan wrote. The first is the dual of the compiler-guided
  import repair in the gaps file, and it has the same oracle.
