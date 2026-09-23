# LiveKit common-room peer discovery and host selection

**Status:** Current  
**Product area:** Daemon, Web (Connection screen)

## Summary

When **`livekit.enabled`** is `true` and **`livekit.common_room`** is set together with valid LiveKit URL, API key, and API secret, each **`tddy-daemon`** joins that shared room as a participant, publishes a small JSON advertisement on local participant metadata (`instance_id`, `label`, and the key id and public key it signs session tokens with), and observes other participants. **ListEligibleDaemons** returns exactly one row with **`is_local: true`** for the answering daemon plus one row per discovered peer (**`is_local: false`**), ordered with the local row first and stable ordering among peers. **StartSession** accepts a **`daemon_instance_id`** that matches the local instance (local spawn) or a currently listed peer (request forwarded over the LiveKit data-channel **tddy-rpc** bridge to that peer’s **ConnectionService**). Unknown or stale ids yield a clear gRPC error (**`FAILED_PRECONDITION`** or related); there is no silent fallback to the local host.

## Configuration

| YAML / setting | Role |
|----------------|------|
| `livekit.enabled` | Whether this daemon joins the common room at all. **Defaults to `false`**, so discovery is opt-in: a complete block with no `enabled: true` names a room the daemon stays out of. Switching it off preserves the credentials below, so it is not the same as deleting them. |
| `livekit.url`, `livekit.api_key`, `livekit.api_secret` | LiveKit project access; required for discovery and forwarding. |
| `livekit.common_room` | Non-empty room name shared by all daemons that should see each other. When unset or blank, the daemon lists only the local eligible row and does not join a discovery room. |
| `daemon_instance_id` | Optional stable id for this process; default derives from the hostname. Must be distinct per physical daemon when multiple hosts share a room. |

Spawned sessions continue to use **`livekit.common_room`** for collaboration when configured; per-session LiveKit identities follow existing **`livekit_server_identity_for_session`** rules.

## Trust model

Membership in the configured LiveKit room (same project credentials and **`common_room` name) defines the peer group. Any participant that can join may appear in **ListEligibleDaemons** and receive a forwarded **StartSession** carrying the full RPC body, including **`session_token`**. Operators rely on a private LiveKit project, restricted network access, and trusted hosts—there is no separate cryptographic attestation that a participant runs **`tddy-daemon`**. For **session tokens** specifically there is one: a forwarded token is verified against the public key its signing daemon advertises, so a peer cannot present a token another daemon did not sign.

**Who may be taken for a daemon.** A peer's advertisement is self-declared metadata, and it carries the public key that peer signs session tokens with — every daemon verifies the peer's tokens against it. So discovery reads an advertisement only from an identity **no client-facing mint hands out**: browser (`web-…`, `browser-…`), coder/session (`server…`, `daemon-…`), split-agent (`split-agent-…`), remote-git (`remote-git-…`) and screen-share bridge (`screenshare-host-…`) participants are never daemons, whatever they publish, and `token.TokenService` refuses to mint any identity outside those prefixes. Both sides read one rule (`tddy_service::may_be_daemon_discovery_identity`), so an advertised key is only ever believed from an identity a daemon minted for itself — a signed-in web user cannot join the common room under a bare id, advertise a keypair of its own and forge tokens for another login. A participant holding the LiveKit API secret can still join under any identity; that credential stays with operators and the processes a daemon spawns.

**Key ids are content-addressed.** When several participants advertise one key id, the verifier keeps the one whose key hashes to it, so a re-advertised id cannot shadow the genuine key; and a key once learned is remembered across a reconnect (an id names exactly one key, forever), so peers' tokens keep verifying while the roster is momentarily empty. An advertisement whose key does not hash to the id it is advertised under is refused, never cached.

**Revocation costs a restart.** A learned key is never evicted for the life of the verifying daemon. A peer that leaves the room, a host an operator removes, or a key known to be compromised keeps having **newly minted** tokens accepted by every daemon that once learned its key, until each of those daemons restarts. There is no expiry on learned keys and no revocation list; both are candidates for a later `#keyring` node.

## Eligible daemon rows

- **Local row:** **`instance_id`** from config/default, **`label`** identifies this daemon, **`is_local: true`**.
- **Remote rows:** Parsed from peer metadata JSON; a participant with no valid advertisement is not a peer (there is no identity fallback), and one under a non-daemon identity prefix is not a peer even with one.
- **Duplicates:** The list contains at most one row per **`instance_id`**; the local id is never duplicated as a remote row.
- **Disconnects:** The registry refreshes on participant events and on a short periodic resync so disconnected peers drop out within a bounded window after LiveKit signals leave.

## StartSession routing

- Empty **`daemon_instance_id`** or a value equal to this daemon’s instance id → local spawn (existing path).
- Value matching a remote eligible **`instance_id`** → unary **StartSession** over **tddy-rpc** to that peer identity; the response (**`livekit_room`**, **`livekit_url`**, **`livekit_server_identity`**, etc.) reflects the peer’s session.
- Value not in the current eligible set → error with an actionable message (not **`UNIMPLEMENTED`** for “unknown peer” semantics).

**ResumeSession**, **ConnectSession**, **DeleteSession**, and **SignalSession** remain owned by the daemon that holds the session; cross-daemon misuse continues to fail with explicit ownership errors.

## Web client

After sign-in, **ConnectionScreen** loads **ListEligibleDaemons** with tools and projects. The Host dropdown lists eligible rows with the **local** daemon first, then peers sorted by **`instance_id`**. The selected **`daemon_instance_id`** is sent on **StartSession**.

## Operator and CI notes

- **`TDDY_PROJECTS_DIR`:** test-only override for **`projects_path_for_user`**; see **`packages/tddy-session-lifecycle/src/user_sessions_path.rs`**. Avoid setting it globally across unrelated suites; tests such as **`multi_host_acceptance`** save/restore the prior value and use **`#[serial]`** where they share LiveKit.
- **LiveKit:** use the Docker testkit or set **`LIVEKIT_TESTKIT_WS_URL`** where documented; acceptance tests that share a room use **`#[serial]`** or equivalent isolation. Rust module overview: **`packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs`** (top-level **`//!`** section), and the crate page **[`livekit-service.md`](../../../packages/tddy-daemon-livekit/docs/livekit-service.md)**.

## Related documentation

- [The identity boundary and the LiveKit service](auth-livekit-services.md) — the crate this subsystem lives in, and what it may not reach
- [Codex OAuth relay — operator loopback tunnel](codex-oauth-relay.md#operator-loopback-tunnel-tddy-daemon)
- [Web terminal — eligible daemons and host selection](../web/web-terminal.md#eligible-daemons-and-host-selection)
- [Project concept — `host_repo_paths`](project-concept.md)
- [Daemon changelog](changelog/)
- [Web changelog](../web/changelog/)
