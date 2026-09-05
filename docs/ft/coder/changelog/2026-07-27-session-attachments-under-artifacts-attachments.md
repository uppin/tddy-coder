# 2026-07-27 — Session attachments under artifacts/attachments/

- Sessions store **user-attached documents** at `{session_dir}/artifacts/attachments/<basename>` (flat, basename-only) via the daemon **`session_attachments`** store; layout helpers live in **`tddy-workflow`** ([session-attachments.md](../session-attachments.md), [session-layout.md](../session-layout.md)).
- `SessionEntry.context_docs` carries **`kind`** (`MANIFEST` / `ATTACHMENT`) and **`size_bytes`**; attachment rows follow recipe-manifest docs and still list when the recipe is blank or unknown. The UTF-8 context-doc reader stays manifest-only (attachments may be binary).
- No RPC accepts an attachment yet — start-session materialization and a type-aware content fetch are follow-ups.
