# misplaced-tests: 122 of 139 integration suites belong to other crates

**Location:** `packages/tddy-daemon/tests/`
**Category:** misplaced-tests
**Detected:** 2026-09-15 by structural audit
**Metrics:** **122 of 139** suites · **46,839 of 55,682 lines (84%)** never reach this crate's production code · 13 destination crates
**Restructure:** required — needs a `move_test_binary_to_crate` operation that does not exist yet
**Status:** Open — claimed by #498, in flight
**Claimed by:** #498 — `#carve` 4/10 `test-homes` · draft · `feature/carve/test-homes`
**Supersedes:** `docs/dev/todo/2026-09-15-122-of-tddy-daemons-139-test-suites-belong-to-other-crates.md` — same finding, filed first as a TODO; a standing measurement belongs in this record, and #498 already claims the TODO
**Lands after:** #488, #489, #490

## Measurement history

| Run | Suites | Misplaced | Lines misplaced | Note |
|---|---|---|---|---|
| 2026-09-15 | 139 | 122 | 46,839 | first detection |

## What the tool found

This crate is 58,693 lines, of which **2,377 are production code**. `src/` is twelve files.

`src/lib.rs` is a 30-line facade re-exporting **82 modules** from `tddy-session-lifecycle` under its
own comment — *"Legacy paths for integration suites"* — and that crate re-exports **49** of those
from ten further crates. So a test's import proves nothing about what it exercises:
`tddy_daemon::host_registry` is `tddy-host-service`'s.

Resolving every `tddy_daemon::<module>` through both hops, and separating a test's subject from the
`config`/user-path boilerplate it uses only to stand a fixture up:

| | Suites | Lines |
|---|---:|---:|
| Exercise a **real** daemon module (`runtime`, `server`, `startup`, `daemon_settings`, `daemon_config_service`, `local_socket_server`, `relay_idle`) | **17** | 8,843 |
| Touch only `config` / user-path boilerplate | 60 | 27,095 |
| Touch **no** daemon module at all | 62 | 19,744 |

Destinations: `tddy-session-lifecycle` 97, `tddy-daemon-livekit` 6, `tddy-worktree-service` 4,
`tddy-projects` 3, `tddy-session-files` 2, `tddy-rpc` 2, `tddy-daemon-auth` 2, and one each to
`tddy-session-agents`, `tddy-tool-engine`, `tddy-daemon-kernel`, `tddy-sandbox-runner`, `tddy-core`,
`tddy-service`.

## Why it matters here

- **139 of the workspace's 597 test binaries (23%)** each link a ~55-crate graph to exercise a
  2,377-line library.
- `./test -p tddy-session-lifecycle` proves almost nothing, because that crate's tests are not in it.
- The facade must stay alive for as long as the suites do, which taxes every extraction from
  `tddy-session-lifecycle`.

## What would close it

Rewrite `tddy_daemon::X` → the owning crate's path in the 122 suites, move each file, delete the
facade, and reclassify the dependencies that follow (see
`heavy-dependency-tests-only-runtime-deps.md`). The module→crate map is derivable from the two
`lib.rs` facades.

Needs a new restructure operation: `move_module_to_crate` requires `<crate>/src/<module>.rs`.

**The 17 that stay are correctly placed** — the daemon is the composition root, and a test that
mounts the composition belongs with it.

## If you are about to change this code

#498 moves files and rewrites import headers. It **changes no assertion**.

- **Adding a test to `tddy-daemon/tests/`**: fine, and usually right — #498 will route it to the
  owning crate with the rest. Say which crate it actually exercises.
- **Editing an existing suite**: fine; the move is mechanical and will carry your edit.
- **Relying on `tddy_daemon::` re-export paths in new production code**: don't. #498 deletes the
  facade.

## Verified by hand

2026-09-15: built the module→crate map from both `lib.rs` files and classified all 139 test files by
resolved owner. Separately verified that the apparent `tddy_daemon::` references *inside*
`tddy-session-lifecycle` are log-target string literals, not a reverse dependency.
