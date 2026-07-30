# Session attachments — user-attached documents as session artifacts

A session's **attachments** are documents a person attaches to that session: a spec, a screenshot, a
log, a design note. They are **session artifacts the recipe does not own** — they live in the same
`artifacts/` tree as recipe-authored planning docs (`PRD.md`, `exploration.md`, `stack-plan.yaml`),
under their own `attachments/` subdirectory, and they surface to clients through the same
`SessionEntry.context_docs` list, tagged with a different **kind**.

Keeping them inside `artifacts/` means one canonical artifact root per session, one containment
guard, and room for kinds beyond text — an attached image is an artifact of the session just as much
as a generated markdown doc.

## On-disk layout

```
{sessions_base}/sessions/{session_id}/
  artifacts/
    PRD.md                  <- recipe-owned (manifest)
    exploration.md          <- recipe-owned (manifest)
    stack-plan.yaml         <- recipe-owned (manifest)
    attachments/
      requirements.pdf      <- attachment
      screenshot.png        <- attachment
      notes.md              <- attachment
```

The attachments directory is **one flat level**. An attachment is addressed by **basename** only;
there is no nesting, so no client-supplied path fragment ever becomes more than a single segment.

A recipe artifact and an attachment may share a basename (`artifacts/PRD.md` and
`artifacts/attachments/PRD.md`) — they are distinct files. Writing an attachment can never overwrite
a recipe-owned artifact, because attachment writes are confined to the `attachments/` subdirectory.

## API surface

### `tddy-workflow` — layout (`artifact_paths`)

```rust
/// Subdirectory of `artifacts/` holding user-attached documents.
pub const SESSION_ATTACHMENTS_SUBDIR: &str = "attachments";

/// `session_dir/artifacts/attachments/`
pub fn session_attachments_root(session_dir: &Path) -> PathBuf;

/// `session_dir/artifacts/attachments/<basename>` — a path for a new write; performs no validation.
pub fn canonical_attachment_write_path(session_dir: &Path, basename: &str) -> PathBuf;
```

These are pure path helpers alongside `session_artifacts_root` / `canonical_artifact_write_path`.
They touch no filesystem and enforce no policy.

### `tddy-daemon` — store (`session_attachments`)

```rust
/// One attachment file on disk under `artifacts/attachments/`.
pub struct SessionAttachmentFile {
    pub basename: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// Lists the session's attachments, sorted by basename. An absent attachments directory yields an
/// empty list, not an error.
pub fn list_session_attachments(session_dir: &Path) -> Vec<SessionAttachmentFile>;

/// Copies `source` into `artifacts/attachments/<basename>`, returning the written path.
pub fn copy_attachment_into_session(
    session_dir: &Path,
    source: &Path,
    basename: &str,
) -> Result<PathBuf, Status>;

/// The in-memory counterpart, for bytes that have no local file to copy from — a `HostDocumentRef`
/// fetched from a peer daemon arrives as a byte buffer.
pub(crate) fn write_attachment_bytes(
    session_dir: &Path,
    basename: &str,
    data: &[u8],
) -> Result<PathBuf, Status>;
```

Both write paths share one `create_attachment_file_exclusively` gate, so neither is weaker than the
other: `basename` is a single validated segment, the attachments directory must resolve inside the
canonical `artifacts/` root, and the target is opened with `create_new(true)` — one atomic
existence-check-plus-create, so an existing attachment (a regular file **or** a symlink planted in the
directory) is refused with `FAILED_PRECONDITION` rather than followed or truncated. A failed write
removes the partial file so a retry is not blocked. A bad `basename` is refused with *"attachment
basename must be a single path segment"* — the uploads path's rule, reused, with a message naming the
field the caller actually sent.

### `tddy-daemon` — context docs (`session_context_docs`)

```rust
/// Whether a context doc is recipe-owned or user-attached.
pub enum ContextDocKind { Manifest, Attachment }

pub struct ContextDoc {
    pub key: String,
    pub basename: String,
    pub path: PathBuf,
    pub description: String,
    pub exists: bool,
    pub kind: ContextDocKind,
    pub size_bytes: u64,
}

/// Human description carried by every attachment row.
pub const ATTACHMENT_DOC_DESCRIPTION: &str = "Attached document";
```

`context_docs_for_session(recipe_name, session_dir)` returns the recipe manifest's docs **followed
by** the session's attachments.

### Wire (`connection.proto`)

```proto
enum SessionContextDocKind {
  SESSION_CONTEXT_DOC_KIND_MANIFEST = 0;    // recipe-owned planning artifact
  SESSION_CONTEXT_DOC_KIND_ATTACHMENT = 1;  // user-attached document
}

message SessionContextDoc {
  string key = 1;
  string basename = 2;
  string path = 3;
  string description = 4;
  bool exists = 5;
  SessionContextDocKind kind = 6;
  uint64 size_bytes = 7;
}
```

`MANIFEST = 0` is the zero value, so an existing producer or reader that knows nothing about `kind`
keeps treating rows as recipe-owned docs.

## Behavior

### Copying an attachment in

- `basename` must be a **single safe path segment**. Empty, `.`, `..`, anything containing `/` or
  `\`, and anything whose `Path::file_name()` differs from the input are refused with
  `INVALID_ARGUMENT` and write nothing.
- `source` must resolve to an existing **regular file**. A missing path or a directory is refused
  with `INVALID_ARGUMENT`.
- The attachments directory is created on demand (`artifacts/` and `attachments/` both).
- After creation, the attachments directory is canonicalized and confirmed to resolve **inside** the
  canonical `artifacts/` root — a symlinked `attachments` pointing outside the session tree is
  refused (`INVALID_ARGUMENT`) rather than followed.
- An **existing** target is never overwritten: a second copy under the same basename is refused with
  `FAILED_PRECONDITION`, leaving the stored bytes untouched.
- On success the file's bytes are copied verbatim (binary-safe) and the written path is returned.

### Listing attachments

- Regular files only. Subdirectories and other non-regular entries are skipped rather than listed.
- Sorted by basename, so a listing is deterministic across filesystems.
- `size_bytes` comes from the file's metadata.
- A missing attachments directory (the common case — most sessions have none) yields an empty list.

### Surfacing on `SessionEntry.context_docs`

- Manifest docs come first, in manifest order, each with `kind = MANIFEST` and its recipe-authored
  description.
- Attachments follow, basename-sorted, each with `kind = ATTACHMENT`, `key = basename`,
  `description = ATTACHMENT_DOC_DESCRIPTION`, and `exists = true` (a listed attachment is on disk by
  construction).
- `size_bytes` is populated for **both** kinds: the on-disk size, or `0` when the doc does not exist.
- `key` is unique **within** a kind. A consumer addressing a row keys on `(kind, key)` — an
  attachment named `exploration.md` and the pr-stack manifest's `exploration` doc are different rows.
- Attachments are **recipe-independent**: a session with a blank or unknown `recipe` still surfaces
  its attachments, and only its manifest list is empty.

### Reading contents

`read_session_context_doc_utf8` is unchanged: its allowlist remains the recipe manifest's basenames,
and an attachment basename is refused with `PERMISSION_DENIED`. Attachments may be images or other
binaries, so a UTF-8 reader is the wrong shape for them; a type-aware fetch is a separate surface.
That surface is [`ReadHostDocument`](#readhostdocument--unary-host-document-fetch): an attachment's
bytes are read with scope `SESSION_ARTIFACT` and `relative_path = attachments/<basename>`, which is
binary and capped rather than UTF-8 and unbounded.

## Start-session materialization

`StartSessionRequest.attachments` (field 29, `repeated SessionAttachment`) lets a client attach documents **at start time**, before the session directory exists. The daemon materializes every attachment into `artifacts/attachments/<basename>` **before** the agent launches, so the agent sees a plain local file regardless of which source produced it. Each attachment carries a `basename` (separate from the source locator, so the UI can rename without touching the stored file) plus a `oneof source` naming the host authority that owns the bytes.

### Staging area

A per-host, per-caller staging root at `{tddy_data_dir}/staging/{os_user}/` holds pre-session uploads. A batch is one `staging_id` subdirectory; files land at `{staging_root}/{staging_id}/{file_name}`.

- `UploadStagedAttachmentChunk` appends ordered chunks (48 KiB client-side chunking, mirroring `UploadSessionFileChunk`), validates `staging_id` and `file_name` as basenames, and returns the completed `StagedAttachmentEntry` on the final chunk.
- `ListStagedAttachments` returns a batch's files (or every batch for the caller when `staging_id` is empty), newest-first.
- `DeleteStagedAttachment` removes one staged file; a delete is never a weaker gate than a write (same `validate_segment` + `contained_canonical_dir` guards as the writer).
- All three route by `daemon_instance_id`: empty / matching the local instance = local; otherwise forwarded to the peer daemon over the LiveKit common room (unary, so forwardable — streaming RPCs return `unimplemented` for `PeerRoute::Forward`).

### `ReadHostDocument` — unary host-document fetch

`ReadHostDocument(ReadHostDocumentRequest) returns (ReadHostDocumentResponse)` reads the bytes of a document that already exists on a connected host. Unary so it is forwardable; binary (attachments may be images/PDFs), so it does **not** reuse the UTF-8 readers. The owning daemon resolves the scope root under **its own** `os_user` mapping — the referencing client's host grants no access. `relative_path` is POSIX-separated, no `.`/`..`, not absolute, validated against the resolved root with canonicalize-and-contain. The **full file path** (not just the parent directory) is canonicalized and re-checked against the canonical scope root, so a symlinked file inside the root that points outside is refused — `std::fs::read` follows symlinks, so a lexical containment check on the parent alone is not enough. The file's on-disk size is checked against the cap **before** the contents are read, so an oversized file is refused without loading it into memory.

| Scope | Root | `relative_path` shape |
|-------|------|-----------------------|
| `SESSION_ARTIFACT` | `{session_dir}/artifacts/` | basename of a `SessionContextDoc` |
| `SESSION_UPLOAD` | `{session_dir}/uploads/` | `<upload_id>/<file_name>` |
| `SESSION_WORKTREE` | the session's git worktree | a path surfaced by `ListWorktreeDirectory` |
| `PROJECT_REPO` | `ProjectEntry.main_repo_path` | a checked-in path (e.g. `docs/ft/*.md`) |

A file larger than `MAX_HOST_DOCUMENT_BYTES` is refused with `INVALID_ARGUMENT` (not truncated — a truncated attachment is useless; the caller should stage larger docs, since staging is chunked with no single-message limit). This keeps the unary response within gRPC's default message-size budget. It routes by `daemon_instance_id` like the staging RPCs.

### Materialization flow

Two attachment sources, both naming the host authority that owns the bytes:

| Source | Locator | Behavior |
|--------|---------|----------|
| `StagedAttachmentRef` | `daemon_instance_id` + `staging_id` + `file_name` | `daemon_instance_id` must be empty or match the host running the session; a mismatch is a request **error** (`INVALID_ARGUMENT`) — never a cross-host fetch, never a silent empty attachment. The staged file is copied via `copy_attachment_into_session`, and only once its upload is **complete** (the writer marks a `.staged-complete` sentinel on the final chunk; an in-progress or aborted upload is refused with `FAILED_PRECONDITION`, never copied as truncated bytes). |
| `HostDocumentRef` | `daemon_instance_id` + `HostDocumentScope` + `session_id`/`project_id` + `relative_path` | If `daemon_instance_id` is empty / local, the daemon reads locally via the `ReadHostDocument` logic; otherwise it forwards `ReadHostDocument` to the owning peer, then writes the returned bytes to `artifacts/attachments/<basename>`. The session host re-checks `MAX_HOST_DOCUMENT_BYTES` on the forwarded bytes before writing, so a buggy/older peer cannot push an oversized blob. |

- `basename` is validated as a single safe segment (the store's `validate_segment`). Duplicate basenames **within one request** are rejected with `INVALID_ARGUMENT` (no silent renaming) — a request error, never reaching the store's `FAILED_PRECONDITION`.
- Exactly one source variant must be set; an unset source is `INVALID_ARGUMENT`.
- Materialization happens for **all session types** (tool / claude-cli / sandboxed claude-cli / cursor-cli / workspace). For the tool branch the daemon pre-creates `session_dir` (using the session_id it already passes to the child) and materializes before spawn; for the other types it materializes right after the handler creates `session_dir` and before the spawn.
- On any materialization failure, `StartSession` fails — no partial attachments left behind: the `artifacts/attachments/` writes for the request are cleaned up before returning the error.

## Constraints

- `copy_attachment_into_session` is the host-side store the start-session materialization path calls; the staging RPCs and `ReadHostDocument` are the wire surface that feeds it. Nothing else reaches the wire except the `SessionContextDoc` fields and the attachment RPCs.
- Attachment bytes are never read through the context-doc surface (see above).
- No staging garbage collection yet: a consumed batch is not auto-deleted, and abandoned batches have no TTL — tracked as a follow-up. Session-scoped attachments live and die with the session directory, removed by `DeleteSession` along with the rest of the tree.

## Related

- [Session directory layout](session-layout.md) — the canonical session tree.
- [PR stacking § Context docs in proto](pr-stacking.md#context-docs-in-proto) — the recipe-manifest
  half of `context_docs`.
