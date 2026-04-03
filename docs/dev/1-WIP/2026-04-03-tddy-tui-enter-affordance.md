# Changeset: TUI mouse Enter affordance (3×2 ASCII, status + prompt)

**Date**: 2026-04-03  
**Status**: ✅ Complete (ready to wrap)  
**Type**: Feature

## Affected Packages

- **tddy-tui**: `mouse_map::enter_button_rect`, `render::paint_enter_affordance`
- Feature docs: [docs/ft/coder/tui-status-bar.md](../../docs/ft/coder/tui-status-bar.md), [docs/ft/web/web-terminal.md](../../docs/ft/web/web-terminal.md)
- Package history: [packages/tddy-tui/docs/changesets.md](../../packages/tddy-tui/docs/changesets.md)

## Related Feature Documentation

- [TUI status bar — Mouse mode: Enter control](../../docs/ft/coder/tui-status-bar.md#mouse-mode-enter-control)

## Summary

Mouse mode draws a **3×2** Enter affordance: ASCII `+--` on the row **above** the first prompt line (typically the **status** row) and `|`, U+23CE, pad on the **first prompt** line—so a **one-line** prompt still shows the control without extra `prompt_h`. Hit-testing uses the same rect.

## Scope

- [x] `tui-status-bar.md` / `web-terminal.md` / `packages/tddy-tui/docs/changesets.md`
- [x] `mouse_map.rs` + `render.rs` + tests

## Acceptance Tests

- `./dev cargo test -p tddy-tui` — all pass

## Technical Notes

- Paint runs after status and prompt `Paragraph`s; affordance overwrites the bottom-right 3 columns on the status row and the first prompt line.
- Skip when `prompt_bar.y == 0` or width &lt; 3.

## PR-wrap validation (2026-04-03)

| Check | Result |
|--------|--------|
| `cargo fmt -p tddy-tui` / `--check` | ✅ |
| `cargo clippy -p tddy-tui --all-targets -- -D warnings` | ✅ |
| `cargo test -p tddy-tui` | ✅ (115 + integration crates) |
| Risk review | Low: localized layout/paint; `enter_button_rect` aligned to `prompt_bar.y - 1` |
| Tests | Assert straddle geometry + buffer `+--` / `\|` / U+23CE |
| Prod-ready | No `FIXME`/`TODO` in `mouse_map`/`render` affordance paths |

**Note:** Full workspace `cargo fmt --check` may fail if other packages have unformatted files (e.g. `tddy-integration-tests`); run `./dev cargo fmt` at repo root before commit if required by CI.
