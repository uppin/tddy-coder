# 2026-10-22 — host_registry sightings module

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `src/host_registry.rs` **596 → 456**: `apply_sighting`, `apply_departure` and their validation and
  bounding helpers to `host_registry/sightings.rs` (155), by `move_item`.
- Record closed and deleted: `oversized-file-host-registry.md` (596 → 456).
