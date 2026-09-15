# PRD — every test binary moves to the crate it actually exercises

**Date:** 2026-09-15
**Stack:** `#carve` 4/10
**Packages:** `packages/tddy-code-restructuring`, `packages/tddy-daemon`, and the thirteen crates the suites belong to
**Product area:** [`docs/ft/coder/rust-code-restructuring.md`](../../ft/coder/rust-code-restructuring.md)

## Problem

`tddy-daemon` is 58,693 lines, of which **2,377 are production code**. The other 55,682 are **139
integration-test binaries**, and 122 of them do not test this crate.

`tddy-daemon/src/lib.rs` is a 30-line facade re-exporting **82 modules** from
`tddy-session-lifecycle`, under its own comment: *"Legacy paths for integration suites
(`tddy_daemon::connection_service`, …)"*. `tddy-session-lifecycle` re-exports **49** of those from
ten further crates. So a test's import says nothing about what it exercises —
`tddy_daemon::host_registry` is `tddy-host-service`'s, `tddy_daemon::session_room` is
`tddy-daemon-livekit`'s.

Resolving every reference through both hops, and separating a test's subject from the `config` /
user-path boilerplate it uses only to stand a fixture up:

| | Suites | Lines |
|---|---:|---:|
| Exercise a **real** `tddy-daemon` module (`runtime`, `server`, `startup`, `daemon_settings`, `daemon_config_service`, `local_socket_server`, `relay_idle`) | **17** | 8,843 |
| Touch only `config` / `tddy_user_config` / `user_sessions_path` boilerplate | 60 | 27,095 |
| Touch **no** `tddy-daemon` module at all | 62 | 19,744 |

**122 suites — 46,839 lines, 84% of the test code — never reach this crate's production code.**
Counted independently: **0 of 139** name `tddy_session_lifecycle`; **133** name `tddy_daemon::`.

**`tddy-session-lifecycle` has no `tests/` directory at all.** Its 38,629 lines of acceptance
coverage sit in another crate, which is why it looks untested and cannot be verified on its own.

### Why this belongs *inside* the stack, not after it

Every later `#carve` node carves one of these crates. A node that moves production code out of
`tddy-session-lifecycle` while that crate's tests live in `tddy-daemon` has to keep the facade alive
and reason about a second crate's test tree — which is precisely the tax `#carve` 7/9 was already
budgeting for in its AC6. Moving the tests to their code **first** means every node above this one
carves a crate whose tests travel with it.

### The tooling cannot do it yet

`move_module_to_crate` refuses a test binary before it starts: `source_crate_of`
(`crate_move.rs:773`) requires `<crate>/src/<module>.rs`, and `#carve` 1/10's nested fix only extends
that *within* `src/`. Nothing in the package knows about `tests/`.

A test binary is genuinely a different shape, and simpler in one way: **cargo auto-discovers
`tests/*.rs`, so there is no `mod` declaration to find, remove or rewrite.** What it does need is the
`use` header re-pointed and the destination's `[dev-dependencies]` extended.

## What this PR delivers

### FR1 — `move_test_binary_to_crate`

A new operation. Anchor: `<crate>/tests/<name>.rs`. `to`: the destination crate's directory.

| What it does | Why it differs from a module move |
|---|---|
| `git mv` the file to `<dest>/tests/<name>.rs` | same |
| Re-point the moved file's `use` header | same pass, but every path resolves **through up to two facades**, so it uses `defining_crate` from `#carve` 1/10 |
| Nothing to the origin's `lib.rs` | **no `mod` declaration exists** — cargo discovers test binaries |
| Adds what the moved test names to the destination's `[dev-dependencies]` | a module move writes `[dependencies]` |
| No facade, ever | nothing can reference a test binary, so there is no caller to keep resolving |

`reexport` is **refused** rather than ignored: a facade for a test binary would be meaningless.

### FR2 — the 123 misplaced suites move

122 from `tddy-daemon` to thirteen crates, plus
`tddy-workflow-recipes/tests/stack_progress_contract_acceptance.rs` (169 lines, which names only
`tddy_core::changeset`).

| Destination | Suites | Lines |
|---|---:|---:|
| `tddy-session-lifecycle` | 97 | 38,629 |
| `tddy-daemon-livekit` | 6 | 2,318 |
| `tddy-session-files` | 2 | 1,376 |
| `tddy-rpc` | 2 | 902 |
| `tddy-worktree-service` | 4 | 860 |
| `tddy-daemon-auth` | 2 | 804 |
| `tddy-projects` | 3 | 610 |
| `tddy-session-agents` | 1 | 515 |
| `tddy-tool-engine` | 1 | 334 |
| `tddy-daemon-kernel` | 1 | 234 |
| `tddy-sandbox-runner` | 1 | 134 |
| `tddy-core` | 1 | 90 + 169 |
| `tddy-service` | 1 | 33 |

### FR3 — the facade goes, and the dependencies follow

`tddy-daemon/src/lib.rs`'s two `pub use` blocks and its four one-line re-export shims are deleted —
their only stated purpose was those suites.

**17 of `tddy-daemon`'s `tddy-*` runtime dependencies are named by no file in its `src/`** and are
declared in `[dependencies]`, not `[dev-dependencies]`, so every consumer rebuilds them. With the
suites gone they are unreferenced entirely and are removed.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `move_test_binary_to_crate` moves a test binary, re-points its header, and leaves both crates compiling |
| AC2 | It resolves a path through **two** facades to the defining crate, not to the crate that re-exported it |
| AC3 | It refuses `reexport`, naming why a test binary can have no facade |
| AC4 | It refuses an anchor that is not `<crate>/tests/<name>.rs` |
| AC5 | `tddy-daemon/tests/` holds only the **17** suites that exercise its own modules |
| AC6 | `tddy-daemon/src/lib.rs` contains no `pub use tddy_session_lifecycle::` block, and the four re-export shims are gone |
| AC7 | `tddy-session-lifecycle/tests/` exists and holds its own acceptance suites |
| AC8 | No crate declares in `[dependencies]` a `tddy-*` crate that only its tests name |
| AC9 | Whole-workspace test **count** is unchanged — every suite still runs, from its new home |

## Out of scope

- **The 17 suites that mount `runtime`** to stand a whole daemon up for a cross-host acceptance
  test. The daemon is the composition root; a test of the composition belongs with it. They stay.
- Any change to what a test asserts. This node moves files and rewrites import headers.
- `tddy-core`'s 44 suites, which the audit found **entirely correctly placed**.
- `tddy-workflow-recipes/tests/proto_workflow_contracts.rs`, which asserts its own package's `proto/`
  directory exists and can only do so from inside it.

## Why it sits at position 4

It consumes both tooling nodes and nothing else:

- **`#carve` 1/10** — `defining_crate` is exactly the two-hop facade resolution FR1 needs. Without
  it, a moved test's `use tddy_daemon::host_registry` would be re-pointed at
  `tddy-session-lifecycle`, which merely re-exports it, rather than at `tddy-host-service`.
- **`#carve` 3/10** — 123 moves across several plans, and `.restructure/` is per-repository until
  that node keys it by plan.

And every node above it then carves a crate whose tests travel with its code.
