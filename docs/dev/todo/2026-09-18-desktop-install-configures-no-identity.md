# 2026-09-18 — A desktop install configures no identity, so it has no sessions

**Category:** Defect / deferred scope

Source: `./install --desktop` ([2026-09-18-install-desktop.md](../changesets/2026-09-18-install-desktop.md)).
Features [tddy-desktop-tauri.md](../../ft/desktop/tddy-desktop-tauri.md),
[daemon-settings.md](../../ft/daemon/daemon-settings.md).

`desktop.yaml.production` renders a configuration with `github:` unset, `livekit:` unset and
`users: []`. The installed application starts and its dashboard reaches the daemon's settings — and
nothing else. No sessions, no hosts, no screen sharing. The template documents this and says what to
add; nothing automates it.

**Three requirements, and all three must be met together**

- `github:` absent ⇒ `build_auth_entries` returns `user_resolver: None`
  (`packages/tddy-daemon-auth/src/auth.rs`), and **every** session service in
  `tddy_daemon::runtime::build` is assembled inside `if let Some(user_resolver)` — the host registry,
  LiveKit peer discovery, `DaemonSessionHost`, the BSP session resolver, chat workspace roots and
  screen sharing. `DaemonConfigService` is registered outside that block, which is the only reason a
  misconfigured daemon is repairable from its own UI.
- `livekit.api_secret` is the **only** source of the session-token signer, required even with no
  common room. Without it the resolver is `Arc::new(|_| None)` and every token-gated RPC refuses.
  `livekit.enabled` governs the common room alone.
- `users:` maps a login to an OS user with no fallback (`DaemonConfig::os_user_for_github`). An
  unmapped login is refused `permission_denied: user not mapped to OS user` on every session RPC.

**Why it was not fixed with the installer.** Closing it means deciding what identity a
single-operator desktop install has. A stub provider (`github: { stub: true }` plus a `users:` entry
for the installing OS user and a generated `api_secret`) makes the application work on first launch,
at the cost of a sign-in that is an identity rather than a credential check. Requiring a real GitHub
OAuth app keeps the credential check and makes `./install --desktop` produce something unusable until
an operator registers one. That is a posture decision, not an implementation detail, and it is not
the installer's to make silently. `daemon.yaml.production` ships the same three blocks unset for the
served deployment, so whatever is decided should probably be decided for both.

**Worth knowing while deciding**

- The same mismatch is what makes a *peer* unreachable over a common room. A daemon whose `users:`
  does not name the login a peer's token resolves to answers `PERMISSION_DENIED: user not mapped to
  OS user` to a forwarded `ListProjects`, which surfaces in the dashboard as a peer that is present
  in the room and refuses every call.
- Session tokens are stateless HMACs over the shared `api_secret`, so a generated per-install secret
  is correct for a standalone machine and wrong the moment that machine joins a deployment — the
  whole common room shares one secret.
- `github.redirect_uri` is overridden by the application from `listen.web_port`, so whatever is
  chosen must not depend on the operator setting it.
