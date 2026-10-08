# 2026-10-08 — the supervisor-declared host socket has never run under a real root supervisor

**Category:** Unverified production path
**Source:** `#keyring` 9/9 (PR #516), deferred with the developer's consent

The supervisor creates each declared session user's host socket (directory `0700`, socket `0600`, owned by that user) and hands it to the unprivileged daemon from fd 4 onward. Unit tests ran with the current user standing in for the target user, so these are unproven:

- the `chown`/`fchown` to a *different* uid;
- the unprivileged daemon accepting on a socket it does not own;
- the real fork/exec fd handover through a root supervisor;
- the three Linux-only tests in `packages/tddy-supervisor/tests/supervisor_socket_handoff.rs`, which compile but were never run on Linux by the author;
- a sandboxed tool session reaching the socket (the declared path would need a bind mount into the jail);
- the survived mutation: skipping the final `fchown` of the directory to the user.

Close it by running the stack under a root supervisor with two accounts and a session for each, asserting the other account's socket refuses a connection.
