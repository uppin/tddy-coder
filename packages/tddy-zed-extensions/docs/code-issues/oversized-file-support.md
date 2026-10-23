# oversized-file: support.rs — an installed extension's language support

**Location:** `packages/tddy-zed-extensions/src/support.rs`
**Category:** oversized-file
**Detected:** 2026-10-23 by the `/pr-wrap` file-length gate on #682 (`#reshape` 23/24)
**Metrics:** **536 production lines** (2026-10-23, `tddy-tools restructure lines`, the items rule; 491 at the merge base `23fe83a5d`) · budget 500
**Restructure:** required
**Status:** Open — split deferred: stack-shared
**Claimed by:** none

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-23 | 536 | #682 (`#reshape` 23/24, `extension-code-actions`): +45 over the 491 at its merge base. `ExtensionSupport::parse` and `document_language_id` (K23's own `impl` block, which reads the support's private grammar and store), the `code_action_kinds` the manifest entry carries from `load_installed` to the `ServerBackend` (`with_code_action_kinds`), and `InstalledExtensions::register_installed`, which both front ends now register through |

## Why it was not split in #682

It is stack-shared. `extension-wasm-command` (#683, K24) is built on #682 and edits this file too, so a split
here would turn its diff into a conflict. Under the `#reshape` yolo pipeline a stack-shared file is recorded,
not split. The split goes to the `#reshape` sweep node, as the stack's other stack-shared splits do.

## What would close it

Splitting off a `support/` child, through `tddy-tools restructure`, for the `LanguageSupport` impl's backend and
parse wiring (`ExtensionSupport::parse`, `with_code_action_kinds`, `backend`) or for the `load_installed` /
`InstalledExtensions` loading half. Either takes the file back under 500. Re-measure after K24 lands, because
K24 changes `load_installed`.
