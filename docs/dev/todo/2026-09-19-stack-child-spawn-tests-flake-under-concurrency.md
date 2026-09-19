# 2026-09-19 — `a_child_started_from_the_dialog_is_told_to_read_its_changeset` flakes in the full lib run

**Category:** Known failing test
**Source:** observed while merging `origin/master` into PR #518; **not caused by that PR**

`packages/tddy-session-lifecycle/src/connection_service/stack_child_spawn_tests.rs:395`

```
assertion `left == right` failed: the dialog's child must be pointed at its changeset too
  left: None
  right: Some("Read your changeset at artifacts/attachments/changeset.md before writing code — it
               states this PR's responsibility, its boundaries, and what each dependency delivers.")
```

## It is a concurrency flake, not a defect in the assertion

Measured on one unchanged tree:

| How it was run | Result |
|---|---|
| `cargo test -p tddy-session-lifecycle --lib` | **252 passed / 2 failed** |
| the same command again | **253 passed / 1 failed** |
| `--lib a_child_started_from_the_dialog_is_told_to_read_its_changeset`, 5 consecutive runs | **5 / 5 pass** |

It passes every time in isolation and fails intermittently beside its siblings, and the *count*
varies between runs — so at least one other test in that lib is non-deterministic too. The
`left: None` says the spawn produced no changeset instruction at all, which is the shape of a
fixture or a shared store being observed mid-write by a concurrent test rather than of a wrong
expectation.

## Not attributable to PR #518 or to the merge

- `git diff 60360a17..HEAD -- '*stack_child_spawn*'` — **empty**; the branch never touches the file.
- No commit in `60360a17..origin/master` (#507, #490 `#carve` 3/10, #498 `#carve` 4/10) touches it.
- The file is present and unchanged on `master`.
- Subject matter is unrelated: PR-stack child sessions and changeset attachments, not codebase
  placement or jails.

## What would close it

Find the shared state. `#[cfg(test)] mod stack_child_spawn_tests` lives inside
`connection_service`, so its siblings share whatever that module's fixtures build — a temp dir, a
session store, or a process-global. The candidates are a fixed path two tests both write, or a
`OnceLock`/`static` one test sets and another reads.

Until then, a single failure of this test in a full-lib run is **not** evidence of a regression in
the PR being tested, and a green run is not evidence that it was fixed.
