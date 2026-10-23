# 2026-10-23 — the cold restructure verify judges installed languages' changed files by their parse (`#reshape` 23/24, #682)

**Type:** Feature

`installed_languages` registers the installed languages with `register_installed` and builds their `ExtensionVerify`s with no server, so the cold `restructure verify` reports parse errors in other languages' changed files and fails on them.

Full history: [cross-package entry](../../../../docs/dev/changesets/2026-10-23-reshape-extension-code-actions.md).
