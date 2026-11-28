# Changeset: Work item files — attachments a work-queue item carries into its sessions

**Created:** 2026-11-28
**Status:** In Progress
**Type:** Feature
**PRD:** docs/ft/coder/work-item-files.md

## Affected Packages
- [ ] `tddy-service` — proto: `HostDocumentScope::WorkQueueFile` (types.proto), `AttachWorkItemFile` / `DetachWorkItemFile` + messages (work_queue.proto); regenerated TS gen
- [ ] `tddy-changeset` — `WorkItem.files: Vec<WorkItemFile>`; the durable per-item file store beside `session_work_queue.rs`
- [ ] `tddy-work-queue` — pure item ops (attach/detach entries); `WorkQueueHandler` trait + generated service impl for the two RPCs
- [ ] `tddy-daemon-rpc` — daemon handler bodies (forward-first, then edit/copy), delete cleanup, tool serving
- [ ] `tddy-tool-engine` — `work_item_attach_file` / `work_item_detach_file` / `work_item_list_files` tool defs, arg schema, read-only/admission classification
- [ ] `tddy-session-files` — `WorkQueueFile` scope in `host_documents.rs` resolution + guards
- [ ] `tddy-agent-launch` — start-path injection of an item's files into `req.attachments`
- [ ] `tddy-client-web` + `tddy-rust-typescript-tests` — regenerated gen (no hand edits)
- [ ] `tddy-workqueue-web` — parse `files`, attachment icon on the row, list in the details, Cypress spec

## Related Feature Documentation
- [Work item files](../../ft/coder/work-item-files.md) — the PRD
- [PR stacking](../../ft/coder/pr-stacking.md) — the work-queue feature this extends
- [Session attachments](../../ft/coder/session-attachments.md) — the materialization contract reused

## State A (Current)

A work item carries only text: `title`, `description`, `branch_suggestion`, labels, badges,
linked sessions (`packages/tddy-changeset/src/changeset/work_queue.rs`). There is no way to attach
an input document to an item; an orchestrator quotes it into the description.

`StartSessionRequest.attachments` (field 29) already materializes per-session attachments into
`artifacts/attachments/` before the agent starts — staged uploads and host documents, cross-host by
reference — but nothing populates it from a work item: `work_item_spawn_session`
(`item_git_legs.rs:198-209`) and the web Start-session dialog
(`startSessionInitialValues.ts`) both send `work_item_id` with an empty attachment list.
`work_queue_parent.rs` resolves the item at start (D34) but reads nothing else from it.

Host-document scopes (`tddy-session-files/src/host_documents.rs`) are
`SESSION_ARTIFACT`, `SESSION_UPLOAD`, `SESSION_WORKTREE`, `PROJECT_REPO`, `STAGED_ATTACHMENT` —
none rooted at a work queue's bytes.

## State B (Target)

- A work item carries `files: Vec<WorkItemFile>` (`basename`, `size_bytes`), part of the queue
  model — so every `WatchWorkQueue` snapshot and `work_queue_json` answer carries the list, and the
  web needs no extra RPC to render it.
- The bytes live on the queue's owner host under
  `<owner session dir>/work-queue-files/<item_id>/<basename>` — durable (unlike the restart-cleared
  staging root), one flat level per item.
- A new `HostDocumentScope::WorkQueueFile` resolves that root with the two-segment path rule
  (`<item_id>/<basename>`, the `SESSION_UPLOAD` shape) and the shared canonicalize-and-contain gate.
- `AttachWorkItemFile` (source oneof: staged ref / caller's own session artifact / project-repo
  file) and `DetachWorkItemFile` are served by the owner host, forwarded like every addressed
  `WorkQueueService` call; attach promotes a `.staged-complete` staged file (deleting it), refuses
  duplicates, enforces the caps, writes exclusively, and lets the revision answer the watchers.
- `work_item_attach_file` / `work_item_detach_file` / `work_item_list_files` daemon exec tools serve
  the same surface to agents, access following the queue.
- The daemon's start path injects an item's files into the start request's attachments — as
  `HostDocumentRef`s naming the owner host — wherever `work_item_id` resolves against the
  work-queue parent, before materialization. Every spawn surface is covered by construction.
- Deleting a work item removes its files directory.

## Delta

### New
- `types.proto`: `HostDocumentScope::WorkQueueFile`; `work_queue.proto`: `AttachWorkItemFile`,
  `DetachWorkItemFile`, `AttachWorkItemFileRequest` (source oneof), `DetachWorkItemFileRequest`.
- `WorkItemFile { basename, size_bytes }` + `WorkItem.files` (serde default / skip-if-empty).
- File store beside `session_work_queue.rs` in `tddy-changeset` (write guards reusing the shared
  `contained_in_scope_root` / `validate_relative_path` spellings; exclusive create).
- Item ops in `tddy-work-queue/src/item_ops/` (new sibling of `item_annotations.rs`).
- Three daemon exec tools + their admission/catalogue entries.
- Start-path injection in `tddy-agent-launch`.
- Web: `files` in `workQueue.ts`, icon + details list, Cypress spec.

### Modified
- `host_documents.rs`: one more scope arm in `resolve_host_document` and the read paths.
- `delete_work_item`'s daemon caller: removes the item's files directory after a successful delete.
- Tool catalogue/vocabulary/read-only/admission lists gain the three names.

### Removed
- Nothing.

## Prerequisites (deferred-work cross-check — planning Step 2b)

| Item | Verdict | What this change does about it |
|---|---|---|
| `docs/dev/todo/2026-11-22-work-item-git-tools-reach-only-the-queue-owners-host.md` | ⚠ During | Same asymmetry this feature must respect: attach sources are resolvable **on the owner host** (owner's artifacts, project repo, staged upload). Attaching from a peer host's worktree is out of scope and recorded in the PRD's constraints — not silently attempted. |
| `docs/dev/todo/2026-08-01-session-attach-ui-follow-ups.md` (no consumed-batch staging GC) | ⚠ During | This change adds a second consumer that **deletes** the staged file it consumes. It does not build the general GC; the existing TODO stays open for batches a `StartSession` consumed. |
| `docs/dev/todo/2026-08-01-session-attach-ui-follow-ups.md` (fakeCommonRoom drops `max_attachment_bytes`) | — Unrelated | Our web spec stubs the queue service only; the cap is enforced daemon-side. |
| `packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-stream-execute-tool.md` | ⚠ During | `exec_tool/ports.rs` gains routing lines for the three tools. Keep additions to the existing routing arm; do not grow the function's nesting. |
| `packages/tddy-tool-engine/docs/code-issues/complexity-start-session-args-schema-schema-offering.md` | ⚠ During | If (and only if) a start-args field is added, add it as one more `offer(...)` line — the issue already prescribes making the table data; do not deepen the function. Preferred plan adds **no** start-args field (injection is daemon-side). |
| `packages/tddy-work-queue/docs/code-issues/complexity-pull-base-*.md`, `complexity-repoint-*.md` | — Unrelated | Different files. |

No package in scope carries a 🚧 claimed issue on files this change touches; none carries
`Restructure: required`. Packages with no `docs/code-issues/` directory (`tddy-session-files`,
`tddy-workqueue-web`, `tddy-changeset`, `tddy-work-queue` beyond the two above) have not been
analyzed — not the same as clean; noted for wrap.

## Milestones

### Milestone 1: Model and storage — ✅ done (cb99527c2)
- [x] `HostDocumentScope::WorkQueueFile` + proto messages; regenerated gen
- [x] `WorkItem.files` + pure attach/detach ops in `tddy-work-queue/src/item_ops/`
- [x] Durable file store in `tddy-changeset` with the shared path gates, exclusive create, caps
- [x] Unit + store acceptance tests (fluent-tests style)

### Milestone 2: RPC and tools — ✅ done (fc9e8ed6e)
- [x] `WorkQueueHandler` trait methods + generated service impl + daemon bodies (forward-first)
- [x] `AttachWorkItemFile` promotes a staged file; session-artifact and repo-file arms for the tool
- [x] The three tools: defs, arg schema, classification, catalogue/vocabulary/admission tests
- [x] Deletion of an item removes its files directory (daemon side of `delete_work_item`)

### Milestone 3: Spawn delivery — ✅ done (dcd293217)
- [x] Injection below every spawn surface: `start_session_core`, `tool_session_spawn` and the CLI
      harness prelude all append the item's files before materializing anything
- [x] Cross-host fetch through the new scope — the refs name the owner host, and the materializer's
      existing peer fetch (streaming, `max_attachment_bytes`-bounded) delivers them
- [x] Acceptance tests: the helper (5) and the end-to-end `work_item_spawn_session` spawn (red first)
      — a collision refusal and a failed-attachment rollback ride the materializer's own rules, pinned
      by its existing suites

### Milestone 4: Web — ✅ done (751631245)
- [x] `parseWorkQueue` carries `files`; icon on rows with files; list in the expanded details
- [x] Cypress component spec (in-memory backend, fixture builders updated; red-verified, then 3/3)

### Milestone 5: Wrap
- [ ] Package docs updates via the wrap flow; changeset completed and wrapped

## Testing Strategy

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Model ops, store guards, caps | Unit (`tddy-work-queue`, `tddy-changeset`) | Pure logic + one filesystem store, deterministic |
| RPC serving, forwarding, tool serving, delete cleanup | Acceptance, in-process daemon (`tddy-daemon-rpc/tests`) | The contract is the RPC shape and its host routing; the existing suites seed stores and drive handlers directly |
| Spawn delivery incl. cross-host | Acceptance, real spawns (`tddy-session-lifecycle`, `tddy-daemon-rpc`) | Only a real spawn proves bytes materialized before the agent's first turn; mirrors `stack_child_spawn_tests.rs` |
| Scope resolution guards | Unit (`tddy-session-files`) | Pure path rules + fs guards |
| Row icon and list | Cypress component (`tddy-workqueue-web`) | UI contract; in-memory RPC backend |

### Acceptance tests
- [ ] **Unit** (`tddy-work-queue`): attaching records basename + size on the item and refuses an
      11th... the count cap by name; detaching an unknown file is refused; deleting the item's entry
      rides the existing delete op (`item_files_unit.rs`)
- [ ] **Unit** (`tddy-changeset`): the store writes two-segment-only paths, refuses escapes and
      overwrites, persists sizes into `work-queue.yaml` (`work_queue_files_store_acceptance.rs`)
- [ ] **Unit** (`tddy-session-files`): `WorkQueueFile` scope resolves `<item_id>/<basename>` inside
      the owner's root and refuses every other shape (`work_queue_file_scope_unit.rs`)
- [ ] **Acceptance** (`tddy-daemon-rpc/tests/work_item_files_rpc_acceptance.rs`): attach from a
      staged ref served by the owner host answers the queue with the entry and removes the staged
      file; a peer-addressed call is forwarded; detach removes entry + bytes
- [ ] **Acceptance** (`tddy-daemon-rpc/tests/work_item_files_spawn_acceptance.rs`):
      `work_item_spawn_session` starts a child whose `artifacts/attachments/` holds the item's files
      before the agent's first turn; a second host holding the spawned session receives them too
      (in-process peers, as `cross_host_stack_parent_acceptance.rs` does)
- [ ] **Acceptance** (`tddy-agent-launch` tests): a `StartSessionRequest` naming `work_item_id`
      gains the item's files before materialization; a basename colliding with an operator
      attachment refuses the start with the duplicate-basename error and leaves nothing written
- [ ] **Acceptance** (`tddy-daemon-rpc/tests`): deleting an item removes its files directory
- [ ] **Cypress** (`packages/tddy-workqueue-web/cypress/component/WorkItemFilesAcceptance.cy.tsx`):
      a node with files shows the icon (`work-item-attachments-<id>`) and the expanded row lists
      names + sizes; a node without files shows neither

## Technical Debt
- **A two-host spawn E2E** (queue owner on host-a, child spawned on host-b) is not written: the
  delivery reuses the materializer's peer-fetch path, which the staging-forwarding suite already
  pins, and the helper's refs name the owner host by construction — but the composed path is
  unproven end to end. Worth one acceptance test when a two-host start harness is next touched.
- Consumed staged batches at `StartSession` still lack a GC (pre-existing TODO — unchanged here).
- Viewing an item file's **contents** from the queue view (a read-only viewer) — future enhancement.
- Agent-side attach from a peer host's worktree — blocked by the same asymmetry the work-item git
  tools carry; revisit when that TODO's recommended sequencing lands.

## Decisions & Trade-offs
- **Durable store under the owning session dir, not staging.** Staging is restart-cleared by design;
  work items must survive restarts. The staged path is reused only as the upload mechanism.
- **Metadata in the queue model, not a disk scan.** Revision bumps + `WatchWorkQueue` deliver the
  icon and list for free; a scan would silently miss updates and bypass revisioning.
- **Injection in the daemon start path, not per surface.** Today's stack-doc attachment is built by
  each surface (and the docs rule lives daemon-side). Building item files *below* every surface
  removes the class of bug rather than adding a second instance of it. Cost: the web dialog cannot
  remove an item file from one particular start — an item's files are unconditional for its spawns.
- **One attach RPC with a source oneof** rather than separate staged/agent RPCs — one guard chain,
  both surfaces, the tool maps its args onto the same source kinds.
- **No new external dependency.**
- **Scope of delivery: single changeset (`/plan-ft-dev`), five milestones.** The feature could be
  split into a stack (model/storage → RPC+tools → spawn → web), but every later node needs the
  earlier ones' behaviour, so the split would be sequential without independent reviewability gains
  the seam cannot justify. The orchestrator may redirect to `/plan-streak` before implementation.

## Open questions — answered at approval (2026-11-28, operator)
1. **Close vs delete:** **Delete-only** — a merely closed (merged) item keeps its files; only a
   deleted item drops them.
2. **Caps:** **As planned** — 20 files per item; per-file cap = the daemon's `max_attachment_bytes`.
3. **Agent attach sources:** **As planned** — the caller's own session artifacts + project-repo
   files only; staged refs stay the web's arm.
4. **Single changeset:** **Approved** — `/plan-ft-dev`, one PR (draft #1067), five milestones.

## Reporting channel note
`tddy-tools send` to the orchestrator (`01a20f10-4a30-7df1-8c63-d12aec760d68`, host
`LT-R1VXTH2V6H`) was refused by the local daemon (`mantas-udoo-ros`) with *not a session you may
message*. The repo's HEAD routes an orchestrator on another host
(`packages/tddy-daemon-rpc/src/exec_tool/session_messages/remote_recipient.rs`, #1036, 2026-11-25);
the installed `/usr/local/bin/tddy-daemon` (built 2026-11-25 07:34) predates that merge. The plan
was reported through the operator instead; this is environment staleness, not recorded as product
debt.

## Refactoring Needed
### From /ft-dev (acceptance test creation)
- [ ] (none yet)
### From /red (TDD red phase)
- [ ] (none yet)
### From /validate-changes
- [ ] (none yet)
### From /validate-tests
- [ ] (none yet)
### From /validate-prod-ready
- [ ] (none yet)
### From /analyze-clean-code
- [ ] (none yet)

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |

## Implementation notes (2026-11-28)
- One regression was caught by the scoped suites and fixed in `f1331bcf2`: the injection consulted
  the daemon's work-queue handler for **every** start, refusing every ordinary start on a host
  without the work-queue RPC families. A start naming no parent/item is now returned untouched.
- Scoped gates run (packages touched only): `tddy-changeset`/`tddy-work-queue`/`tddy-session-files`/
  `tddy-workflow` 619 pass · `tddy-tool-engine`, `tddy-tools`, `tddy-agent-launch`,
  `tddy-daemon-rpc`, `tddy-session-lifecycle` 3,176 pass / 26 fail — of the 26, nine tool-catalogue
  pins and the five `session_sync_livekit` tests (a missing `tddy-remote-git-repo` binary after a
  build cache clear) are fixed and green; the remaining nine are the sandboxed-start suites refusing
  on this host's missing cgroup v2 delegation ("Permission denied"), a host capability the suites
  document, not this change's code.
- Cypress: the new spec 3/3 under xvfb; workqueue-web unit tests 168 pass; both touched web packages
  typecheck clean. Clippy `-D warnings` clean on every touched package.
- Environment notes: `/mnt/data` filled twice during verification (a 107G delete by the operator,
  then this worktree's own debug output); `target/debug` was cleared once as a consequence. The
  cross-host `tddy-tools send` refusal is the installed daemon predating #1036.
