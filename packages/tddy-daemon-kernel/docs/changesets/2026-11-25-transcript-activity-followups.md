# 2026-11-25 — A hooks turn end drops unpaired transcript calls

**Type:** Fix

`ToolTurns::hooks_turn_ended` drops unpaired transcript calls with the turn, so leftovers from a lost first turn or a re-read line never pair with the next turn's hooks and misorder its opening text (n3). Full record: [`docs/dev/changesets/2026-11-25-transcript-activity-followups.md`](../../../../docs/dev/changesets/2026-11-25-transcript-activity-followups.md) (#1037).
