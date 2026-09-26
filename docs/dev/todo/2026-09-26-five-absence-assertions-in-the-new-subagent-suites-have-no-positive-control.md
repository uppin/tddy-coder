# 2026-09-26 — Five absence-assertions in the new subagent suites have no positive control

**Category:** Test quality
**Source:** `2026-09-26-subagent-turn-control-and-honest-tool-failure` changeset, PR #545 —
`/validate-tests`

PR #545's best assertions are exact counts, for a good reason: both headline behaviours are about
something *not* happening. The flip side is that an absence-assertion with no positive control
passes when the mechanism that would produce the presence is simply broken — and this PR already
shipped two tests with exactly that flaw and had to fix them (`shell_containment_red.rs`'s `cat`
case, which passed against the live defect because the harness's own stdin is at EOF; and the MCP
preview test, whose worktree was never handed to the child).

The five that still lack one:

| Location | Passes wrongly when |
|---|---|
| `tddy-tool-engine/tests/shell_containment_red.rs` — survivor test | the marker would never appear anyway: a filename typo, a wrong cwd, a shell that mishandles `( … ) & wait` |
| `tddy-session-lifecycle/.../jail_relaunch_unit_tests.rs` — canary test | the host tool engine would not have written *that* path regardless |
| `tddy-discovery/tests/subagent_message_ids_red.rs` — "no id minted twice" | the second turn reports **no** messages at all (guarded only by `!seen.is_empty()`, which one id satisfies) |
| `tddy-sandbox-recipes/src/claude_cli.rs` — allowlist test | both tools leak into the no-subagent allowlist (the `assert_eq!` of two `contains` is true when both are `true`) |
| `tddy-discovery/tests/subagent_resume_red.rs` — `a_rewind_never_separates_a_tool_call_from_its_result` | the rewind discarded everything (`0 == 0`). Worse, its helper drops `tool_calls`/`tool_call_id`, so the pairing the test is named for cannot be checked from its own data |

## What closing it would take

One sibling assertion each: run the same command under a budget it *can* meet and assert the
marker appears; write the canary through a non-jailed session and assert it is overwritten; assert
the exact message count; assert `!without_subagent.contains(prompt)`; and compare the rewound
history to a concrete expected vector, as the two sibling tests in that file already do.

## Two timing constants in the same file, both failing in the wrong direction

- `SURVIVOR_GRACE = 3s` — the surviving grandchild touches its marker at spawn + 1s and the
  assertion runs ~3.2s in. A box that delays it past ~2.2s makes the test **pass with the defect
  present**. It also costs a flat 3s, about 88% of that suite's runtime. Asserting the process
  group is gone (`kill(-pgid, 0)` → `ESRCH`) would be both faster and sound.
- `BACKGROUND_AWAIT_MS = 2000` — fails as *"it is still blocked on an inherited stdin"*, which is
  indistinguishable from the real defect, on a path that forks twice. The file's own
  `AMPLE_BUDGET_MS` comment records losing that race once in nine runs at 200ms; this is only 10×
  that, where the cheaper single-fork path was given 25×.
