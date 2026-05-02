# LiveKit common room: owned project count

## Purpose

Operators viewing the **Connected participants** table in **tddy-web** (shared LiveKit room **`livekit.common_room`**) see how many projects each **server-class** participant has registered in the same **`projects.yaml`** layout **tddy-daemon** uses for the effective OS user, without opening disk or daemon RPC.

## Participant metadata

Server-side LiveKit participants publish JSON **local participant metadata** that includes:

- **`owned_project_count`**: non-negative integer — row count for **`projects.yaml`** under the configured registry directory (same file shape as **`tddy_daemon::project_storage`**).

Participants that omit the field (older agents) are indistinguishable from “no count” in the UI: the **Projects** column shows an em dash (**—**).

Metadata updates are **shallow-merged** at the top level so **Codex OAuth** hints (`codex_oauth`) and **`owned_project_count`** coexist in a single JSON document on each **`set_metadata`** call.

## Web dashboard

**`ParticipantList`** renders a **Projects** column with **`data-testid`** pattern **`participant-owned-project-count-{identity}`** (identity segments sanitized for test ids). **`useRoomParticipants`** maps LiveKit **`Participant.metadata`** into an optional **`ownedProjectCount`** and listens for **`ParticipantMetadataChanged`** so counts refresh with the same event stream as other presence fields.

## Coder and LiveKit transport

**`tddy-livekit`** exposes **`LiveKitParticipant::connect(..., projects_registry_dir: Option<PathBuf>)`**. When **`Some(dir)`** is supplied, a background task re-reads the registry on a **30-second** interval and republishes the merged metadata. **`spawn_local_participant_metadata_watcher`** shares a **`metadata_publish_lock`** with those publishers so concurrent updates do not overwrite unrelated keys.

Callers that pass **`None`** for **`projects_registry_dir`** do not publish **`owned_project_count`**.

## Related documentation

- **[Web terminal / common room](web-terminal.md#shared-livekit-room-livekitcommon_room)** — where the presence table appears
- **[`participant-metadata.md`](../../../packages/tddy-livekit/docs/participant-metadata.md)** — **`tddy-livekit`** technical reference
- **[Daemon project concept](../daemon/project-concept.md)** — per-user project registry
