# 2026-10-25 — `./desktop-dev` has a credential vault, stub logins included

`dev.desktop.yaml` now sets `auth_storage: tmp/.tddy/auth`, as `dev.daemon.yaml` does for
`./web-dev`. Before, the desktop dev daemon registered no AccountsService, so the Accounts screen and
the Projects account pickers said "Accounts are not enabled on this host". The vault and the
session-token signing key live under the dev data directory, apart from `./web-dev`'s `tmp/auth`.
