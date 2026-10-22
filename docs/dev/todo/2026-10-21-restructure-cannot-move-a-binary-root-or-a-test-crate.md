# 2026-10-21 — `restructure` cannot move a binary root or a test crate file

**Deferred (consent):** 2026-10-22 — user: "defer the remaining", given during `#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)). Not taken by the stack; stays open.
**Category:** Future enhancement
**Source:** `#satellite` 14/15 (PR #689), `tddy-remote-git-repo` and `tddy-session-sync` split into a library and a `-cli` wiring package

Splitting a library from the binary that wires LiveKit onto it means moving two things the
restructure engine addresses nowhere: the binary root and the suite that spawns that binary.

```text
packages/tddy-remote-git-repo/
  src/main.rs                 # the binary root: `fn main`, `use tddy_livekit::...`
  tests/cli_acceptance.rs     # `env!("CARGO_BIN_EXE_tddy-remote-git-repo")`
```

```rust
// packages/tddy-remote-git-repo/src/main.rs
use tddy_livekit::room_connector::LiveKitRoomConnector;
use tddy_remote_git_repo::{relay, ssh_argv};

#[tokio::main]
async fn main() { /* ... */ }

// packages/tddy-remote-git-repo/tests/cli_acceptance.rs
const BINARY: &str = env!("CARGO_BIN_EXE_tddy-remote-git-repo");
```

The engine refuses both:

- (a) the binary root:
  `0: plan is malformed: ... src/main.rs is a crate or module root, not a module — moving one moves everything it declares, which is a plan of its own`
- (b) the test file:
  `Error: plan is malformed: packages/tddy-remote-git-repo/tests/cli_acceptance.rs is not under packages/tddy-remote-git-repo/src, so it is no module of the package \`tddy-remote-git-repo\``

Both files moved whole, so the node used plain `git mv` (proved byte-identical with
`git show HEAD:<old> | diff - <new>`) and then fixed the manifests by hand. The same was done for
`tddy-session-sync`.

**What would close it:** a plan step that moves a whole file between packages without treating it as
a module — a binary root (`src/main.rs`, `src/bin/*.rs`) and a `tests/*.rs` suite, with the manifest
`[[bin]]` / `[dev-dependencies]` edits that follow it.
