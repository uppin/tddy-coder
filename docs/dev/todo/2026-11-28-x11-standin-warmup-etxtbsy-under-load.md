# 2026-11-28 — The x11 stand-in's warm-up can hit `ETXTBSY` when the suite runs under heavy parallel load

**Category:** Known failing test
**Source:** `tddy-screen-sharing`'s `x11_channels_acceptance` suite, seen twice while the
`#fs-port` stack's workspace-wide gates ran under a parallel compile storm (PR #1066's
lead-up); passes every time in isolation and under ordinary load.

- `StandInX11vnc::installed_in` (`packages/tddy-screen-sharing/tests/common/mod.rs`) writes the
  stand-in `x11vnc` shell script with `std::fs::write`, makes it executable and immediately execs it
  in `warm_up` — and `warm_up`'s `Command::status()` came back
  `Os { code: 26, kind: ExecutableFileBusy, message: "Text file busy" }` twice, three tests at a
  time, while two cargo processes were compiling concurrently. Each test builds its stand-in in its
  own `tempfile` temp dir, so the file is not shared across tests; which process held the write end
  at the moment of the exec was not established.
- **Fix shape:** establish the holder first (strace or an fd audit across the parallel runs), then
  make `warm_up` tolerate the race it actually has — most likely by retrying the exec briefly, or by
  writing the stand-in through a path that cannot be exec'd mid-write.
- **Why deferred:** reproduced twice under a load profile (two concurrent cargo processes over one
  `target/`) that a CI run does not produce — every CI run of the suite, and repeated isolated and
  load-runs afterwards (3× isolated, 1× with a parallel `clippy -p tddy-daemon` running), passed.
  The developer judged the race not worth the troubleshooting now; revisit if a CI run ever shows it.
