# 2026-10-20 — `LanguageId` replaces `enum Language`
**Type:** Feature

`tddy_lsp::LanguageId` (open string, `LanguageId::RUST = "rust"`) keys `LspAllowList` and `LspKey`;
`LaunchSpec` gains `language_id` (`with_language_id`; `allow` fills an empty one with the id), and document
sync sends it. `registry::workspace_root_within(dir, bound)` is the Cargo root walk bounded by the caller's
root.

Docs: [workspace-root.md](../workspace-root.md).
