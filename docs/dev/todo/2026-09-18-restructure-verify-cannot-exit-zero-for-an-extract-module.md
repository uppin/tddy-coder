# 2026-09-18 — `restructure verify` cannot exit zero for an `extract_module`

**Category:** Future enhancement
**Source:** `#carve` 2/10 `/green`, [#489](https://github.com/uppin/tddy-coder/pull/489), changeset
[`2026-09-15-carve-recipe-parsers`](../1-WIP/2026-09-15-carve-recipe-parsers.md)

`tddy-tools restructure verify --against HEAD` compares trimmed statements as multisets and excuses
only the scaffolding a restructure is supposed to churn. `verify.rs::is_structural` normalises a
leading `pub(crate) ` **only** on `use` / `mod` / `impl` / brace lines — so the two things
`extract_module` always does are never excused:

- a widened field or function (`file: String,` → `pub(crate) file: String,`), and
- the `rustfmt` reflow that widening forces once a line stops fitting.

On this node that was **205 lost / 235 gained**, exit 1, of which:

| Class | Count |
|---|---:|
| `pub(crate) ` widenings — 114 private `Structured…`/`…De` mirror fields, 20 unreflowed hook signatures | 134 |
| reference re-points (`before_interview(…)` → `before::before_interview(…)`) | 39 |
| `rustfmt` reflows of lines the two above made longer | 32 |

Nine `fn` signatures split across lines, six `match` arms gained a block, and the continuation lines
of emptied `use` groups are unfiltered because only a group's *first* line starts with `use`.

## Why this matters more than a noisy exit code

The one **real** finding on this node was a single line inside those 440: the banner comment
`// ── evaluate-changes output types ──`, attached to no item, which rust-analyzer therefore carried
nowhere. Nothing else could have caught it — not the compiler, not the suite, not a diff of the moved
lines. That is exactly what `verify` is for, and it arrived buried in 439 entries an author has to
classify by hand before they can see it.

So today the gate is "read both lists and account for every entry", not "the command exited zero" —
which is a gate that scales badly and that a tired author will skip.

## What closing it would take

Normalise a leading `pub(crate) ` (and `pub `) on **any** line in `is_structural`, not only a
structural one, and treat a statement that differs from a lost one by a module prefix as a re-point.
That collapses 173 of the 440 immediately. The reflows need line-joining before comparison —
normalise each side into whole statements rather than physical lines — which is the larger half of
the work and the part worth designing rather than patching.

Until then, `verify`'s exit code is not a signal for any plan that widens visibility or re-points a
reference; the two lists are.
