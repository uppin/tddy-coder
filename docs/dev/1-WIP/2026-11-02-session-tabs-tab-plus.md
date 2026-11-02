# Changeset: "+" sits after the last tab

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 2 — one component's layout plus moving a menu into a portal; layout is hard to assert in the CSS-less component harness
**Stack:** `#session-tabs 5/7` — branch `feature/session-tabs/tab-plus`, base `feature/session-tabs/osc52-clipboard` · **PR:** [#808](https://github.com/uppin/tddy-coder/pull/808)
**PRD:** [PRD-2026-11-02-tab-plus-follows-last-tab.md](../../ft/web/1-WIP/PRD-2026-11-02-tab-plus-follows-last-tab.md)

## Initial Discovery

[2026-11-02-session-tabs-tab-plus-initial-discovery.md](./2026-11-02-session-tabs-tab-plus-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-08-01-tddy-web-inactive-session-activities-follow-ups.md](../todo/2026-08-01-tddy-web-inactive-session-activities-follow-ups.md) | ⚠ During — "the component harness renders without any CSS, so no Cypress component test can assert layout". The structural tests assert DOM order, the sticky class and that the menu is not a descendant of the scroller. The overflow test imports the app stylesheet **in its own spec** (`import "../../src/index.css"`), as some terminal layout specs already do, and so leaves the shared harness alone |

## Affected Packages

- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): `SessionTerminalTabs`, `NewTerminalMenu`

## Responsibility

- `SessionTerminalTabs`: `NewTerminalMenu`'s button is the last child of the `overflow-x-auto` tab list,
  after agent-conversation tabs. It carries `sticky right-0 bg-background`. The full-screen toggle stays
  outside, at the right edge. The doc comment is updated.
- `NewTerminalMenu`:
  - its menu (`sessions-terminal-tab-new-menu`) renders through a React portal into `document.body`,
    positioned under the button from its bounding rect;
  - outside click counts clicks inside the portal as inside;
  - Escape is unchanged.

## Boundaries

- No change to the menu's entries (n3/n7 add tmux entries; this node must not reorder them).
- No tab reordering and no overflow menu.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n3` tmux-attach | the tmux section inside `NewTerminalMenu` | its menu content is portaled together with the rest | change the tmux section |
| `n1`, `n2`, `n4` | nothing this node uses — the line is flattened | — | — |

## Draft PR contract

The first push publishes:

- the failing tests below.

No new public surface: the change is the placement of existing components. Commit 2 adds the tests only.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — it mounts `SessionTerminalTabs` directly
**Concurrent with:** n1 `dormant-tabs`, n2 `agent-history`, n3 `tmux-attach`, n4 `osc52-clipboard`
**Blocks:** nothing

Real dependency edges, as opposed to the branch line:

    n1 → n6      n2 → n6      n3 → n7

## State A

- The tab list is `flex-1 overflow-x-auto`. "+" follows it, outside it, then ⛶. The menu is absolutely
  positioned under "+".

## State B

- "+" follows the last tab inside the scroller, sticky at its right edge.
- The menu is portaled and never clipped.

## Delta

### Modified
- `packages/tddy-web/src/components/sessions/SessionTerminalTabs.tsx`, `NewTerminalMenu.tsx`.
- Docs, at wrap: `docs/ft/web/session-terminal-tabs.md`.

## Testing Strategy

### Acceptance tests

**`tddy-web`**: `packages/tddy-web/cypress/component/SessionTerminalTabsPlusPlacement.cy.tsx`

- [ ] `"+" follows the Agent tab when it is the only tab`
- [ ] `"+" follows the last terminal tab`
- [ ] `"+" follows the last agent-conversation tab`
- [ ] `"+" stays visible at the scroller's edge when the tabs overflow` (imports `src/index.css`)
- [ ] `the "+" menu is fully visible while the tabs overflow`
- [ ] `Escape and an outside click still close the menu`

The spec imports `src/index.css` once, for all six cases. The last three pass on today's layout,
where "+" is outside the scroller. They are the guarantees the move must keep, and stay green through
it.

### Unit / integration tests

- [ ] The existing `SessionTerminalTabsAgentKindsAcceptance.cy.tsx` menu cases pass unchanged against the portaled menu (a regression guard, no new test).

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Order and portal | Cypress component, CSS-less | Structural |
| Sticky overflow | Cypress component with the stylesheet imported in-spec | Geometry needs Tailwind |

## Decisions & Trade-offs

- **Sticky inside the scroller** keeps the reachability that the outside placement bought.
- **The portal** is the only way to stop `overflow-x: auto` from clipping the menu (CSS clips
  absolutely-positioned descendants of a scroll container).

## Baseline before green

Recorded at the contract commit (2026-11-02), scoped to `tddy-web`:

- **This node's red tests:** `SessionTerminalTabsPlusPlacement.cy.tsx`: the three placement cases fail ("+" is not inside the tab scroller).
- **Guards passing on today's layout, which the move must keep green:** overflow visibility, the unclipped menu, Escape and an outside click.
- No new public surface: commit 2 is the tests alone.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-tab-plus-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-11-02 (reviewed with the stack at the end of pass 2)
- [x] TDD Red — write failing unit/integration tests
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
