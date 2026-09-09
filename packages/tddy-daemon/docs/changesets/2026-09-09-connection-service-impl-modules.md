# 2026-09-09 — `connection_service.rs` 16,659 ➜ 2,634 lines: every `impl` block to its own module

**Type:** Refactor

The file is a facade — its `use` header, a `mod` line per submodule, and the `pub use` re-exports the
families outside the module are named through. Fifty-nine modules under `src/connection_service/`
hold the rest: `rpc_service.rs` for the RPC trait impl, seventeen `svc_*.rs` for the inherent impl
blocks (each named after the first method it carries), four trait impls for the service's helper
types, eight free-item families, and twenty-nine test modules.

**No caller anywhere was rewritten.** A method is reached through its type rather than a module path,
so moving a whole `impl` costs no caller churn — that is why seventeen blocks could relocate with
nothing outside changing. The eight free-item families carry `reexport: "glob"` facades, which keeps
the 93 files that name their symbols untouched as well.

Getting there needed the four large inherent impls cut into file-sized blocks first, by hand, because
`extract_module` is the only splitting operation and an `impl` body cannot hold a `mod`. Thirteen `}`
/ `impl ConnectionServiceImpl {` pairs, verifiable as a **zero-moved-line diff**: 0 lines lost, 26
gained, being exactly those thirteen pairs. Two methods (`start_session_core` 830 lines,
`start_sandboxed_claude_cli_session` 606) are each larger than a target file on their own and stayed
whole; `extract_method` needs full type inference and did not finish on a file this size.

Behaviour unchanged: `cargo test -p tddy-daemon --lib` 809 passed / 0 failed, matching the recorded
baseline, and `cargo clippy --workspace --all-targets --locked -- -D warnings` clean. Every moved
character came from rust-analyzer. The hand-written lines are the block boundaries above, imports the
move could not restore, six types widened to `pub(crate)` to match methods the assist had already
widened, and a `cargo fmt --all` pass — a body correctly wrapped at one indentation is not correctly
wrapped at another, and `svc_start_session_core.rs` alone needed 156 lines rewrapped.

**Seven files stay over 500 lines.** `rpc_service.rs` (6,940) is out of scope permanently: 111 trait
members — 90 RPCs and 21 stream associated types — need 385 lines of signatures alone, a trait impl
cannot be split across blocks, and the only route lower costs go-to-definition and grep on every RPC
name. The other six are deferred with reasons recorded in
[`docs/dev/todo/2026-09-09-restructure-defects-from-the-connection-service-split.md`](../../../../docs/dev/todo/2026-09-09-restructure-defects-from-the-connection-service-split.md).

Dev doc: [connection-service.md](../connection-service.md) § *Where the code lives*, which carries the
module map and where new code goes.
