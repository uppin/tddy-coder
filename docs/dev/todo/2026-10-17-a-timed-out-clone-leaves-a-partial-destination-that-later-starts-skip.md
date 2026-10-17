# 2026-10-17 — A timed-out clone leaves a partial destination that later starts skip

**Category:** Future enhancement (defect)
**Source:** `2026-10-17-clone-passphrase-prompt` (clone passphrase prompt)

- `clone_as_user_with_env` is `test -e "$1" || git clone …`. When the caller's
  `tokio::time::timeout` fires, the blocking task is abandoned and the `git clone` child keeps running;
  if it was killed or failed part-way, `<repos>/<name>` exists, so the **next** start sees the directory
  and skips the clone, reporting a half-written repository as present.
- **Why deferred:** the clone passphrase prompt removes the dominant cause (an indefinite hang) by
  failing fast, which makes a partial destination rare rather than impossible. A correct fix needs a
  decision about ownership — clone into a sibling temp directory and rename on success, or remove the
  destination on failure — and about killing the abandoned child, which `Command::output()` does not
  allow. That touches the same `spawner.rs` that is already 2,152 lines (see
  [the tddy-spawn budget TODO](./2026-09-10-tddy-spawn-modules-are-over-budget-and-not-yet-reusable.md)).
- Found by reading the code while diagnosing the 2026-10-17 passphrase hang on `udoo`. It was **not**
  observed there: `/home/mantas/repos` did not exist after the timed-out attempt. Whether a killed
  `git clone` leaves a directory behind is therefore unconfirmed on the failing path.
