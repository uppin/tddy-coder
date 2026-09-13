# 2026-09-09 — The identity boundary

**Type:** Architecture

New crate, added by node 4 of the `#unbundle` stack
([#473](https://github.com/uppin/tddy-coder/pull/473)). Full story in the cross-package entry:
[2026-09-09-unbundle-auth-livekit.md](../../../../docs/dev/changesets/2026-09-09-unbundle-auth-livekit.md).

Seven modules and 2,145 production lines leave `tddy-daemon`: `auth`, `github_token_store`,
`github_pr_credentials`, `codex_oauth_relay`, `oauth_loopback_tunnel`,
`codex_oauth_participant_metadata`, `token_provider`. Four proto services come with them —
`auth.AuthService` (5), `auth.LiveKitTokenService` (1), `token.TokenService` (2),
`loopback_tunnel.LoopbackTunnelService` (1) — and **every one was already its own proto**, so no
wire coordinate moved and no client migrated.

**A striking amount of weak coupling behind a large line count.** This subsystem's *entire*
dependency on the 23,099-line god module was one type alias, `auth.rs:25: use
crate::connection_service::SessionUserResolver`. Node 1 moved that alias into
`tddy-daemon-kernel`, and auth became free-standing — which is why the identity boundary could be
drawn in an early node.

**`AuthBuildResult::user_resolver` is the daemon's single identity function.** Every other service,
in every other crate, authenticates with a clone of it. It is `Option` because a daemon with no
GitHub configuration has no way to resolve a token, and in that state the wiring layer registers
**no session services at all** — a deliberate refusal rather than an oversight.

**One secret signs two things.** `config.livekit.api_secret` signs both LiveKit room JWTs and
session tokens, through `tddy_github::SessionTokenSigner`. Splitting auth from LiveKit into two
crates does not split that secret, and neither crate may derive its own. The enforcement is
structural: `tddy-daemon-livekit` reaches minting through a `SessionTokenMinter` **port**, and its
`dependency_boundary_unit.rs` pins this crate off its dependency path.

**The secret store now writes atomically**, through
`tddy_core::atomic_file::write_atomic_with_mode` — the mode-aware variant, because plain
`write_atomic` carries permissions over from an *existing* target and a **first** write would
create the swap file at the process umask and publish a world-readable credential. This was
in scope as a deliberate exception to "move only": shipping the identity boundary with a known
truncate-in-place path at its centre would be worse than a slightly larger diff, and the fix is a
call to an existing helper rather than new machinery.

⚠ **Its test does not discriminate the change.** The base already staged to `<tokens>.tmp` and
renamed, and that staged create also fails in a `0o555` directory — so reverting the refactor would
leave the test green. It is an honest guard against a *future* truncate-in-place, not evidence for
the fix.

⚠ **`ensure_owner_only_dir` changed behaviour twice, and only one half was intended.** It builds
with `DirBuilder::recursive(true).mode(0o700)` instead of `create_dir_all` plus an unconditional
`set_permissions(0o700)`. *Intended*: an existing storage directory no longer has `0o700` re-imposed
on every write, so the daemon stops overruling an operator's deliberate `chmod` — at the cost that
an `auth_storage` currently group- or world-readable **stays that way after upgrade**. *Unintended*:
the mode now applies to every directory the call creates, not just the leaf, so with
`auth_storage = /var/lib/tddy/auth` and no `/var/lib/tddy`, that parent is created `0700` and owned
by the daemon user where it previously took the process umask. Recorded in
[auth-service.md](../auth-service.md) rather than silently accepted.

**Log targets still say `tddy_daemon::…`** — `auth`, `codex_oauth`, `github_token_store`,
`oauth_tunnel`. Kept deliberately: renaming them would silently break every operator's `RUST_LOG`
filter. Node 1 set the same precedent.

**The crate is smaller than "auth" suggests.** The host-key path — `host_keypair`,
`host_private_key`, `ssh_agent`, `ssh_agent_add`, 1,994 lines — went to `tddy-host-service` in node
1, because `AddHostKey` and `ListHostKeyCandidates` are host-service methods. The boundary is real:
what is here signs and verifies; what left unlocks and loads.

`tddy-integration-tests` reaches `codex_oauth_relay` through this crate, and its `tddy-daemon`
dependency is gone entirely rather than merely joined.

Tests: `auth_service_acceptance` (6), `token_service_acceptance` (each JWT verified with
`livekit_api::access_token::TokenVerifier`, plus a server holding a *different* secret refusing the
same token — without which the positive cases assert nothing), `cross_crate_session_token_acceptance`
(2, against `tddy-service` deliberately, since this crate cannot reach the daemon's own services),
and `dependency_boundary_unit` (3, including one asserting the manifest walk actually reaches
`tddy-daemon-kernel`, so a walk that silently found nothing cannot pass as a clean result).
