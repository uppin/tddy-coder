# 2026-11-28 work-item-files — initial discovery

Companion to `2026-11-28-work-item-files.md`. The Explore / Discovery dump; deleted at wrap.

## How the pieces fit today (State A, verified by reading the code)

### Queue model and persistence
- `WorkItem` / `WorkQueue` live in `packages/tddy-changeset/src/changeset/work_queue.rs`
  (re-exported by `tddy-core`). Fields are serde with `default` + `skip_serializing_if`
  (`labels`, `badges`, `needs` are the pattern to copy).
- The queue persists at `<sessions>/<owner_session_id>/work-queue.yaml`
  (`packages/tddy-changeset/src/session_work_queue.rs`, `WORK_QUEUE_FILE`), written under a
  per-record lock with atomic rename; `WorkQueueStore` (`work_queue_store.rs`) is the
  `(project_id, queue_id)` view. A queue is created only at session start.
- Every write through `SessionWorkQueues::update` bumps `revision` iff the record changed;
  `WatchWorkQueue` is filesystem-watch based (`tddy-daemon-rpc/src/work_queue/queue_changes.rs`,
  `queue_watch.rs`) — a new mutation needs **no** notification wiring.

### RPC plumbing (pattern to copy: `SetWorkItemLabels`)
1. proto `work_queue.proto` → 2. `WorkQueueHandler` trait (`tddy-work-queue/src/rpc.rs`) →
3. generated service impl (`tddy-work-queue/src/rpc/service.rs`) → 4. daemon handler
   (`tddy-daemon-rpc/src/work_queue/ports.rs`) → 5. `*_rpc` body following
   `annotations.rs` (`forward_addressed` first, then `edit_queue` → `store.update` →
   `work_queue_json`). Forwarding: `forward_if_addressed` / `forward_addressed`
   (`work_queue/project_stacks.rs:204,273`) over `peer_routing.rpc_served_by_peer`.

### Deletion hook point
- `packages/tddy-work-queue/src/item_ops/nodes.rs:194-240` `delete_work_item` — pure model op
  (reparenting, open-PR refusal, cycle check). The daemon side that runs it
  (`tddy-daemon-rpc/src/exec_tool/work_queue_tools/ops.rs:161`, tool `work_item_delete`) is where
  filesystem cleanup of an item's files would join.

### Spawn paths and their convergence
- **Agent tool**: `work_item_spawn_session` →
  `tddy-daemon-rpc/src/exec_tool/work_item_git_tools/item_git_legs.rs:150-250` `spawn_child` builds
  `StartSessionArgs { work_queue_parent, work_item_id, new_branch_name, initial_prompt = title +
  description, … }` → `self.home.start_session(...)`.
- **Web**: queue row "Start session" → `startSessionInitialValues.ts` (workQueueParent, workItemId,
  branch intent, prompt — **no attachments today**) → `CreateSessionDialog`
  (`tddy-session-create-web`) → `StreamStartSession`.
- **Convergence**: both arrive as a `StartSessionRequest`; `work_item_id` is field 37
  (`session.proto:844`), meaningful only with `work_queue_parent = 15`.
- `StartSessionArgs` already carries `attachments: Vec<HostDocumentAttachment>` (host-document only;
  `start_session_args.rs:62,76-96,245-307`) and maps them into `SessionAttachment` field 29.
- **Materialization**: `tddy-agent-launch` `prepare_session_attachments` —
  `svc_start_session_core.rs:325-330` and `tool_session_spawn.rs:84-95` — engine in
  `tddy-session-files/src/session_attachment_materialization.rs` (duplicate-basename refusal,
  rollback on failure). `work_item_id` is *not* consulted there today.
- `work_queue_parent.rs` (`WorkItemBaseLookup`, D34) resolves the parent/item at start — the natural
  injection seam for item files; placement to be confirmed during implementation (both the plain and
  the sandboxed harness start paths must be covered).

### The reference pattern: stack_doc_attachments
- `tddy-session-files/src/stack_doc_attachments.rs` builds the four documents a planned-PR child is
  spawned with as `HostDocumentRef { daemon_instance_id: owner, scope: SESSION_ARTIFACT,
  session_id: owner, relative_path: "prs/<node_id>/<basename>" }` — nested source, flat destination,
  missing document skipped not fatal. Cross-host fetch already exists:
  `materialize_host_document_attachment` streams from the peer when the ref names another host
  (`svc_materialize_staged_attachment.rs:128-220`), bounded by `max_attachment_bytes`.
- History note: the 2026-08-29 changeset says "both spawn paths use it", but `StackChildSpawnHandler`
  was removed 2026-11-22 (`docs/dev/changesets/2026-11-22-remove-pr-stack-recipe.md`); today the
  injection lives client-side (the dialog test `stack_child_spawn_tests.rs:253-280` simulates the
  dialog building them). Our feature deliberately moves the equivalent rule **below** the surfaces —
  into the daemon start path — so it cannot regress per surface.

### Host-document scopes (where the new scope joins)
- `tddy-service/proto/types.proto` `HostDocumentScope`; resolved in
  `tddy-session-files/src/host_documents.rs` (`resolve_host_document`: canonicalize-and-contain the
  full path, `MAX_HOST_DOCUMENT_BYTES` 4 MiB unary cap, scope-specific path shapes —
  `SESSION_UPLOAD`/`STAGED_ATTACHMENT` are the two-segment precedents).
- Staging: chunked `UploadStagedAttachmentChunk` (`session_files.proto:63`), `.staged-complete`
  marker checked by the scope resolver; staging root is **restart-cleared, not durable** — hence the
  new durable store instead of reusing staging for storage.

### Web queue view
- `WorkItem` TS type + `parseWorkQueue` in `packages/tddy-workqueue-web/src/workQueue.ts` (wire =
  serde snake_case of the Rust model).
- Row: `WorkItemRow.tsx` header flex row already hosts `WorkItemRowBadges`, `WorkItemCommitCounts`,
  `WorkItemPrBadge`; expandable `WorkItemRowDetails.tsx` renders description/branch/labels/links —
  the attachment list's natural home. Icons are `lucide-react`.
- Cypress specs are component specs over `anInMemoryRpcBackend()` with typed stubs
  (`WorkQueueViewAcceptance.cy.tsx`, fixtures in `cypress/support/rpc/prStackFixtures.ts`).

### Generated TS proto
- Two committed copies regenerated via `scripts/generated-code.sh write packages/tddy-client-web
  packages/tddy-rust-typescript-tests` (manifest: `scripts/generated-code.manifest`); CI has a
  drift gate. `migrate_proto_connection*.py` are unrelated one-off import rewrites.

### Tool registration checklist (for the three tools)
- defs + name consts: `tddy-tool-engine/src/work_queue_tools.rs` (or a new sibling file);
- classification: `tddy-tool-engine/src/read_only.rs` (`is_stack_tool` covers them once registered —
  attach/detach are **mutations**, list is a read);
- daemon offer/admission: `tddy-tools/src/exec_tool_admission.rs` expected list;
- catalogue + vocabulary tests: `tddy-tool-engine/tests/work_queue_tools_catalogue.rs`
  (`EVERY_PLAN_TOOL`), `tests/work_queue_vocabulary.rs` (`EVERY_WORK_QUEUE_TOOL`);
- serving: `tddy-daemon-rpc/src/exec_tool/work_queue_tools.rs` routing from `project_home.rs:135`.
