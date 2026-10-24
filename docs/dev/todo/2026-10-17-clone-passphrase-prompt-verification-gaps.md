# 2026-10-17 — The clone passphrase prompt is not proven end to end across hosts

**Category:** Deferred verification
**Source:** the clone passphrase prompt (2026-10-17) — wrapped with these gaps approved by the developer

Every part of [cloning over SSH](../ft/daemon/clone-ssh-passphrase.md) has a test; three seams between the parts do not.

- **A forward over a real LiveKit room.** A forwarded `StartSession` has no overall deadline since
  `#start-progress` 4/8 (#737): it rides a relayed stream bounded by the relay's 30 s per-frame idle
  timeout, which the peer's whole-life keep-alives (every 5 s) keep from firing on a live start. The relay
  and its peer-answered vs peer-silent split are pinned over a scripted in-memory peer on a paused clock
  (`tddy-daemon-kernel/tests/common_room_forwarding.rs`, `tddy-session-split/tests/split_room_hosting_port.rs`),
  the keep-alive emitter as a paused-clock unit. No test forwards a clone-bearing start to a peer through
  a real transport, so "the desktop does not give up while the operator types" is argued from those, not
  observed. See also [the in-memory peer-forwarding testkit entry](2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md).
- **A real remote host.** The real-process acceptance suite runs real `git`, `ssh`, `sshd` and `ssh-agent`
  on one machine as one OS user. It does not run under `tddy-supervisor`, as a different OS user than the
  daemon, or against a daemon on another host. Under the supervisor the environment reaches the clone only
  through `spawn_policy.allowed_env_keys`, which is documented in the configuration templates and pinned
  against `CloneSshEnv::ALLOWED_ENV_KEYS`, but a supervised clone asking for a passphrase has not been run.
- **A supervisor refusal is not named.** A request refused because a key is missing from
  `allowed_env_keys` reaches the operator as the supervisor's refusal, not as "add these keys". The
  daemon cannot read the supervisor's policy, so naming it means the supervisor reporting which key it
  refused.

`tddy-daemon`'s own test suite was not run for this change (compile and clippy only); the packages it
touches were.

**Why deferred:** the first two need infrastructure (a LiveKit container, a second host or a supervised
install) that a feature PR does not carry, and the third is a supervisor change.
