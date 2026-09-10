# 2026-09-10 — The dependency-boundary harness is duplicated in every extracted crate

**Category:** Deferred refactor
**Source:** `#unbundle` node 4, [#473](https://github.com/uppin/tddy-coder/pull/473)

`packages/tddy-daemon-auth/tests/dependency_boundary_unit.rs` and its `tddy-daemon-livekit` twin
carry ~100 identical lines each: a walk over the transitive manifest closure asserting that
`tddy-daemon` is absent from the crate's dependency path. It is the check that proves an extraction
is real rather than a re-export, so **nodes 5–8 will each add another copy** — five or six by the
time the stack lands.

Lift it into `tddy-testing-commons` as one helper taking the crate under test and the crates that
must not appear.

Two latent blind spots travel with every copy, both harmless against today's tree and both worth
fixing once rather than six times:

- a **table-form** dependency (`[dependencies.tddy-daemon]` with `path` on its own line) is skipped
  by the parser, so a dependency declared that way would pass the check;
- a **workspace-inherited** path dependency (`{ workspace = true }`) is never followed, so a crate
  reached only through workspace inheritance is invisible to the walk.

The `tddy-daemon-auth` copy has a third test asserting the walk actually reaches
`tddy-daemon-kernel`. Keep that idea in the lifted helper: without it, a walk that silently found
nothing passes as a clean result.
