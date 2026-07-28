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
```

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
Until it exists, clients list attachments (name, size, path) without reading their bytes.

## Constraints

- No RPC accepts an attachment yet. `copy_attachment_into_session` is the host-side store that the
  start-session attachment materialization path calls once it is built; nothing in this feature
  reaches the wire except the two new `SessionContextDoc` fields.
- Attachment bytes are never read through the context-doc surface (see above).
- No garbage collection: attachments live and die with the session directory, removed by
  `DeleteSession` along with the rest of the tree.

## Related

- [Session directory layout](session-layout.md) — the canonical session tree.
- [PR stacking § Context docs in proto](pr-stacking.md#context-docs-in-proto) — the recipe-manifest
  half of `context_docs`.
