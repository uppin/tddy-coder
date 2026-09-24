# 2026-09-24 — The install template ships the Tddy Desktop OAuth App's public client id

**Type:** Configuration

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509), milestone M8.

`desktop.yaml.production` renders `github: { client_id: "Ov23lioH6CfiaZR8ESr5" }` with **no**
`client_secret`, so every fresh `./install --desktop` serves the GitHub device flow and enrols its
first window login into `users:`. The template's identity comments describe that instead of a
secret and a hand-written `users:` row, and its `listen:` comment says the device flow never
returns through `/auth/callback`. Detail:
[config-resolution-and-install.md](../config-resolution-and-install.md) § *What a fresh install signs
in with*.

Verified: `tddy-e2e` `install_script` desktop tests 6/6, and the rendered template loads as a
`DaemonConfig` whose `github_auth_flow` is `Device` with no secret. **Not verified:** a fresh install
signing in end to end against github.com, and the OAuth App's token carrying no expiring
`refresh_token` — the developer's check, kept in the backlog entry
`2026-09-18-desktop-install-configures-no-identity.md` (narrowed to that verification).
