# 2026-07-26 — Telegram model keyboards read the shared model catalog

- The Telegram **Claude** and **Cursor** model keyboards no longer carry their own copies of the model lists — they render `tddy_core::backend::claude_cli_models()` / `cursor_cli_models()`, so a catalog change reaches Telegram, the web dropdowns, and the CLI defaults at once. This also corrects the Claude keyboard's stale labels and grows the Cursor keyboard from 3 entries to the catalog's 5. See [telegram-session-control.md](../telegram-session-control.md).
- An out-of-range `tcm:`/`tcur:` model index is now **rejected with an error** rather than resolving to some model; picking a Claude model still stores it in `changeset.yaml` and `.session.yaml` unchanged.
