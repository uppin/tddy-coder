# oversized-file: listener.rs

**Location:** `packages/tddy-toolcall/src/toolcall/listener.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **879 production lines** (2026-10-26; 671 at detection) · budget 500 · **1.76× over**
**Thresholds breached:** length 879 > 500
**Restructure:** not designed
**Status:** Open — regressed 2026-10-26 (766 → 879 in #758, `#start-parity` 9/9, the universal-start verbs; deferred by developer instruction) — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/toolcall/listener.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 671 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: no line of the file changed |
| 2026-09-24 | 676 | 671 on the merge-base with `origin/master` (`4e7157d2`) → 676 after #509 (`#keyring` 2/9): the `RequestTransport::UnixSocket` stamp at the listener's `from_duplex` call, wrapped by rustfmt. Grown; decomposition deferred with the developer's consent (`docs/dev/todo/2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`). First `#[cfg(test)]` now at L677 |
| 2026-10-08 | 766 | 676 on `origin/master` → 766 after `#keyring` 9/9 (#516); split deferred with the developer's consent — see the backlog entry "`#keyring` 9/9 left files over the 500-production-line budget" (2026-10-08): +90, the `github-token` verb's request and response dispatch in `impl ToolcallRpcService` (seam B above). Count: production lines to the first `#[cfg(test)]` |
| 2026-10-26 | 879 | **regressed.** 766 at the base of #758 (`#start-parity` 9/9) → 879 (+113): the three universal-start methods on `ConversationSpawnHandler` (default bodies refusing), `json_answer`, and `impl ToolcallRpcService`'s `StartConversation` / `ConversationStartStatus` / `ConversationStartCancel` dispatch and handlers (seam B). Split deferred by developer instruction (code issues recorded, not solved, in the `#start-parity` stack) |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | listener start-up: the spawn-handler traits, the toolcall log, both `start_toolcall_listener` variants, `start_toolcall_listener_with_conversation_handler`, `accept_loop` (L23–205) | ~185 |
| B | `impl ToolcallRpcService`, split by RPC family: submit/ask/approve, actions, build (L206–627) | ~420 |
| C | `handle_build_request` (L628–671) | ~45 |

Seam A alone brings it to about 490 lines. Prove the seams with `restructure check --deep` before applying them.
