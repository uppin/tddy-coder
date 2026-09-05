# 2026-08-01 — Attach documents while creating a session

- The **New session** form (and the PR-stack **Start session** dialog, which shares it) gained an attachments section: pick or drag-drop local files, or reference a document that already exists on a connected host with no upload at all. Available for every session type and for peer spawns.
- Each attachment is one row showing its source and size, with an **editable name** — renaming changes only what the agent sees, never the stored file.
- Attached documents land in the session's `artifacts/attachments/` **before the agent's first turn**, so an initial prompt written in the form can refer to them.
- The host-document picker browses four places on a host: a session's planning artifacts, its uploaded files, its git worktree, and the project's repository.
- Nothing uploads until **Create** is pressed, so an abandoned form costs nothing — and a creation refused by a branch conflict re-uses the bytes already sent instead of uploading them again.
- The host reports its own progress per attachment while the session is created, which matters most when the bytes are crossing between two machines.
- A file larger than the host's configured attachment limit is refused when it is picked, naming the limit, rather than after a long upload.
- Files can be staged to whichever daemon the browser is connected to and still used by a session started on a **different** host — the session's host fetches them.
- Two names differing only in case (`Spec.md` and `spec.md`) are now treated as a collision, because storing both breaks session creation on macOS.
