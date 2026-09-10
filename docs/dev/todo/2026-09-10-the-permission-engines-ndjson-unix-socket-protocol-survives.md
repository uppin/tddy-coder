# 2026-09-10 — The permission engine's NDJSON Unix-socket protocol survives

**Category:** Future enhancement
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474), seam A

`tddy-tools`' permission-decision engine — the Claude Code `--permission-prompt-tool` endpoint,
`approval_prompt` — relays an undecidable case to its host over a **bespoke newline-delimited-JSON
Unix-socket protocol** on `TDDY_SOCKET`. Every other client of that socket has moved on:
`toolcall_client` was deliberately migrated to `tddy-rpc` framing, and node 5 moved it into
`tddy_core::toolcall::client` unchanged.

Node 5 kept seam A in `tddy-tools` **because** of this protocol, not in spite of it: relocating it
into a library would spread a protocol the codebase is retiring rather than retire it (see the
changeset's `## Decisions & Trade-offs`). The engine itself is a coherent ~250 lines with zero
`tddy-*` dependencies; it is the wire under it that is legacy.

The work: re-express the relay on `tddy-rpc` framing, as `toolcall_client` already did, and drop
the hand-rolled NDJSON reader/writer. `TDDY_SOCKET` is read at ten sites across
`tddy-tools`' `cli.rs`/`server.rs` and `tddy-bsp`'s `build_cli.rs`, so the listener side has to be
migrated in the same change or serve both framings during it.

Deferred because it is a wire-format change, and node 5's claim is that no observable behaviour
changed.
