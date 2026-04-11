# LiveKit common-room peer discovery and host selection

**Status:** Current  
**Product area:** Daemon, Web (Connection screen)

## Summary

When **`livekit.common_room`** is set together with valid LiveKit URL, API key, and API secret, each **`tddy-daemon`** joins that shared room as a participant, publishes a small JSON advertisement on local participant metadata (`instance_id`, `label`), and observes other participants. **ListEligibleDaemons** returns exactly one row with **`is_local: true`** for the answering daemon plus one row per discovered peer (**`is_local: false`**), ordered with the local row first and stable ordering among peers. **StartSession** accepts a **`daemon_instance_id`** that matches the local instance (local spawn) or a currently listed peer (request forwarded over the LiveKit data-channel **tddy-rpc** bridge to that peer’s **ConnectionService**). Unknown or stale ids yield a clear gRPC error (**`FAILED_PRECONDITION`** or related); there is no silent fallback to the local host.

## Configuration

| YAML / setting | Role |
|----------------|------|
| `livekit.url`, `livekit.api_key`, `livekit.api_secret` | LiveKit project access; required for discovery and forwarding. |
| `livekit.common_room` | Non-empty room name shared by all daemons that should see each other. When unset or blank, the daemon lists only the local eligible row and does not join a discovery room. |
| `daemon_instance_id` | Optional stable id for this process; default derives from the hostname. Must be distinct per physical daemon when multiple hosts share a room. |

Spawned sessions continue to use **`livekit.common_room`** for collaboration when configured; per-session LiveKit identities follow existing **`livekit_server_identity_for_session`** rules.

## Trust model

Membership in the configured LiveKit room (same project credentials and **`common_room` name) defines the peer group. Any participant that can join may appear in **ListEligibleDaemons** and receive a forwarded **StartSession** carrying the full RPC body, including **`session_token`**. Operators rely on a private LiveKit project, restricted network access, and trusted hosts—there is no separate cryptographic attestation that a participant runs **`tddy-daemon`**.

## Eligible daemon rows

- **Local row:** **`instance_id`** from config/default, **`label`** identifies this daemon, **`is_local: true`**.
- **Remote rows:** Parsed from peer metadata JSON when present; otherwise **`instance_id`** falls back to the LiveKit participant identity string.
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

- **`TDDY_PROJECTS_DIR`:** test-only override for **`projects_path_for_user`**; see **`packages/tddy-daemon/src/user_sessions_path.rs`**. Avoid setting it globally across unrelated suites; tests such as **`multi_host_acceptance`** save/restore the prior value and use **`#[serial]`** where they share LiveKit.
- **LiveKit:** use the Docker testkit or set **`LIVEKIT_TESTKIT_WS_URL`** where documented; acceptance tests that share a room use **`#[serial]`** or equivalent isolation. Rust module overview: **`packages/tddy-daemon/src/livekit_peer_discovery.rs`** (top-level **`//!`** section).

## Related documentation

- [Web terminal — eligible daemons and host selection](../web/web-terminal.md#eligible-daemons-and-host-selection)
- [Project concept — `host_repo_paths`](project-concept.md)
- [Daemon changelog](changelog.md)
- [Web changelog](../web/changelog.md)
