# 2026-08-03 — tddy-supervisor — deliberate gaps and follow-ups

**Category:** Future enhancement
**Source:** tddy-supervisor changeset, wrapped 2026-08-03

**Session types that still spawn from the daemon**, and therefore run as the daemon user on a supervised
host:

- **Sandbox sessions.** `SpawnSandbox` exists and builds a real jail, but `sandbox_session.rs` still
  calls `tddy_sandbox_cgroups::spawn_plan` in-process. Routing it needs the daemon's session bridge to
  stop depending on the child's *piped stdio* (`bridge_sandbox_stdio` needs `take_stdio()`), which is
  why the wire contract chose a `--grpc-uds` path over fd-passing.
- **claude-cli, cursor-cli and PTY sessions.** `pty_runtime.rs` drops privilege by shelling out to
  `setpriv --reuid`, which an unprivileged daemon cannot do. This is the one place the "paths, not fds"
  decision does not stretch: routing them needs the pty master fd over `SCM_RIGHTS`.

**`SignalSession` will fail with `EPERM` on a supervised host.** `connection_service.rs` calls
`libc::kill(pid, sig)` directly, which an unprivileged daemon cannot do to a session running as another
user. `SIGTERM`/`SIGKILL` map onto the supervisor's `stop_session`; **`SIGINT` has no equivalent**, so
closing this needs a protocol decision (add a `SignalSession` rpc, or accept TERM/KILL only) rather than
a patch.

**A session handle on the wire.** `SessionRef` names a session by pid, so when the kernel reissues a pid
the displaced session's retained exit status becomes unreachable even though it is the answer a poller
wants. The supervisor already carries a generation counter internally and logs a `WARN` when this
happens; closing it means putting that generation in `SpawnedProcess`/`SessionRef` — a wire change worth
making only when Milestone 6's remaining paths need it. `TODO(supervisor)` in `supervisor.rs`.

**The AppArmor userns grant has to move, not disappear.** The PRD originally claimed it became
unnecessary "because the supervisor is root". That does not follow: the supervisor drops to the target
uid *before* `unshare(CLONE_NEWUSER)`, so at that moment the process is unprivileged and the label in
force is the one attached at exec of `tddy-supervisor`. On a host with
`kernel.apparmor_restrict_unprivileged_userns=1` the grant must therefore exist for the **supervisor**
binary — a `packages/tddy-supervisor/apparmor/tddy-supervisor` profile, with a test pinning which binary
carries it. `install` still renders and loads the existing `tddy-daemon` profile, correctly: it is
path-attached, and the daemon keeps its own non-brokered jail path.

**`Supervisor::shutdown` signals sessions by pid, not by process group.** Every child now leads its own
group, so group-signalling there would also reach a session's own descendants — the same argument that
justifies it in `stop_session`. Those surviving descendants are exactly what makes a cgroup scope
`EBUSY` at the worst moment.

**Unvalidated inputs, both wanting a decision rather than a patch:** a sandbox mount's `target` is not
checked absolute or traversal-free (it names a path inside the jail's own namespace, which is why no
test pins it), and a session spawn's `working_dir` is `chdir`'d after the privilege drop — so it is
traversed with the target user's authority, the important part — but is not matched against any
allowlist the way `tool_path` is.

**Recursive read-only bind mounts.** A read-only `BindMount` remounts only the top mount; submounts
beneath it stay writable. Closing it wants `mount_setattr(AT_RECURSIVE)`. `TODO(supervisor/jail)` in
`spawn_broker.rs`.

**Duplication worth folding, all three deliberate for now:** `apply_socket_ownership` and the
create-dir/unlink/bind/chown sequence exist in both `server.rs` and `supervisor.rs` and want to move to
`socket.rs` (whose header currently advertises itself as pure, so that claim needs amending); the
single-path-component check exists as both `policy::scope_dir` and `cgroup_broker::names_one_directory`
and must keep its two different error types (opaque `Denied` for a caller's scope name, `Invalid` naming
the key for root's own config); and `spawner.rs`'s `clone_as_user`/`run_capture_as_user` still carry
their own `getpwnam_r` copies now that `resolve_target_account` exists.

**Packaging and docs:**

- **`./publish.sh` does not ship the supervisor.** It builds a `.deb` installing binaries plus a systemd
  unit into `/lib/systemd/system` and knows nothing about `tddy-supervisor` or `supervisor.yaml`, so a
  `.deb`-installed host gets the old daemon-only deployment.
- **`docs/ft/daemon/systemd-install.md` documents neither `--user` nor `--headless`** (neither side of
  the rebase that introduced `--user` added them), so "`--user` means no supervisor" currently lives
  only in the `install` header.
- **`INSTALL_NO_SYSTEMCTL=1` does not gate the file writes** — binaries, both configs and both units are
  still written to the four `INSTALL_*_DIR` destinations, which must therefore also be overridden for a
  test install. Gating them was rejected because it would hollow out
  `install_fails_when_config_lists_codex_acp_without_native` and its sibling, which assert only a
  non-zero exit and would then pass for a different reason. The header's promise was narrowed instead.
- **The socket-path drift check warns rather than fails.** Install cannot repair a preserved operator
  config without overwriting it, and a mismatch can be benign since the daemon adopts the passed fd
  regardless of `local.socket_path`.
- **Two codex-acp install tests pass for the wrong reason.** They do not override `INSTALL_BIN_DIR`, so
  the mode-hardening step aborts them at the real `/usr/local/bin` before the codex-acp check they are
  named for. They assert only a non-zero exit, so they still pass. The fix is to give them the four
  `INSTALL_*_DIR` overrides every other install test uses.

**`detect_and_prepare_base`'s process-global `OnceLock`** (in `tddy-sandbox-cgroups`, the no-supervisor
path) means a cgroup topology change requires a restart. The supervisor's own preparation deliberately
has no such cache.
