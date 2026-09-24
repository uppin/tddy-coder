# 2026-09-24 — Sign in to Tddy Desktop with a GitHub device code, no secret and no hand-written user

- **Device-code sign-in.** A desktop configured with the OAuth App's public `client_id` signs in the
  way the GitHub CLI does: click "Sign in with GitHub", open the link, type the short code at GitHub
  and approve. No `client_secret` is configured or shipped. A refusal and an expired code are each
  named, and offer a fresh code.
- **The first account to sign in from the desktop's own window owns the install.** Its GitHub login
  is enrolled against the OS user the application runs as and written into `~/.tddy/desktop.yaml`,
  so `users:` is never edited by hand. A second, different account signs in and is refused on every
  RPC; adding one deliberately is `#keyring` 8/9 ([#515](https://github.com/uppin/tddy-coder/pull/515)).
  A login completed over a LiveKit room or a local socket never enrols. That first write drops the
  file's comments.
- **The dashboard offers only the flow the daemon declares.** A daemon with no GitHub sign-in says so
  rather than showing a button that cannot work. Served deployments with a `client_secret` keep the
  redirect flow unchanged.
- **A strict Content Security Policy** on release builds. It has not yet been checked in a launched
  production build.
- **Not yet zero-configuration out of the box.** `desktop.yaml.production` still ships `github:`
  unset, so a fresh install reports "no sign-in configured" until the OAuth App's `client_id` is
  rendered into it.

See [tddy-desktop-tauri.md § Signing in](../tddy-desktop-tauri.md#signing-in) and
[Cross-daemon session authentication](../../daemon/session-auth.md).

`#keyring` 2/9, [#509](https://github.com/uppin/tddy-coder/pull/509).
