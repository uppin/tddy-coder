# 2026-09-10 — `local_pty_relay::run` never returns when its stdin is an open pipe

**Category:** Known failing test
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474), M9 verification

`packages/tddy-terminal-rpc/tests/local_pty_relay.rs`'s
`runs_a_fast_exiting_command_to_completion` runs `/bin/sh -c 'exit 0'` through
`tddy_terminal_rpc::local_pty_relay::run` and asserts the relay returns once the child exits. It
**hangs indefinitely** — no timeout, no failure — whenever the test process' stdin is an open pipe
that never delivers EOF. Measured, not inferred:

| The test process' stdin | Result |
|---|---|
| `/dev/null` (or closed) | passes in ~0.1s, three runs out of three |
| a pty with no writer (`script -q /dev/null …`) | hangs; killed at 120s |
| an open pipe (`sleep 60 \| …`) | hangs; killed at 30s |
| inherited from a detached background shell | hangs; killed at ~5min |

The last row is how it was found: a plain `cargo test -p tddy-terminal-rpc` from a non-interactive
shell wedges the whole package run, and because it hangs rather than fails, `--no-fail-fast` and
harness timeouts do not help.

**The shape of it is in the teardown order.** `run` spawns a stdin pump holding a clone of the PTY
channel's `stdin_sender`, then waits for the task's status to become terminal, and only *afterwards*
drops the sender and aborts the pumps (`packages/tddy-terminal-rpc/src/local_pty_relay.rs:131-139`).
With a live stdin the pump is parked in a read that never returns, so the sender clone outlives the
child and the wait never completes. The comment on the teardown block says the pump "is blocked on a
blocking stdin read (no input will arrive), so it is aborted rather than awaited" — which is true of
the abort and not of the wait that precedes it.

**This is not a node-5 regression, and that was checked rather than assumed.** M6 moved
`RawMode`/`terminal_size` out of this module into `local_terminal.rs`; the extracted code is
byte-identical to what it replaced (`git show 3c63a6e1 -- packages/tddy-terminal-rpc/src/
local_pty_relay.rs`), `run`'s own logic is untouched, and `packages/tddy-pty` and `packages/tddy-task`
have no node-5 diff at all. The hang is reachable at every commit this function has had; what node
5 changed is only that the crate's suite is now run in isolation often enough to meet it.

Two things to fix, and they are separable:

- **The relay**: select on the child's terminal status *and* stdin, or drop the sender clone before
  waiting, so a live stdin cannot pin the master fd open.
- **The test**: it should not be able to hang. `#[tokio::test]` gives it no deadline; wrapping the
  call in `tokio::time::timeout` turns an indefinite wedge into a named failure.

Not fixed at M9 because M9 is verification and closeout: the first is a behaviour change to the
relay, and the second changes a test that node 5 is asserting it moved without rewriting.
