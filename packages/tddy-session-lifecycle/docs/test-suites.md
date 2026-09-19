# The crate's test suites

`tests/` holds **95 integration suites** and a shared `tests/common/mod.rs`. Together they are this
crate's acceptance coverage: session start, resume, split and deletion; the sandboxed Claude and
Cursor CLI paths; terminal control and replay; staging, uploads and session files; the Telegram
control surface and its notifier; peer and cross-host forwarding.

`./test -p tddy-session-lifecycle` therefore exercises the crate. That is the point of them being
here: the same suites, reaching the same code through `tddy-daemon`'s re-export facade, left the
crate looking untested from inside it and untestable on its own — the worst property a coverage gap
can have, because nothing in the crate shows it.

## Writing one

- A suite goes in the crate whose production code it exercises. `tddy-daemon` keeps only the suites
  that mount the composition root — see [its rule](../../tddy-daemon/docs/test-placement.md).
- Import what the suite exercises **from the crate that defines it**. `tddy_session_lifecycle::…`
  for this crate's own modules; the owning crate directly for anything this crate re-exports.
- `tests/common/mod.rs` is shared fixture code, declared with `mod common;` by the binaries that use
  it. It is a module of each of those binaries, not a test binary of its own.
- Crates a suite needs go in `[dev-dependencies]`, never `[dependencies]` — a crate only a test
  needs is not one the library needs, and declaring it in `[dependencies]` makes every consumer
  rebuild it.
