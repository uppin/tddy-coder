# Terminal resync at any point - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#term-snapshot 2/4` (branch `feature/term-snapshot/resync`)

## Affected Features

- **Primary**: [Terminal sessions (daemon)](../terminal-sessions.md) - `request_snapshot` on any bidi message; a lagged subscriber is resynced
- **Related**: [Web terminal](../../web/web-terminal.md) - the browser terminal detects a gap in the stream and resyncs

## Summary

On an open bidi terminal stream a client may ask for the current screen **at any time** and receive a fresh snapshot at a new sequence point, followed by live output from there. The server resyncs a client it could not keep up with, and the browser resyncs itself when it sees a gap.

## Background

`#term-snapshot 1/4` gives every *open* a snapshot. A long-lived stream can still diverge. The server's broadcast to a slow subscriber lags and drops bytes silently today, and a client cannot tell or recover without reconnecting.

## Proposed Changes

### What's Changing
- `SessionTerminalInput.request_snapshot`, valid on any message of `StreamSessionTerminalIO`: input bytes on the same message are applied first, then the server emits a snapshot frame at a new sequence point and continues live from its `end_offset`.
- A subscriber whose live broadcast lags gets a fresh snapshot frame instead of a hole.
- The web terminal checks offset continuity. A frame that does not start where the previous ended is not painted, and the feed sends `request_snapshot` on the same stream.

### What's Staying the Same
- Opening with `SNAPSHOT` (`#term-snapshot 1/4`), history, ACKs and the control lease.

## Impact Analysis
- Server: the bidi output loop serializes resync requests against live output. Client: a continuity guard in the host feed.
- Users: a terminal that drifted (slow link, dropped bytes) repairs itself without a reload.

## Acceptance Criteria
- [ ] A mid-stream `request_snapshot` yields a snapshot frame that renders identically to the server's screen, then only later deltas, with contiguous offsets.
- [ ] A lagged subscriber receives a snapshot rather than a gap, and ends identical to the server's screen.
- [ ] The web terminal does not paint a non-contiguous frame and requests a snapshot on the same stream.
