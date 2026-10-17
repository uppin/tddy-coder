# LiveKit-free modules leave the LiveKit plugin - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 5/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Remote managed worktree](../remote-managed-worktree.md). Codebase
  placement classification is unchanged, but it moves out of the LiveKit plugin crate.
- **Related Feature**: [GitHub identity](../github-identity.md). A session's git environment is
  unchanged, but it moves likewise.

## Summary

Three modules that contain no LiveKit code move out of `tddy-daemon-livekit`: `placement` (codebase
placement classification), `session_git` (a session's git identity environment) and
`os_user_resolution` (session token to OS user). Code that reaches the daemon's instance id through
the plugin's re-export (`livekit_peer_discovery::local_instance_id_for_config`) imports it from the
kernel, which defines it. Two crates drop a `livekit = "0.7"` dependency they never use. After this,
everything left in the plugin is LiveKit work, and a core crate that still depends on the plugin
does so for a LiveKit reason that a later node can name.

## Background

The whole-work discovery found `tddy-agent-launch` and `tddy-session-lifecycle` importing the plugin
for these modules alone at several call sites. It also found 24 files taking the kernel's identity
helper through a plugin re-export, and `livekit` edges with no `livekit::` use in `tddy-daemon-rpc`
or `tddy-session-lifecycle`. While those edges stand, the ratchet's offender list overstates what
each later node has to move.

## Proposed Changes

### What's Changing
- `placement` → `tddy-agent-launch::placement` (its only consumer).
- `session_git` → `tddy-agent-launch::session_git` (its only production consumer). The acceptance
  suite `session_git_identity_acceptance.rs` moves with it.
- `os_user_resolution` → `tddy-daemon-kernel::os_user_resolution`, next to `SessionUserResolver`.
- `tddy-session-lifecycle`'s re-exports (`connection_service::placement`, `os_user_resolution`) point
  at the new homes.
- 24 imports of `local_instance_id_for_config` / `local_base_instance_id_for_config` through
  `livekit_peer_discovery` → `tddy_daemon_kernel::daemon_identity`.
- `livekit = "0.7"` removed from `tddy-daemon-rpc` and `tddy-session-lifecycle`.

### What's Staying the Same
- Behaviour: no logic changes. These are moves and import rewrites.
- Ratchet entries. Every crate touched still names `tddy-daemon-livekit` for a LiveKit reason, which
  later nodes resolve.

## Impact Analysis
- **Technical:** mechanical. Moves go through the restructure engine (`tddy-tools restructure`), and
  the engine's refusal of any move is put to the developer before anything is moved by hand.
- **User:** none.

## Implementation Plan
1. Move the three modules with the engine, each with its own tests.
2. Repoint the identity imports and re-exports.
3. Drop the two dead SDK edges.

## Success Criteria
- `tddy-daemon-livekit` declares none of the three modules.
- `tddy-daemon-rpc` and `tddy-session-lifecycle` do not name `livekit`.
- The moved suites pass from their new homes.

## Out of Scope
- Any module that does use LiveKit (later nodes).
