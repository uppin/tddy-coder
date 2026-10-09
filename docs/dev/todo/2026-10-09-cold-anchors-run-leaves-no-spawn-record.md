# 2026-10-09 — A cold `restructure anchors` leaves no record of the server it tried to start

**Category:** Future enhancement (observability)
**Source:** #reshape 12/19 (`feature/reshape/anchors-outline`), discovery Exploration 2

`ColdRunSpawnRecord` (`packages/tddy-code-restructuring/src/spawn_record/deferred.rs:23-58`) defers
every line until the run's `.restructure/` directory exists, so that a run that writes nothing leaves
nothing behind. `anchors` is read-only and never creates that directory. A cold `anchors` whose
rust-analyzer failed to start, or exited mid-run, therefore leaves no spawn record: there is no
program, argument list, environment name list or exit outcome to read afterwards. Only the refusal on
stderr remains. After #reshape 12 that refusal names the cause of a failed *start*, but it does not
name the cause of a server that came up and later died.

What would close it is one of these:
- a record location that a read-only command may write (for example under `$TMPDIR`, keyed by
  workspace, the way the index daemon keeps `tddy-index-*.spawns.jsonl`);
- flushing the deferred lines to stderr when a cold read-only run fails.

**Why deferred.** It decides where a read-only command may write, which is a policy question. Not
needed to close the empty-outline record.
