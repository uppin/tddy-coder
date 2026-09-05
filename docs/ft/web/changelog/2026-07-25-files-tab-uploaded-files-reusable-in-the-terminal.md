# 2026-07-25 — Files tab: uploaded files, reusable in the terminal

- The Session Inspector has a new **Files** tab listing the files already uploaded to the session (server-read from `{session_dir}/uploads/`, newest first), so an upload stays reusable instead of its path being typed once and lost. See [session-files-inspector.md](../session-files-inspector.md).
- Reuse a file by **dragging** its row onto the terminal (desktop) or **tapping / Insert** (mobile) — the file's host path is inserted without re-uploading (it is already on the host). Starting a drag or an insert **auto-closes the Inspector** so the terminal beneath becomes the drop target.
- Each row also offers **Copy path** (insecure-origin-safe clipboard) and a two-step **Delete** (removes the upload from the host and prunes the emptied drop folder).
