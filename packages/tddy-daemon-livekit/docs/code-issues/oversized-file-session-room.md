# oversized-file: session_room.rs

**Location:** `packages/tddy-daemon-livekit/src/session_room.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` file-length gate on #508 (`#keyring` 1/9)
**Metrics:** **2,874 production lines** (3,029 total, first `#[cfg(test)]` at `:2875`) · budget 500 · **5.7× over**
**Thresholds breached:** length 2874 > 500
**Restructure:** required — `extract_module --to_file` seams, designed below, not applied
**Status:** Open — regressed 2026-10-17 (+61 from #623, `#satellite` 1/13; deferred with the developer's consent), then narrowed 2026-10-20 by #633 (2,935 → 2,885) and #634 (2,885 → 2,857), still over 500; **unclaimed** · pre-existing; #508 rewrote comments only (net 0 lines)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 2,874 | master 2,874 → 2,874 after #508 (measured on `4e260d7f`, after #520) — six comment lines re-pointed from the shared secret to the per-daemon key; split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |
| 2026-10-17 | 2,874 → 2,935 (+61) | `#satellite` 1/13 (#623): `RoomOpenFailure` and `open_staged` / `ensure_open_staged`, which tag a failed open with the stage that failed so only a server failure reports LiveKit unreachable. The new reach logic itself went in `session_reach.rs`, as planned. Grown; split deferred with the developer's consent — `docs/dev/todo/2026-10-17-satellite-1-left-files-over-the-500-production-line-budget.md` |

| 2026-10-20 | 2,935 → 2,885 (−50) | `#satellite` 11/13 (#633): `WorktreeSnapshot` moves to `tddy-daemon-kernel::worktree_snapshot` and `OpenedSessionRoom` to `tddy-daemon-extensions` (re-exported here); the registry gains the `agents_outbox` field. Smaller, still 5.8× over. Seam A starts at the measurement functions now that `WorktreeSnapshot` is gone |
| 2026-10-20 | 2,885 → 2,857 (−28) | `#satellite` 12/13 (#634): `SessionTokenMinter` and `RemoteSnapshotSource` move to `tddy_daemon_extensions::split_rooms` (re-exported here), and `WorktreeSnapshot` is defined there too (the kernel re-exports it). Smaller, still 5.7× over. Not split here: #633 and #635 in the same stack touch this file |

## What the gate found

Production lines are counted before the first `#[cfg(test)]`:

```
$ grep -n "^#\[cfg(test)\]" packages/tddy-daemon-livekit/src/session_room.rs | head -1
2875:#[cfg(test)]
```

The file holds five concerns that already meet at narrow interfaces, in contiguous runs of
module-level items (line numbers against the 2026-09-23 tree):

| Seam | Items | Approx. lines |
|---|---|---|
| A — worktree measurement | `WorktreeSnapshot` … `delete_wip_ref` (L81–394) | ~314 |
| B — the delta ring | `ActivityDelta` … `SessionDeltaStore` impl (L395–644) | ~250 |
| C — patch parsing | `PATCH_SECTION_HEADER` … `side_name_terminator_trimmed` (L645–930) | ~286 |
| D — git plumbing | `diff_between` … `git_output` (L931–1238) | ~308 |
| E — room hosting | `SESSION_ROOM_TOKEN_TTL` … `SessionRoomRegistry` impl, `create_room`, `join_room` (L1239–1937) | ~700 |

Seams A–D are free functions and plain types, so `extract_module --to_file` can address them;
E is the residue that stays. The three `complexity-session-room-*` records in this directory are
functions inside seams C and E.

⚠ Line numbers must be re-derived with `restructure anchors` before a plan is written — they will
move under the `#keyring` stack.
