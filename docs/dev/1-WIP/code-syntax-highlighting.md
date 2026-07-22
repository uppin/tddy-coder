# Changeset: Code syntax highlighting in the Code pane preview

**PRD**: `docs/ft/web/session-code-pane.md` (File preview section + acceptance criterion #7)
**Branch**: `feat-code-hightlighting`

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests
- [x] Write unit tests
- [x] Add `react-syntax-highlighter` dependency (+ `@types/…`)
- [x] `codeLanguageForPath` helper (`src/lib/codeLanguage.ts`)
- [x] `CodeBlock` highlight component
- [x] Wire `CodeBlock` into `WorktreeCodePane` non-markdown branch

## Summary

The Worktree Code pane preview (`WorktreeCodePane`, added in `worktree-code-pane`) currently dumps
every non-markdown file into a plain `<pre>`. This change tokenizes and colors recognized code
files. Scope: the Code pane preview only — markdown fenced code blocks and `SessionFilesPanel`'s
YAML box are unchanged.

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-web/src/lib/codeLanguage.ts` | `codeLanguageForPath(relPath) → prism id \| null` (extension map; unknown → null) |
| `packages/tddy-web/src/lib/codeLanguage.test.ts` | Unit tests for the mapping (bun:test) |
| `packages/tddy-web/src/components/session/CodeBlock.tsx` | `PrismLight` highlighter; theme from `.dark`; plain-`<pre>` fallback when language is null |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-web/package.json` | + `react-syntax-highlighter`, + dev `@types/react-syntax-highlighter` |
| `packages/tddy-web/src/components/session/WorktreeCodePane.tsx` | non-markdown branch renders `<CodeBlock content relPath/>` |
| `packages/tddy-web/cypress/support/testIds.ts` | + `worktreeCodeHighlight: "worktree-code-highlight"` |
| `packages/tddy-web/cypress/support/pages/worktreeCodePanePage.ts` | + `highlight()` |
| `packages/tddy-web/cypress/component/WorktreeCodePaneAcceptance.cy.tsx` | + highlight tests; extend `aWorktreeBackend` fixtures (`config.yaml`, `LICENSE`) |

## Design decisions

### Prism via `react-syntax-highlighter` (`PrismLight`)
Themes are JS objects, not external CSS — CSP/offline-safe for the self-hosted web bundle. Only the
languages we ship are registered (keeps the bundle small). No auto-detection: the language is
derived from the file path, matching the existing `sessionWorkflowPreview.ts` extension approach.

### Language derived client-side; unknown → plain
The backend `ReadWorktreeFile` returns raw UTF-8 only (no language field), so `codeLanguageForPath`
maps the extension. An unrecognized/extensionless path returns `null` and the preview falls back to
the current plain monospace `<pre>` — no highlighter, no crash.

### Theme follows the app's `.dark` class
`CodeBlock` picks `oneLight`/`oneDark` from the active `.dark` class (the app's
`@custom-variant dark (&:is(.dark *))` strategy in `src/index.css`), defaulting to light.

## Acceptance tests

Cypress component, in-memory RPC backend, extending
`packages/tddy-web/cypress/component/WorktreeCodePaneAcceptance.cy.tsx`:

1. **highlights a selected Rust code file with syntax tokens** — `src/main.rs` → highlight container
   present with `.token` spans; the file text is preserved.
2. **highlights a selected YAML file** — `config.yaml` → highlight container + `.token` present.
3. **renders a file with no recognized extension as plain monospace without highlighting** —
   `LICENSE` → its text shows and the highlight container does not exist.

## Unit tests

`packages/tddy-web/src/lib/codeLanguage.test.ts` — `codeLanguageForPath` maps `.rs`→rust,
`.ts`/`.tsx`→tsx, `.py`→python, `.json`→json, `.yaml`/`.yml`→yaml; normalizes uppercase extensions;
returns `null` for extensionless names (`LICENSE`, `Makefile`) and for `.md` (handled by the
markdown renderer, not `CodeBlock`).

## Out of scope

- Markdown fenced (```` ``` ````) code block highlighting inside `renderSimpleMarkdown`.
- Upgrading `SessionFilesPanel`'s `yaml-syntax-highlight` box to real highlighting.
- Any backend/proto change (language stays a client-side concern).

## Validation Results

- **Tests**: unit `codeLanguage.test.ts` 8/8; Cypress `WorktreeCodePaneAcceptance` 9/9 (incl. the
  two highlight tests + the extensionless-`LICENSE` no-highlight fallback).
- **Build**: `vite build` exit 0 — the new `PrismLight` + 15 language imports bundle cleanly.
- **Quality scan**: no `console`/`debugger`/TODO/FIXME or phase-marker comments in the changed
  source; `CodeBlock`'s theme read is SSR-guarded; `codeLanguageForPath` treats dotfiles
  (`.env`) as extensionless (→ plain). No mock/hardcoded logic.
- **Scope**: web-only (no Rust/proto touched) — `cargo` steps N/A. Reverted the unrelated
  `packages/tddy-web/src/buildId.ts` build-timestamp churn.
- **Known tradeoff**: the Prism theme is read once per render, so toggling dark mode while a file
  is open updates on the next render, not live (a MutationObserver was intentionally deferred).
