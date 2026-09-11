# Host documents, staging and uploads (tddy-session-files)

Everything a session's files are read from, written to and picked out of: the host-document scopes,
the pre-session staging area, the terminal-drop uploads, and the attachment store a session start
materialises into.

Product contract: [session-attachments.md](../../../docs/ft/coder/session-attachments.md),
[session-files-inspector.md](../../../docs/ft/web/session-files-inspector.md),
[web-terminal.md](../../../docs/ft/web/web-terminal.md) § File drop upload.

## Host documents — a scope and a relative path, never a host path

`ReadHostDocument` and `StreamReadHostDocument` fetch the bytes of a document that already exists on
a connected host. `HostDocumentScope` names a root the **owning** host resolves itself under **its
own** `session_token` → `os_user` mapping; the referencing client's host grants it no access. A raw
path field would let any caller name any file the host's user can read, which is why there isn't one.

| Scope | Root | `relative_path` shape |
|---|---|---|
| `SESSION_ARTIFACT` | `{session_dir}/artifacts/` | A `SessionContextDoc`'s `relative_path` — its basename for a stack-level manifest doc, `prs/<node_id>/<basename>` for a per-PR one, `attachments/<basename>` for an attachment. A full relative path resolves behind the canonicalize-and-contain guard, so a nested source is legal; only the final segment is name-validated |
| `SESSION_UPLOAD` | `{session_dir}/uploads/` | Exactly `"<upload_id>/<file_name>"` |
| `SESSION_WORKTREE` | the session's git worktree | A path surfaced by `worktree.WorktreeService`'s `ListWorktreeDirectory`, re-gated against `tddy_worktree_service::worktree_files::git_listed_files` on read |
| `PROJECT_REPO` | `ProjectEntry.main_repo_path` | A checked-in path, e.g. `docs/ft/*.md` |
| `STAGED_ATTACHMENT` | `{staging_base}/{os_user}/` | Exactly `"<staging_id>/<file_name>"`, and the file's `.staged-complete` marker must be present |

Adding a source of documents means adding a scope — that is the point, so each new root is reviewed
rather than reachable by construction. `STAGED_ATTACHMENT` mirrors `SESSION_UPLOAD`'s two-segment
shape so it reuses that validation rather than inventing a parse
(`host_documents::validate_staged_attachment_relative_path`).

`relative_path` is POSIX-separated, non-absolute and `.`/`..`-free, and the **full file path** — not
just its parent — is canonicalized and re-checked against the canonical scope root. `std::fs::read`
follows symlinks, so a lexical check on the parent alone is not enough: a symlink inside the root
pointing outside would be served.

The reads are binary (attachments may be images or PDFs), so they do not reuse the UTF-8 readers.
Size is checked from `metadata().len()` **before** anything is read: the unary variant refuses a
document over `MAX_HOST_DOCUMENT_BYTES` (4 MiB) with `INVALID_ARGUMENT` rather than truncating it, and
the streaming variant refuses one over the host's configured `max_attachment_bytes` before its first
frame. Both share `resolve_host_document`, and therefore every guard above.

`StreamReadHostDocument` frames at `HOST_DOCUMENT_FRAME_BYTES` (48 KiB) with `total_byte_size`
stamped on **every** frame, so a consumer sizes a progress bar from the first one and needs no
preamble. A zero-byte document still yields exactly one empty frame.

**One gate, one spelling.** `contained_in_scope_root` and `validate_relative_path` are re-exported
from `host_documents` rather than declared at the crate root. Two spellings of "is this inside the
root" is how one of them ends up being the lenient one — a second pair at the crate root enforces
neither `SESSION_WORKTREE`'s git listing nor the basename rule the served path applies, so a caller
reaching for the crate's advertised gate gets the weaker check. So there is one spelling, it lives
beside the resolver that runs it, and `resolve_host_document` calls those two and nothing else.

## Staging — before the session exists

Documents picked in the Start-Session form are uploaded ahead of the session, into a per-host,
per-caller root at `{staging_base}/{os_user}/{staging_id}/{file_name}`, then referenced from
`StartSessionRequest.attachments` by a `StagedAttachmentRef`. Chunking mirrors
`UploadSessionFileChunk`: client-side 48 KiB chunks, one unary call per chunk, `last` on the final
one, which writes a `.staged-complete` marker and returns the completed `StagedAttachmentEntry`.

`staging_id` and `file_name` are untrusted client input that become path segments, so each is
validated as a pure basename and the per-batch directory is canonicalize-and-contained under the
caller's staging root. An unsafe segment is `INVALID_ARGUMENT`, and nothing is written or removed.
`DeleteStagedAttachment` reuses the writer's guards verbatim — a delete must never be a weaker gate
than a write.

**The staging root is restart-cleared, not durable.** It sits under the process temp dir
(`default_staging_base_dir`) rather than under the data directory, and that is deliberate: staged
batches have no TTL and no garbage collection, so a Start-Session form that is filled in and abandoned
would leak its uploads forever. A root the host clears on restart bounds abandonment without a
background job. It does **not** bound a batch a `StartSession` actually consumed — that cleanup is
tracked in [`docs/dev/todo/`](../../../docs/dev/todo/).

A `StagedAttachmentRef` naming a host **other** than the one `StartSession` runs on is **fetched from
that host** through the `STAGED_ATTACHMENT` scope rather than refused: the browser stages to whichever
host it is connected to and may then start the session elsewhere. What makes that safe is the
`.staged-complete` marker, checked by `resolve_host_document` on the host that owns the bytes, so an
in-progress or aborted upload is refused with `FAILED_PRECONDITION` whether the fetch is local or
forwarded. A `daemon_instance_id` that is neither local nor a connected peer is `FAILED_PRECONDITION`.

## Uploads — the terminal drop

Files dropped on the web terminal are chunked client-side and appended, in order, to
`{session_dir}/uploads/{upload_id}/{file_name}`. Each drag gesture gets a fresh `upload_id` subfolder,
so original filenames survive and collisions between drops are impossible. The final chunk returns
the file's absolute host path, which the web types into the terminal.

`ListSessionUploads` reads that tree back as a **flat, newest-first** list across every `upload_id`
folder — one `SessionUploadEntry` (`upload_id`, `file_name`, absolute `host_path`, `size_bytes`,
`uploaded_at_ms`) per regular file. A missing uploads root is an **empty list, not an error**.
`DeleteSessionUpload` removes one file addressed by `upload_id` + `file_name`, prunes the emptied
folder, and answers `NOT_FOUND` for a missing file. Both untrusted segments go through the same
`session_file_upload::contained_canonical_dir` guard the writer applies.

## Workflow files

`ListSessionWorkflowFiles` returns the allowlisted basenames present on disk under the session
directory — `changeset.yaml`, `.session.yaml`, `PRD.md`, `TODO.md` — behind a **fixed server
allowlist**. An entry whose canonical path falls outside the canonical session directory (a symlink
escape) is omitted from the list rather than reported. `ReadSessionWorkflowFile` returns UTF-8 text
for one of those basenames, refusing empty, non-allowlisted or path-unsafe values (`..`, `/`, `\`).
Responses are not size-capped at the API layer; operators keep workflow files to a reasonable size
for the deployment.

## The attachment store

`session_attachments` is where a session's attachments live: `session_dir/artifacts/attachments/`,
**alongside** and never inside the recipe-owned planning artifacts, one flat level so no
client-supplied value ever becomes more than one path segment.

`StartSession` materialises every attachment there **before** the agent launches, so the agent sees a
plain local file regardless of which source produced it. `SessionAttachment.basename` is separate
from the source locator, so the UI can rename an attachment without touching the stored file.
Duplicate basenames within one request are rejected with `INVALID_ARGUMENT` before anything is
written — no silent renaming — and on any materialisation failure the partial writes for that request
are cleaned up before the RPC returns its error.

**Both write paths share one gate.** `copy_attachment_into_session` streams a local file (what a
`StagedAttachmentRef` uses) and `write_attachment_bytes` writes bytes already in memory (what a
`HostDocumentRef` uses, since its bytes arrive as a buffer and there is no local file to copy). Both
call `create_attachment_file_exclusively`, so neither is the weaker one: the target is opened with
`create_new(true)`, making the existence check and the creation one atomic step, and an existing
target — a regular file **or a symlink planted in the attachments directory** — is refused with
`FAILED_PRECONDITION` rather than followed or truncated. A failed write removes the partial file so a
retry is not blocked.

Basename refusal names the field the caller sent. The rule is the uploads path's `validate_segment`,
reused rather than reimplemented, but wrapped by `validate_attachment_basename` so a bad
`SessionAttachment.basename` is refused with *"attachment basename must be a single path segment"*
instead of the uploads path's message about `upload_id` / `file_name` — fields that do not exist in
the attachment API. The staging RPCs keep the original message, where those fields are real.

## Context documents and PR-stack children

`session_context_docs` is the *list* of a session's planning documents — surfaced on `SessionEntry`
and to child Start-session prompts — plus an allowlisted, canonicalize-and-contained reader for their
contents rooted at `session_artifacts_root`. It has two kinds: the recipe manifest's own artifacts
under `session_dir/artifacts/`, then the user-attached documents under `artifacts/attachments/`.
Attachments are recipe-independent, so a session with a blank or unknown recipe still lists them.

Manifest rows come from two sources, because a manifest's `known_artifacts` returns `&'static`
basenames and so cannot enumerate the N per-PR documents a PR-stack orchestrator writes under
`artifacts/prs/<node_id>/`. Those are discovered by scanning, exactly as attachment rows are, and are
classified `Manifest` rather than `Attachment`: the docs pass authored them, no operator attached
them.

`stack_doc_attachments` builds the list a planned PR's child session is spawned with. One helper feeds
both spawn paths — the agent's `pr_spawn_child` and the web Start-session dialog — because a child
that differs by how it was started is a bug the operator cannot see. Every document is attached **by
reference**: a `HostDocumentRef` naming the orchestrator's own session and host, which the
materialiser fetches, streaming if that host is not the one starting the session. Nothing is copied up
front, so a cross-host stack works unchanged. The source may be nested (`prs/n1/PRD.md`) because
`SESSION_ARTIFACT` resolves a full relative path; the destination is a flat basename because the
attachment store is one level deep.

## Size caps

`DaemonConfig::max_attachment_bytes` (default 64 MiB) is a **policy** cap, distinct from
`MAX_HOST_DOCUMENT_BYTES`, which is the unary transport ceiling. It bounds `StreamReadHostDocument`
and the cross-host staged fetch, and it is published on the common-room daemon advertisement —
omitted when zero, so "unadvertised" stays distinguishable from "no cap" — so the web can show and
enforce it at pick time rather than letting an operator wait out an upload the host will refuse.
