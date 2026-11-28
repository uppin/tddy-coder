# Work item files — attachments a work-queue item carries into its sessions

## Overview

A work item in a work queue can carry **files**: a spec the operator drops onto the item, a log the
orchestrator saved beside it, a screenshot of the bug. The queue row shows an attachment icon for an
item that has them, with the list viewable. When a session is started **for the item** — the agent's
`work_item_spawn_session` tool, a `StartSession` whose `work_item_id` names it, or the web "Start
session" button on the row — the item's files are passed as that session's start attachments
(`StartSessionRequest.attachments`), materialized into the child's
`artifacts/attachments/` **before the agent starts**, under the same rules as every other start
attachment: a failed attachment fails the start, and a failure leaves nothing half-written.

## Technical Context

### Problem statement

A work item today carries only text (`title`, `description`, labels, badges). Inputs the item's
work needs — a design note, a failure log, a reference export — have nowhere to live, so the
orchestrator quotes them into the description or links sessions by hand. Attachments already exist
for a *session* (`StartSessionRequest.attachments`, the Start-session dialog); they just cannot be
attached to the *item* that spawns sessions.

### Target consumers

- **Orchestrator agents** — `work_item_attach_file` / `work_item_detach_file` / `work_item_list_files`
  daemon exec tools, alongside the existing work-queue tools.
- **The operator in the web queue view** — attach, detach, and see what an item carries.
- **Every child session the item spawns** — whatever surface starts it, it finds the item's files
  already materialized in its artifacts.

### Integration points

- `WorkQueueService` (`packages/tddy-service/proto/work_queue.proto`) — new attach/detach RPCs.
- The queue model (`WorkItem` in `tddy-changeset`) — the item's file list is queue data, so it rides
  the queue's own revision + `WatchWorkQueue` stream to every view.
- Host documents (`tddy-session-files`) — a new scope is how another host reads the bytes.
- Start-session attachments (`tddy-agent-launch` materialization) — the delivery mechanism.

## Storage

- Bytes live **on the queue's owner host**, under the **owning session's directory**:
  `<sessions>/<owner_session_id>/work-queue-files/<item_id>/<basename>`. One flat level per item, so
  no client-supplied value becomes more than one path segment.
- The durable root is deliberate: the staged-attachment root is restart-cleared and not durable
  (`docs/dev/todo/2026-08-01-session-attach-ui-follow-ups.md`), and a work item must outlive
  restarts. What is **reused** from the staged-attachment path is the upload mechanics — the browser
  stages through the existing chunked `UploadStagedAttachmentChunk`, and the attach call promotes a
  `.staged-complete` staged file into the item's store (deleting the staged file: consumed).
- The item's **file list is queue data**: `WorkItem.files: Vec<WorkItemFile>` (`basename`,
  `size_bytes`) in `work-queue.yaml`, serde-default and skipped when empty like `labels`/`badges`.
  Metadata in the queue — not a disk scan — is what makes the icon appear on every `WatchWorkQueue`
  revision without a new read path.
- Reading the bytes back is a **new `HostDocumentScope::WorkQueueFile`**: root
  `{owning session dir}/work-queue-files/`, `relative_path` exactly `"<item_id>/<basename>"` — the
  two-segment shape `SESSION_UPLOAD` and `STAGED_ATTACHMENT` already validate. The full path is
  canonicalize-and-contained against the canonical root, as every scope is.

## API surface

### `WorkQueueService` (proto)

| RPC | Behaviour |
|---|---|
| `AttachWorkItemFile` | Promotes one source file into the item's store and records it. Served by the queue's owner host (forwarded like every `WorkQueueAddress` call). Source oneof: a staged ref (the web's upload path), the caller's own session artifact, or a file in the project's main repo (the agent tool's paths). Validates the basename as a single path segment, refuses an existing same-named file, enforces the caps below, writes exclusively, then records the entry and lets the queue revision answer the watchers. The consumed staged file is deleted. |
| `DetachWorkItemFile` | Removes one file and its list entry. Unknown file: `NOT_FOUND`. A child session that already materialized its copy keeps it — a detach is a plan change, not a retroactive one. |

`ListWorkItemFiles` is deliberately **not** an RPC: the list rides `work_queue_json` in every
snapshot the queue already serves.

### Daemon exec tools

`work_item_attach_file`, `work_item_detach_file`, `work_item_list_files` — registered with the work
tools, addressed by `queue_id` (+ `daemon_instance_id`, forwarded whole to the holding host), access
following the queue exactly as the other work tools do. The agent's attach sources are the caller's
own session artifacts and project-repo files; the tool maps them onto the same RPC source oneof the
web uses.

### Spawn delivery

One rule, applied **in the daemon's start path** where `work_item_id` is resolved against the
work-queue parent: when the parent names an item, the item's files are appended to the start
request's `attachments` as `HostDocumentRef`s naming the **queue owner's** host and session —
`scope: WorkQueueFile`, `relative_path: "<item_id>/<basename>"`, destination basename flat — before
materialization runs. This covers every surface that starts a session for an item (the web dialog,
`work_item_spawn_session`, any future one) by construction, because it sits below them all: a child
that differs by how it was started is a bug the operator cannot see. Cross-host needs no new
forwarding — the materializer already streams a `HostDocumentRef` from a peer host, and the owner
host resolves the scope under its own session→`os_user` mapping like any other scope.

## Limits

| Cap | Value | Where enforced |
|---|---|---|
| File size | the daemon's `max_attachment_bytes` policy cap (default 64 MiB) | attach (refusal names the cap); again at materialization, via the host-document read |
| Files per item | 20 | attach |

No new external dependency is introduced.

## Cleanup when an item closes

Deleting a work item deletes its files directory on the owner host alongside the queue mutation —
an item gone while its files remain is exactly the leak the staged path taught us to avoid. The
owning session's directory (and with it the files) is removed when the session is deleted, so no
new session-lifetime rule is needed. A merely closed (merged) item keeps its files (decision
recorded in the changeset).

## Acceptance criteria

### Functional requirements

- [ ] An item with files shows the attachment icon on its queue row; an item without shows none.
- [ ] The expanded row lists the item's files with their sizes.
- [ ] `work_item_attach_file` / `work_item_detach_file` / `work_item_list_files` follow the queue's
      access rules and answer with the updated queue.
- [ ] A session started for an item — by tool, by `StartSession(work_item_id)`, or by the web
      "Start session" — finds every item file materialized in its `artifacts/attachments/` before
      its agent's first turn.
- [ ] The queue and the spawned session may live on different hosts; delivery still works.
- [ ] A failed attachment fails the start and leaves no partial attachments (existing rule).
- [ ] Deleting the item removes its files from the owner host.
- [ ] The caps refuse over-limit files with a refusal naming the cap.

## Constraints and limitations

- Attaching from a file that lives only on **another** host (e.g. an item's child worktree on
  host-b, driven by an owner on host-a) is out of scope — the same asymmetry the work-item git tools
  already carry (`docs/dev/todo/2026-11-22-work-item-git-tools-reach-only-the-queue-owners-host.md`).
- No read-only viewer for file *contents* is added; the queue row lists the files. A viewer would
  reuse `ReadHostDocument`/`StreamReadHostDocument` with the new scope and is a separate feature.
- The list of files travels in `work_queue_json`; there is no separate listing RPC.

## Related documentation

- [PR stacking](pr-stacking.md) — the work-queue feature this extends (`#workqueue`)
- [Session attachments](session-attachments.md) — the materialization contract the delivery reuses
- [Host documents and attachments](../../../packages/tddy-session-files/docs/host-documents-and-attachments.md) — scopes, staging, caps
