# The VM testkit guest's `livekit:` block is stale

**Filed:** 2026-09-23 by `#keyring` 1/9 (PR #508)
**Status:** open — marked `TODO(keyring)` at `packages/tddy-vm-testkit/src/test_host_vm.rs`

`SESSION_TOKEN_SECRET` is written into the guest daemon's `livekit:` block. Until `#keyring` 1/9
that value was also the session-token signing key, so the block existed for the guest daemon to
authenticate at all. It no longer is: the guest signs with its own Ed25519 key, and the host
cannot mint tokens against this value. The block is now a LiveKit room credential for a guest
that never uses LiveKit, under a name that describes its former job.

## Why it was left

The VM-backed suites (`./vm-tests`, all `#[ignore]`d) need `TDDY_CLOUDINIT_BASE_IMAGE` and a baked
image cache; they could not be run in the change that made the block stale, so removing it would
be unverified.

## What would close it

A `./vm-tests` run with the block removed (or the constant renamed to what it now is). Green →
drop the block and the `TODO(keyring)` marker; red → rename it and record which suite needs it.
