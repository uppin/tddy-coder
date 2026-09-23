# The crate's test suites

`tests/` holds **59 integration suites**. Together they are this crate's acceptance coverage:
session start, resume, split and deletion; the sandboxed Claude and Cursor CLI paths; terminal
control and replay; staging, uploads and session files; peer and cross-host forwarding; and the
`DaemonRpcFamilies` port (`rpc_families_port_acceptance.rs`: an unwired host reports the missing
wiring, a wired one hands back what it was given). The Telegram control surface and its notifier are
tested where they live, in [`tddy-telegram-control`](../../tddy-telegram-control/README.md), and the
Project, Catalog, ExecTool and PR-stack families' suites where those families live, in
[`tddy-daemon-rpc`](../../tddy-daemon-rpc/docs/architecture.md#tests).

`./test -p tddy-session-lifecycle` therefore exercises the crate. That is the point of them being
here: the same suites, reaching the same code through `tddy-daemon`'s re-export facade, left the
crate looking untested from inside it and untestable on its own — the worst property a coverage gap
can have, because nothing in the crate shows it.

## Writing one

- A suite goes in the crate whose production code it exercises. `tddy-daemon` keeps only the suites
  that mount the composition root — see [its rule](../../tddy-daemon/docs/test-placement.md).
- Import what the suite exercises **from the crate that defines it**. `tddy_session_lifecycle::…`
  for this crate's own modules; the owning crate directly for anything this crate re-exports.
- A suite that opens a session room without exercising the four families served above this crate
  installs `test_util::RpcFamiliesNotUnderTest` with `with_rpc_families`. A suite that asserts one
  of those families — through a room or a stack link — belongs in `tddy-daemon-rpc`, whose
  `test_util::TestDaemon` installs the real `RpcHandlers`.
- There is no `tests/common/` module. The PTY wait the CLI suites share is
  `tddy_testing_commons::wait::a_capture_showing`, with its `PTY_STUB_OUTPUT` ceiling, so the
  Telegram start suites in `tddy-telegram-control` use the same one.
- Crates a suite needs go in `[dev-dependencies]`, never `[dependencies]` — a crate only a test
  needs is not one the library needs, and declaring it in `[dependencies]` makes every consumer
  rebuild it.
