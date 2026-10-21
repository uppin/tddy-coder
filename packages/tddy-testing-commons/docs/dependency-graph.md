# `dependency_graph` — the workspace's crate graph

`tddy_testing_commons::dependency_graph::WorkspaceGraph` is the one place the workspace's crate
dependency graph is read, for the tests that hold an architectural boundary.

It runs `cargo metadata --no-deps` and reads each member's dependency list and `bin` targets, so a dependency is seen
however its manifest spells it: inline, as a `[dependencies.name]` sub-table, or inherited with
`{ workspace = true }`. A manifest walker that parses TOML by hand misses the last two.

## What counts

A crate **names** another when it lists it under `[dependencies]` or `[build-dependencies]`,
optional dependencies included: an optional dependency is one the crate can be built with.
`[dev-dependencies]` are **not** counted, because a test may reach anything, so a boundary suite
that must hold "no test reaches it either" cannot use this graph.

## Surface

| Method | Answers |
|---|---|
| `WorkspaceGraph::load()` | The workspace this checkout belongs to. |
| `WorkspaceGraph::of_workspace(root)` | The workspace whose root manifest is `root/Cargo.toml`: for a fixture workspace. |
| `workspace_crates()` | Every workspace member. |
| `names_directly(krate)` | What the member names, workspace members and registry crates alike. **Panics** when `krate` is not a member, so a misspelled name cannot read as a crate that names nothing and make every "does not name X" check pass. |
| `crates_building(binary)` | The workspace members with a `bin` target named `binary`; empty when none builds it. A caller compares it with the crate it expects, so a misspelled binary fails there. A library of the same name does not count: only `bin` targets do. |
| `reaches(krate)` | Everything reachable by following what each crate names, `krate` excluded (a back-edge never reaches itself). Registry crates are reached but not followed. |

## Who uses it

- `tddy-daemon-extensions/tests/livekit_is_a_satellite.rs`: the
  [ratchet](../../tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet)
  that holds which crates may name a LiveKit crate, and which wiring crate builds each client binary.
- `tddy-daemon-livekit/tests/dependency_boundary_unit.rs`: the crate does not name or reach
  `tddy-daemon` or `tddy-daemon-auth`, and its walk does reach `tddy-daemon-kernel`, so "reaches
  nothing" is not the result of a walk that found nothing.
- `tddy-daemon-auth/tests/dependency_boundary_unit.rs`: the same rule for `tddy-daemon`.

A boundary suite pairs each absence check with a presence check on the same walk.

## Tests

The module's unit tests build a fixture workspace in a temporary directory that spells dependencies
every way a manifest can, and pin: the member list; inline, sub-table, optional and build
dependencies named while a dev dependency is not; a workspace-inherited dependency; a crate reached
only through another; a registry crate reached but not followed; a crate that never reaches itself
through a back-edge; a dev dependency not followed to what only it names; the panic on a crate
that is not a member; and a binary built by the crate with the `bin` target, not by a library of the
same name.
