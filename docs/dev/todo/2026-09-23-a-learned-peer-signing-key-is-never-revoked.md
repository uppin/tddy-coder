# A learned peer signing key is never revoked

**Filed:** 2026-09-23 by `#keyring` 1/9 (PR #508)
**Status:** open — accepted trade-off for 1/9

`CommonRoomKeyDirectory` (`packages/tddy-daemon/src/common_room_key_directory.rs`) caches every
peer signing key it has verified against its key id, and that cache deliberately survives the
discovery registry's `clear()` on reconnect — otherwise every peer token (including 24-hour
split-agent tokens) would be refused for the length of each reconnect. The entry is safe against
forgery (a key is cached only after it hashes to its id), but it is **never evicted** for the life
of the verifying process.

So a daemon that left the room, a host an operator removed, or a key known to be compromised keeps
having **newly minted** tokens accepted by every daemon that once learned its key, until each of
those daemons restarts.

## Why this is acceptable for now

Under `v1` the equivalent was worse or equal: revoking meant rotating the fleet-wide
`livekit.api_secret` and restarting every daemon. Under `v2` a restart of the verifying daemons is
still the revocation path — the same cost — and a compromise is now confined to one daemon's key.

## What would close it

Either, in a later `#keyring` node (6/9 authenticates peers for vault state and is the natural owner):

- expire a learned key a bounded time after its peer was last seen advertising it, re-learning it
  when the peer re-advertises; or
- an operator-controlled revocation list (by key id) consulted before the cache.

A test that a key whose peer is gone and whose grace period elapsed is refused closes this entry.
