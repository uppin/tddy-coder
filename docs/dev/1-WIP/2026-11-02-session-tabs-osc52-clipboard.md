# Changeset: A terminal's OSC 52 copy reaches the viewer's clipboard

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 2 — one web module plus a tap in the terminal's write path; a streaming parser with chunk-split handling; possible desktop capability check
**Stack:** `#session-tabs 4/7` — branch `feature/session-tabs/osc52-clipboard`, base `feature/session-tabs/tmux-attach`
**PRD:** [PRD-2026-11-02-terminal-osc52-clipboard.md](../../ft/web/1-WIP/PRD-2026-11-02-terminal-osc52-clipboard.md)

## Initial Discovery

[2026-11-02-session-tabs-osc52-clipboard-initial-discovery.md](./2026-11-02-session-tabs-osc52-clipboard-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-web/docs/code-issues/oversized-file-ghostty-terminal.md` | ⚠ During — the recognizer is `src/lib/osc52.ts`; `GhosttyTerminal.tsx` gains only the tap call (a few lines), recorded at wrap |
| `tddy-desktop` | ℹ No `docs/code-issues/` directory. Touched only if the webview needs a clipboard capability |

## Affected Packages

- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): `osc52.ts`, the tap in `GhosttyTerminal`
- [ ] [`tddy-desktop`](../../../packages/tddy-desktop/README.md): only if the Tauri webview refuses `navigator.clipboard.writeText` (decided with the developer first)

## Responsibility

- `src/lib/osc52.ts`:
  - `createOsc52Recognizer(onCopy: (text: string) => void, options?: { maxDecodedBytes?: number })`
    returns `(chunk: Uint8Array | string) => void`;
  - it is a streaming state machine across chunks, handling BEL and `ESC \` terminators and an empty or
    `c` selection;
  - it decodes base64 → UTF-8;
  - a `?` payload, invalid base64 and payloads over `maxDecodedBytes` (default 1 MiB) are ignored;
  - `OSC52_MAX_DECODED_BYTES`.
- `GhosttyTerminal`: every byte written to the terminal also feeds one recognizer per terminal
  instance, which calls `copyToClipboard`. The bytes written to ghostty-web are unchanged.
- If ghostty-web 0.4 already consumes OSC 52 itself (checked in pass 2), the tap is still where the
  copy happens. Nothing is answered back to the program.

## Boundaries

- No OSC 52 reads, no primary selection, no paste changes.
- No tmux configuration (n3 attaches with `-T clipboard`).
- No new npm dependency.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1`–`n3` | nothing this node uses — they sit below only because the line is flattened | — | touch tabs, history or tmux |

## Draft PR contract

The first push publishes:

- `createOsc52Recognizer` and `OSC52_MAX_DECODED_BYTES`, with the body as `// TODO(osc52-clipboard): implement`;
- the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — unit tests over the recognizer; component tests feed bytes into a mounted terminal with a stubbed clipboard
**Concurrent with:** n1 `dormant-tabs`, n2 `agent-history`, n3 `tmux-attach`, n5 `tab-plus`
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    n1 → n6      n2 → n6      n3 → n7

## State A

- `src/lib/clipboard.ts` `copyToClipboard` exists (`navigator.clipboard.writeText`, with an
  `execCommand("copy")` fallback); it is used only by the Files tab.
- The terminal has paste only; no OSC 52 handling exists.

## State B

- Any program's OSC 52 write in any session terminal lands on the viewer's clipboard. Reads are
  silently refused.

## Delta

### New
- `packages/tddy-web/src/lib/osc52.ts`, `packages/tddy-web/src/lib/osc52.test.ts`

### Modified
- `packages/tddy-web/src/components/GhosttyTerminal.tsx` (the tap).
- Docs, at wrap: `docs/ft/web/web-terminal.md`.

## Testing Strategy

### Acceptance tests

**`tddy-web`**: `packages/tddy-web/cypress/component/GhosttyTerminalOsc52Acceptance.cy.tsx` (clipboard stubbed with `cy.stub(navigator.clipboard, "writeText")`)

- [ ] `an OSC 52 write on the agent terminal copies its text to the clipboard`
- [ ] `an OSC 52 write on a bash tab copies its text to the clipboard`
- [ ] `an OSC 52 read writes nothing and sends no input`
- [ ] `the output around an OSC 52 sequence still renders`

### Unit / integration tests

- [ ] `packages/tddy-web/src/lib/osc52.test.ts` (bun):
  - BEL and ST terminators;
  - empty and `c` selection;
  - a sequence split at every byte boundary is recognized once;
  - multibyte UTF-8;
  - `?` is ignored;
  - invalid base64 is ignored;
  - an over-cap payload is ignored;
  - unrelated OSC sequences (titles) are ignored;
  - two sequences in one chunk give two copies.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Parsing | bun unit | Pure and exhaustive |
| Wiring per feed | Cypress component | Both feed paths (`GhosttyTerminalSession`, `GrpcSessionTerminal`) write through `GhosttyTerminal` |

## Decisions & Trade-offs

- **A tap, not a filter**, so the emulator sees exactly what it saw before.
- **Write-only**, the safe subset of OSC 52, as chosen in the interview.
- **1 MiB cap**, which bounds memory for a hostile or buggy stream.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-osc52-clipboard-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
