# oversized-file: attach.rs — attaching a session mirror to a daemon

**Location:** `packages/tddy-session-sync/src/attach.rs`
**Category:** oversized-file
**Detected:** 2026-09-24 by the `/pr-wrap` file-length gate on #510 (`#keyring` 3/9)
**Metrics:** **555 production lines** (2026-10-19, #629 HEAD; 536 on #628; 520 on #510 HEAD, 517 on `origin/master` `35cf2913`) — the file has no `#[cfg(test)]` module, so every line counts · budget 500 · ~1.11× over
**Thresholds breached:** length 520 > 500
**Restructure:** required — one module seam
**Status:** Open — regressed 2026-10-17 (+16 from #623, `#satellite` 1/13; deferred with the developer's consent) and again 2026-10-19 (+19 from #629, `#satellite` 7/13: the token and connect code left, the connector call, its error mapping and `join_session_room` arrived; **deferred with the developer's consent, 2026-10-19 — `docs/dev/todo/2026-10-19-satellite-7-client-transports-left-files-over-the-500-line-budget.md`**). Already over budget on `origin/master`; #510 adds three lines. Split deferred with the developer's consent during #510's wrap, to a follow-up after the `#keyring` stack lands — `docs/dev/todo/2026-09-24-keyring-store-deferred-oversized-file-splits.md`

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-24 | 517 → 520 | `origin/master` `35cf2913` → #510 HEAD: `+3`, `RefreshSessionRequest.vault_unlock_key: String::new()` and its comment in `DaemonHttp` — a tool is no browser session lineage and presents no unlock key |
| 2026-10-17 | 520 → 536 (+16) | `#satellite` 1/13 (#623): `open_session_room` and `DaemonHttp::open_session_room` replace the `ConnectSession` call, with the comments saying why. Grown; split deferred with the developer's consent — `docs/dev/todo/2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md` |
| 2026-10-19 | 536 → 555 (+19) | `#satellite` 7/13 (#629): the LiveKit token mint and `connect_client` leave `attach`; `join_session_room` (the room request, the `RoomConnectError` → `AttachError` mapping) and the token-lifetime constant arrive, with their doc comments. `AttachError::Token` is gone. The file stays over budget and the `DaemonHttp` seam below is still the cure. Split deferred with the developer's consent (2026-10-19) — `docs/dev/todo/2026-10-19-satellite-7-client-transports-left-files-over-the-500-line-budget.md` |

## Why it is not split in #510

The growth is one struct field #510's additive proto change requires. Decomposing a file that was
already over budget, for three lines, would put an unrelated move in a credential-store diff.

## What would close it

Move the `DaemonHttp` client (the daemon RPC calls the mirror makes) into a module of its own with
the `code-restructuring` skill; that alone takes the file well under 500.
