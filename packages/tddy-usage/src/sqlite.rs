//! The `sqlite` format: one row per conversation in a table, read through a read-only connection
//! so a running agent's writer is never contended (`#usage-binding 5/5`).

// TODO(opencode-sqlite): implement — open `path` read-only (`mode=ro`, a short busy timeout), select
// the newest root row matching `root`, then its children by `parent`, recursively; present each row
// to the counter expressions as a JSON object of its columns. A busy or locked database is
// "unavailable" (the caller keeps its last snapshot), a missing one is no rows.
