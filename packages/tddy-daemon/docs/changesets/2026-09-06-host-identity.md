# 2026-09-06 — The first host capability probe: git identity and `gh` status
**Type:** Feature

`host_tooling.rs` introduces `HostToolingProbe` — one method, `probe(os_user) -> HostTooling` —
injected through `ConnectionServiceImpl::with_host_tooling` the way `HostStats` is through
`with_host_stats`, so a test never depends on what is installed on the machine running it.
`GetHostTooling` serves it over `ConnectionService`, routed by `daemon_instance_id`.

**Routing runs before authentication**, as it does for the roster RPCs and `ResolveStackBase`: the
token is verified by the daemon that serves the call, and a relay has no business judging a peer's
user mapping. Authenticating first would refuse an operator whose GitHub user maps to an OS user on
the host being probed but not on whichever host their browser is talking to.

Each half of the answer carries a `ProbeOutcome` ahead of its findings, so "could not run" is never
collapsed into "not configured" or "not installed". `classify_gh_auth_status` is a pure function of
`(exit_code, output)` — the classification is the expensive thing to get wrong, and as a pure
function it is provable without spawning anything — and its default arm is `Failed`: **unrecognised
output is a probe failure, never a negative finding.**

Two design points that are easy to lose:

- **`PATH` resolution is the caller's job.** `spawner::resolve_tool_path` deliberately anchors a
  relative program to the daemon's own cwd and never searches `PATH`, so bare `"git"` / `"gh"` would
  exec `<daemon-cwd>/git`. Both programs are resolved up front with
  `spawner::find_program_on_spawn_child_path`, against the same `PATH` the child is given. `None`
  from that lookup is the **only** evidence accepted for "gh is not installed"; a missing `git` is a
  probe failure, and every error after a successful lookup — `ErrorKind::NotFound` included — is a
  failure too.
- **The deadline can end the child.** `spawner::start_output_as_user` hands back the live `Child`
  with its pipes detached; a reader thread gets only the pipes, and the parent `kill()`s **and**
  `wait()`s on timeout, since kill alone leaves a zombie. `run_output_as_user` waits
  unconditionally and returns no handle, so it cannot serve a bounded probe. `as_user_command` now
  holds the setup both spawners share — program resolution, `getpwnam_r`, `HOME`/`PATH`, cwd, the
  privilege drop — rather than a second copy that could drift into running as the wrong user.

`git config --global --get` is used rather than plain `--get`: the probe chdirs into the target
user's home, which is a work tree whenever that user keeps dotfiles in git, and a repo-local
`user.name` there would shadow the identity commits elsewhere on the host actually carry.

Node 4 of the `#hosts-screen` stack — [PR #456](https://github.com/uppin/tddy-coder/pull/456).

See [`host-tooling-probe.md`](../host-tooling-probe.md) and
[`connection-service.md`](../connection-service.md).
