# misplaced-tests: privilege_drop

**Location:** `packages/tddy-daemon-kernel/src/privilege_drop.rs` — the whole module
**Category:** misplaced-tests
**Detected:** 2026-09-19 by structural audit
**Metrics:** **5 of 5 tests live in another crate** (`tddy-session-lifecycle/src/pty_runtime.rs`) · 0 `#[cfg(test)]` blocks in the module · 104 production lines · the crate's own 82 tests enter none of it
**Restructure:** not required — ordinary work (move the tests)
**Status:** Open — **unclaimed**
**Verified:** ✅ hand-verified 2026-09-19 — see *Verified by hand*

## Measurement history

| Run | Tests in owning crate | Tests in consumer crate | Note |
|---|---|---|---|
| 2026-09-19 | 0 | 5 | first detection |

## What the tool found

Every test of `privilege_drop` is in `tddy-session-lifecycle`, which re-exports the module's five
symbols (`pty_runtime.rs:23-26`) and tests them through that re-export:

```
pty_runtime.rs:175  user_env_overrides_set_home_to_the_target_user_home
pty_runtime.rs:194  user_env_overrides_prepend_the_user_path_extra
pty_runtime.rs:215  no_privilege_drop_for_the_daemons_own_user
pty_runtime.rs:225  privilege_drop_required_for_a_different_user
pty_runtime.rs:236  wraps_the_command_behind_a_setpriv_privilege_drop_launcher
```

`privilege_drop.rs` has no `#[cfg(test)]` block. `./test -p tddy-daemon-kernel` runs 82 tests and
enters none of this module.

**This is a leftover of a move, not a choice.** The module's siblings say so: `pty_runtime.rs:9-12`
records that the impersonation logic "stays in `tddy-daemon`" while the derived helpers went to the
kernel. The code moved; its tests did not.

## Why it matters here

Two concrete consequences, both checkable:

- **The owning crate's gate does not cover its own code.** A change to `privilege_drop.rs` verified
  with `./test -p tddy-daemon-kernel` — the scoped command this repo's verification policy asks for,
  and the one a contributor touching this crate would naturally run — passes without executing a
  single line of the changed module. The tests that would have caught the regression are in a
  package the contributor had no reason to name.
- **The tests describe PTY runtime behaviour, not the helpers.** Their names are about
  `pty_runtime`'s concerns (`no_privilege_drop_for_the_daemons_own_user`), so a reader of
  `privilege_drop.rs` looking for its contract finds nothing, and a reader of the tests reasonably
  concludes they are testing `tddy-session-lifecycle`.

This compounds with `missing-tests-privilege-drop-resolve-pty-os-user`, which records that the one
symbol none of these five reaches is the uid resolver itself. Together: the owning crate tests none
of the module, and the consumer crate tests four fifths of it.

## What would close it

Move the five tests into a `#[cfg(test)]` block in `privilege_drop.rs` and rename them for the
symbols they pin rather than for the caller's scenario. `tddy-session-lifecycle` keeps whatever tests
are genuinely about `PtyRuntime`'s own wiring — the point is not to strip that crate, it is that the
helpers' contract belongs beside the helpers.

Ordinary work: no restructure assist, no manifest change, and the module has no dependencies the
kernel crate lacks. Worth doing in the same change as the `resolve_pty_os_user` tests, since both
land in the same new `#[cfg(test)]` block.

## Verified by hand

**2026-09-19.** Checked:

- Located `#[cfg(test)]` in `pty_runtime.rs` (`:166`) and confirmed all five test functions sit
  after it; read each `#[test]` attribute rather than inferring from the call lines.
- Confirmed `privilege_drop.rs` contains no `#[cfg(test)]`.
- Ran `./test -p tddy-github -p tddy-daemon-kernel`: 112 passed, 0 failed — 82 of them the kernel's
  (78 unit + 4 acceptance), none naming a `privilege_drop` symbol.

**What this record is not claiming.** Testing through a re-export is not wrong in itself, and these
five tests are good tests — they are Given/When/Then, they assert on behaviour, and they pass. The
finding is about *where the contract is pinned*, which is what decides whether the owning crate's
scoped gate is meaningful. A re-run that finds the tests still there should keep this record open
even if the tests have improved.
