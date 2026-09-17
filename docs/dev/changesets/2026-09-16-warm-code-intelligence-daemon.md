# 2026-09-16 — Warm code-intelligence daemon

**Type:** Feature

`tddy-index-daemon` holds a warm rust-analyzer index per workspace root and serves the plan-driven
restructuring and analysis operations as `code_index.CodeIndexService`, over gRPC (TCP and Unix
socket) and stdio, concurrently. The same binary runs one operation in process and exits when given
no transport argument — single-shot calls the generated service trait directly, so there is one code
path rather than two. Measured against a real rust-analyzer: a second request on a warm root is
~2,500× faster than its cold load (865 µs against 2.15 s).

`tddy-tools restructure` uses the daemon when `TDDY_INDEX_SOCKET` is set and non-empty, and otherwise
spawns its own language server exactly as before, so CI is unaffected by construction. A socket that
is set but unreachable is an error rather than a silent fall back. `run-index-daemon` starts or
reuses one per checkout; `release`, `install` and `publish.sh` ship the binary. `tddy-daemon` can own
its lifecycle — lazy spawn, restart, idle stop, cancellation on shutdown — behind an `index_daemon:`
config section that is absent by default.

**`--indexing-budget` is withdrawn.** It set a warm-up bound and derived the per-operation bound as a
twentieth of itself, so `--indexing-budget 900` produced a 45-second ceiling and refused plans that
had already indexed for twenty minutes. Waits now end on readiness or on the caller going away,
checked inside the poll loops because the engine is synchronous under `spawn_blocking` where dropping
the calling future stops nothing. An indexing timeout is no longer reported as `plan is malformed`.
This answers § 2 of `2026-09-10-move-module-to-crate-cannot-move-an-entangled-cluster`, which stays
open for its § 1.

`tddy-code-restructuring` no longer writes to stdout. Its five entry points return what they printed,
`dispatch` returns an outcome the front end renders, and every entry point takes the workspace root it
acts on rather than reading the process directory — which is what lets one process serve several
worktrees. A test reads the crate's own sources and asserts only the command-line front end prints,
because a daemon serving `Check` over `--stdio` would otherwise write findings into the stream
carrying its RPC frames.

`tddy-lsp` gains what a host that outlives one request needs: per-URI document versions with
`did_change` / `did_close`, an idle timer that a borrowed client refreshes, one server spawned under
concurrent cold demand, and a drained server stderr. All four were live defects in the registry
`tddy-daemon` already ran.

Analysis gains `Coverage`, `Report`, `DuplicateTests` and `Complexity`, with complexity cached by
content hash so an unchanged file is not rescored. No dependency was added anywhere.

- Product: [`../../ft/coder/warm-code-intelligence-daemon.md`](../../ft/coder/warm-code-intelligence-daemon.md)
- Packages: `tddy-index-daemon`, `tddy-lsp`, `tddy-code-restructuring`, `tddy-code-analysis`,
  `tddy-tools`, `tddy-daemon`, `tddy-daemon-kernel`
