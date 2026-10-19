# `#satellite` 7/13 left three files over the 500-production-line budget

**Date:** 2026-10-19
**Source:** `/pr-wrap` file-length gate on #629 (`#satellite` 7/13). Deferred with the developer's consent (2026-10-19, relayed to the wrap by the coordinating agent; not confirmed in the wrapping session itself).

Every file was already over budget on `master` and has a standing code-issue record. #629 grew three of them by
+19, +3 and +8 lines, and decomposing them inside a 42-file PR is disproportionate to that growth. None of
`#630`–`#635`'s own commits touches them, and no record carries a `Claimed by:` line, so the splits are free
to land after the `#satellite` stack.

| File | Production lines | Grown by #629 | Record |
|---|---|---|---|
| `packages/tddy-session-sync/src/attach.rs` | 536 → 555 | +19 | `packages/tddy-session-sync/docs/code-issues/oversized-file-attach.md` |
| `packages/tddy-session-tool-client/src/lib.rs` | 1,111 → 1,114 | +3 | `packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md` |
| `packages/tddy-terminal-rpc/src/pty_relay.rs` | 776 → 784 | +8 | `packages/tddy-terminal-rpc/docs/code-issues/oversized-file-pty-relay.md` |

`packages/tddy-session-sync/src/sync.rs` (788 → 788) was touched but not grown; it is alert-only and recorded
in `packages/tddy-session-sync/docs/code-issues/oversized-file-sync.md`.

The growth is the join through the client room port: the connector call and its error mapping
(`join_session_room`, `join_terminal_room`), the connector install and its error type (`install_room_connector`),
and named constants for what were inline literals, against the SDK-typed token mint and connect code that left.

**What would close it:** the splits named in each record, applied with `code-restructuring` after the
`#satellite` stack lands.
