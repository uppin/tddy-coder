# Changeset: Session attachment storage and context docs

**PRD**: `docs/ft/coder/session-attachments.md`
**Branch**: `feature/session-attach-docs/attach-store`

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit tests
- [x] `tddy-workflow`: attachments layout helpers
- [x] `tddy-daemon`: `session_attachments` store (copy + list)
- [x] `tddy-daemon`: `ContextDoc` kind + size, attachment rows in `context_docs_for_session`
- [x] `connection.proto`: `SessionContextDocKind`, `kind`, `size_bytes`
- [x] Regenerate `connection_pb.ts` (`bunx buf generate`)
- [x] `session_list_enrichment`: map kind + size into the proto row
- [ ] Changelog + package changeset index entries

## Implementation status

Feature complete; only the wrap-phase changelog and index entries remain.

**Commit**: `fa91f202` — *feat(daemon): session attachment store under artifacts/attachments/*
**PR**: [#353](https://github.com/uppin/tddy-coder/pull/353) (draft, base `master`)

**Verification** (`./dev cargo …`, nix dev shell):

| Command | Result |
|---------|--------|
| `cargo test -p tddy-workflow` | `ok. 9 passed; 0 failed` (includes the 2 attachments-layout tests) |
| `cargo test -p tddy-daemon --lib` | `ok. 386 passed; 0 failed` (14 `session_attachments`, 5 new `session_context_docs`, 4 acceptance tests in `session_list_enrichment`) |
| `cargo test -p tddy-core --lib` | `ok. 284 passed; 0 failed` (re-export touch) |
| `cargo clippy -p tddy-daemon -p tddy-workflow -p tddy-core --all-targets -- -D warnings` | clean |
| `cargo fmt … -- --check` | clean |

`cargo test -p tddy-daemon --lib` needs `cargo build -p tddy-sandbox-runner` first, or the unrelated
`sandbox_session::tests::dial_and_bridge_drives_run_host_relay_over_a_stdio_sandbox_client` fails on a
missing runner binary.

**Where the behavior lives**

| Item | Location |
|------|----------|
| Layout helpers | `packages/tddy-workflow/src/artifact_paths.rs:31` |
| Attachment store | `packages/tddy-daemon/src/session_attachments.rs:32` (list), `:87` (copy) |
| Kind + size on context docs | `packages/tddy-daemon/src/session_context_docs.rs:25` (kind), `:72` (listing) |
| Wire mapping | `packages/tddy-daemon/src/session_list_enrichment.rs:274` (kind map), `:312` (proto row) |
| Proto | `packages/tddy-service/proto/connection.proto:194` |

## Handoff

### What is left in *this* changeset

One item: the wrap-phase docs. `/wrap-context-docs` transfers State B into permanent docs and deletes
this file. Concretely:

- `docs/ft/coder/changelog.md` — a release-note section. Follow
  [changelog merge hygiene](../guides/changelog-merge-hygiene.md); if another section already carries
  today's date, give this one a **distinct title** rather than merging into it.
- Index bullets in `docs/dev/changesets.md` and `packages/tddy-{workflow,core,daemon,service,web}/docs/changesets.md`.
- `docs/dev/changesets.d/` does not exist in this repo yet, so the index stays in `changesets.md` — do
  not create the shard directory for a change this size.
- The PRD (`docs/ft/coder/session-attachments.md`) and the `session-layout.md` section are already
  permanent docs written in present tense; they need **no** transfer, only a re-read to confirm they
  still match the code.

### The client half is greenfield — verified, not assumed

Both facts below were checked by grep at handoff time, and both are easy to assume otherwise:

- **`SessionContextDoc` has no consumer in `tddy-web`** outside `src/gen/connection_pb.ts`. There is
  no Docs tab reading this list yet; the field has been populated since the pr-stack work but nothing
  renders it.
- **`read_session_context_doc_utf8` has no wire caller at all.** No RPC exposes context-doc contents,
  for either kind. Whoever builds the web surface adds that RPC too.

So "surface attachments in the UI" is not a matter of extending an existing view — the reading surface
does not exist on either side.

### Direction for the next changeset

The natural next PR is **materializing `StartSessionRequest.attachments` into the session** before the
agent launches — the item this store was built for:

- `copy_attachment_into_session` is the write primitive; the caller does the per-request policy:
  reject duplicate basenames **within one request** (rather than letting the second copy hit the
  store's `FAILED_PRECONDITION`), and refuse a `StagedAttachmentRef` naming a host other than the one
  `StartSession` runs on — a request error, never a cross-host fetch.
- That wire contract lives on the sibling branch `feature/session-attach-docs/attach-proto`
  (unmerged), in `packages/tddy-daemon/docs/connection-service.md` § *Start-session attachments*. Read
  it from that branch, and mind the path discrepancy in [Known follow-ups](#known-follow-ups-out-of-scope-here).
- For a future **attachment content fetch**, mirror the `ListSessionWorkflowFiles` /
  `ReadSessionWorkflowFile` pair (`connection.proto:32`) rather than inventing a shape — and keep it
  **unary**: the streaming RPCs return `unimplemented` for `PeerRoute::Forward`, so only unary calls
  can be peer-forwarded today. It must be byte-oriented, not UTF-8, since attachments may be images.

### Working in this repo (non-obvious)

- `cargo` lives in the nix dev shell and `nix` is not on `PATH`:
  `export PATH="/nix/var/nix/profiles/default/bin:$PATH"`, then `./dev cargo …`.
- `cargo build -p tddy-sandbox-runner` once, or `cargo test -p tddy-daemon --lib` fails on an
  unrelated sandbox stdio-relay test.
- Regenerating `connection_pb.ts` needs `node_modules` in the worktree: `./dev bun install`, then
  `./dev bun run --filter tddy-web generate`.

## Technical debt

No `TODO` or `FIXME` markers were added. The deferred work is behavioral, not debt-in-code, and is
listed under [Known follow-ups](#known-follow-ups-out-of-scope-here) — chiefly that
`copy_attachment_into_session` has no wire caller yet, attachment bytes are not fetchable, and the
sibling branch's `connection-service.md` still names the pre-decision path.

## State A (before this changeset)

- `tddy-workflow/src/artifact_paths.rs` — `session_artifacts_root(session_dir)` → `session_dir/artifacts/`,
  plus `canonical_artifact_write_path`, legacy-layout resolution, and UTF-8 readers. No attachments concept.
- `tddy-daemon/src/session_context_docs.rs` — `ContextDoc { key, basename, path, description, exists }`
  derived **only** from the recipe manifest's `known_artifacts()`; a blank or unknown recipe yields an
  empty `Vec`. `read_session_context_doc_utf8` allowlists the manifest basenames with a
  canonicalize-and-contain guard rooted at `artifacts/`.
- `tddy-daemon/src/session_list_enrichment.rs:291` — maps those into `SessionEntry.context_docs`.
- `connection.proto:196` — `SessionContextDoc { key, basename, path, description, exists }`; no kind,
  no size. `SessionEntry.context_docs` is field 27.
- `tddy-daemon/src/session_file_upload.rs` — reusable guards: `validate_segment(value)` (rejects empty,
  `.`, `..`, separators, non-basename → `INVALID_ARGUMENT`) and
  `contained_canonical_dir(trusted_root, candidate)` (canonicalize both, confirm containment). Both are
  `pub(crate)`.
- Nothing on disk or on the wire distinguishes a user-attached document from a recipe artifact.

## State B (target)

Attachments are stored at `{session_dir}/artifacts/attachments/<basename>` and surface on
`SessionEntry.context_docs` as rows with `kind = ATTACHMENT`, alongside — never replacing —
recipe-owned manifest rows. Full contract in the PRD.

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-daemon/src/session_attachments.rs` | The store: `SessionAttachmentFile`, `list_session_attachments`, `copy_attachment_into_session` |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-workflow/src/artifact_paths.rs` | `SESSION_ATTACHMENTS_SUBDIR`, `session_attachments_root`, `canonical_attachment_write_path` + unit tests |
| `packages/tddy-workflow/src/lib.rs` | Re-export the three new items alongside `session_artifacts_root` |
| `packages/tddy-core/src/lib.rs` | Same three items added to the existing blanket `pub use tddy_workflow::{…}` re-export |
| `packages/tddy-daemon/src/lib.rs` | `pub mod session_attachments;` |
| `packages/tddy-daemon/src/session_context_docs.rs` | `ContextDocKind`, `kind` + `size_bytes` on `ContextDoc`, `ATTACHMENT_DOC_DESCRIPTION`, attachment rows appended in `context_docs_for_session` |
| ~~`packages/tddy-daemon/src/session_file_upload.rs`~~ | **No change needed** — `validate_segment` and `contained_canonical_dir` are already `pub(crate)` and are reused verbatim by the attachments module |
| `packages/tddy-daemon/src/session_list_enrichment.rs` | Map `kind` (as i32) and `size_bytes` into the proto `SessionContextDoc` |
| `packages/tddy-service/proto/connection.proto` | `enum SessionContextDocKind`; `kind = 6`, `size_bytes = 7` on `SessionContextDoc` |
| `packages/tddy-web/src/gen/connection_pb.ts` | Regenerated from the proto |
| `docs/ft/coder/session-layout.md` | ✅ Attachments subdirectory section linking the PRD |
| `docs/ft/coder/changelog.md` | Release note (wrap phase) |
| `docs/dev/changesets.md`, `packages/tddy-{workflow,core,daemon,service,web}/docs/changesets.md` | Index entries (wrap phase) — note `tddy-core` is in the list because its re-export was touched |

## Design decisions

### Attachments live under `artifacts/`, not beside it

An attachment is a session artifact the recipe does not own, so it belongs in the same tree: one
canonical artifact root per session, one containment guard, and a natural home for future kinds
(an attached image is as much an artifact as a generated `exploration.md`). The alternative,
`{session_dir}/attachments/`, would need a second trusted root for every guard that already roots at
`artifacts/`.

### `kind` on the row, not a second repeated field or a key prefix

`SessionContextDocKind kind = 6` keeps one list and one message shape while making the distinction
typed. `MANIFEST = 0` is the zero value, so a producer or consumer that ignores `kind` still reads
existing rows as recipe-owned. A `key = "attachment:<basename>"` convention was rejected: it pushes
the distinction into a string every consumer must parse, and it does not extend to further kinds.

### Writes are confined to the subdirectory, and never overwrite

`copy_attachment_into_session` only ever writes `artifacts/attachments/<validated-basename>`. A
recipe artifact (`PRD.md`, `exploration.md`) is therefore unreachable by an attachment write even
when the basenames match — the two are separate files, both listed, told apart by `kind`. A second
copy under an existing basename is refused rather than clobbering the stored bytes.

### `FAILED_PRECONDITION` for a duplicate basename

`tddy_rpc::Code::AlreadyExists` exists but has no `Status` constructor. `failed_precondition` keeps
this change out of `tddy-rpc` and matches the repo's existing use of that code for "current state
forbids this operation". Revisit if a client needs to distinguish it from other preconditions.

### Layout in `tddy-workflow`, policy in `tddy-daemon`

The path helpers are pure and dependency-free, so they sit next to `session_artifacts_root` where any
crate can reach them. The validated, containment-checked copy needs `tddy_rpc::Status` and the
existing `validate_segment` / `contained_canonical_dir` guards, so it lives in `tddy-daemon` — the
delete path is never a weaker gate than the write path, reusing one guard rather than a second copy.

### Attachments are listed independent of the recipe

`context_docs_for_session` currently short-circuits to an empty `Vec` for a blank or unknown recipe.
After this change only the *manifest* half is empty in that case; attachments still list. A session
someone attached a spec to must show it whether or not it runs a known recipe.

### No content read in this change

`read_session_context_doc_utf8` stays manifest-only. Attachments may be binary (images), so a UTF-8
reader is the wrong shape; the listing carries `size_bytes` so a client can render a file row without
the bytes. A type-aware fetch is a separate surface.

### Deterministic listing order

Attachments are sorted by basename so a listing does not depend on directory iteration order. Manifest
docs keep manifest order and come first, so the recipe's own docs stay at the top of the list.

## Known follow-ups (out of scope here)

- **Doc correction on the sibling branch**: `packages/tddy-daemon/docs/connection-service.md` on
  `feature/session-attach-docs/attach-proto` states the materialization target as
  `{session_dir}/attachments/`. Whichever of the two branches merges second must correct it to
  `artifacts/attachments/`. Not editable from here — `packages/*/docs/` changes go through the
  changeset workflow, and that branch is unmerged.
- No RPC accepts an attachment yet; `copy_attachment_into_session` has no caller on the wire until the
  start-session materialization path is built.
- No type-aware content fetch for attachment bytes.
- No staging root, chunked staging writer, or staging GC (those belong to the staging RPCs).

## Acceptance tests

`packages/tddy-daemon/src/session_list_enrichment.rs` (`mod tests`) — the store → enrichment → proto seam:

1. `an_attachment_copied_into_a_session_surfaces_on_the_proto_entry_with_the_attachment_kind` — copy a
   file into a pr-stack session through the store, run enrichment, and assert the proto row carries
   the attachment with `kind = ATTACHMENT`, its basename, its byte size, `exists = true`, and a
   non-empty description. Validates the whole feature end to end at the wire boundary.
2. `manifest_context_docs_keep_the_manifest_kind_when_the_session_also_has_attachments` — a session
   with `exploration.md` in `artifacts/` and one attachment: the manifest row reports
   `kind = MANIFEST` and precedes the attachment row. Validates that adding attachments does not
   reclassify or reorder recipe-owned docs.
3. `an_attachment_named_like_a_recipe_artifact_does_not_replace_the_manifest_row` — an attachment
   named `exploration.md` alongside the real `artifacts/exploration.md`: both rows are present with
   distinct paths and distinct kinds. Validates the "no overwriting PRD.md" requirement at the
   surface clients actually read.
4. `a_session_with_no_recipe_still_surfaces_its_attachments_on_the_proto_entry` — a session whose
   `changeset.yaml` names no recipe: `context_docs` holds exactly the attachment row. Validates that
   attachments do not depend on recipe resolution.

## Unit tests

`packages/tddy-workflow/src/artifact_paths.rs`:

1. `session_attachments_root_is_the_attachments_subdir_of_the_artifacts_root`
2. `canonical_attachment_write_path_joins_the_basename_under_the_attachments_root`

`packages/tddy-daemon/src/session_attachments.rs`:

3. `copying_a_source_file_stores_it_under_artifacts_attachments_with_its_bytes`
4. `copying_a_binary_source_stores_the_bytes_verbatim` — non-UTF-8 bytes survive (images)
5. `copying_creates_the_attachments_directory_when_the_session_has_none`
6. `copying_a_second_source_under_an_existing_basename_is_refused_and_keeps_the_stored_bytes` — `FAILED_PRECONDITION`
7. `copying_with_a_traversal_basename_is_rejected_as_invalid_argument` — nothing written outside
8. `copying_with_an_empty_basename_is_rejected_as_invalid_argument`
9. `copying_a_missing_source_is_rejected_as_invalid_argument`
10. `copying_a_directory_as_the_source_is_rejected_as_invalid_argument`
11. `copying_into_an_attachments_directory_symlinked_outside_the_artifacts_root_is_refused` (unix)
12. `listing_attachments_returns_them_sorted_by_basename_with_their_sizes`
13. `listing_a_session_without_an_attachments_directory_returns_an_empty_list`
14. `listing_skips_a_subdirectory_inside_the_attachments_directory`
15. `a_listed_attachment_carries_its_absolute_path_under_the_attachments_root`

`packages/tddy-daemon/src/session_context_docs.rs`:

16. `context_docs_list_the_manifest_docs_first_then_the_attachments`
17. `an_attachment_context_doc_carries_its_basename_as_key_with_a_size_and_a_description`
18. `context_docs_for_a_blank_recipe_list_only_the_attachments`
19. `a_manifest_context_doc_reports_its_on_disk_size_and_zero_when_absent`
20. `reading_an_attachment_basename_through_the_context_doc_reader_is_permission_denied`
