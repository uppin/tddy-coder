# oversized-file: session_room.rs

**Location:** `packages/tddy-daemon-livekit/src/session_room.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` file-length gate on #508 (`#keyring` 1/9)
**Metrics:** **2,856 production lines** (3,011 total, first `#[cfg(test)]` at `:2857`) · budget 500 · **5.7× over**
**Thresholds breached:** length 2856 > 500
**Restructure:** required — `extract_module --to_file` seams, designed below, not applied
**Status:** Open — **unclaimed** · pre-existing; #508 rewrote comments only (net 0 lines)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 2,856 | master 2,856 → 2,856 after #508 — six comment lines re-pointed from the shared secret to the per-daemon key; split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |

## What the gate found

Production lines are counted before the first `#[cfg(test)]`:

```
$ grep -n "^#\[cfg(test)\]" packages/tddy-daemon-livekit/src/session_room.rs | head -1
2857:#[cfg(test)]
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
