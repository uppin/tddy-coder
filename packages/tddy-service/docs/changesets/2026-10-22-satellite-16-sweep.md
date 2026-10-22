# 2026-10-22 — build.rs main split into build/*.rs

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `build.rs` **743 → 93** production lines, `main` **682 → 20**: by hand, with the developer's consent
  (the restructure engine refuses a `build.rs`: it is no module under `src/`), into four `#[path]` modules
  — `build/core_services.rs` (185), `build/unbundled_families.rs` (236), `build/platform_services.rs`
  (204), `build/descriptors.rs` (105) — with byte-identical bodies, every function at most 57 lines, the
  protos compiled in the same order. The generated `OUT_DIR` was identical before and after (`diff -r`).
  `TODO(restructure-engine)` at the head of `build.rs`.
- Record closed and deleted: `oversized-file-build.md` (743 → 93; `main` 682 → 20).
