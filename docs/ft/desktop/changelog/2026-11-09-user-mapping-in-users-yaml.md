# 2026-11-09 — The desktop's first sign-in enrols into ~/.tddy/users.yaml

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- The login → OS-user mapping lives in `~/.tddy/users.yaml`, beside `desktop.yaml`, which carries no
  `users:` key. `./install --desktop` renders it empty (`0644`) and never overwrites one.
- The first sign-in from the window writes its row there; `desktop.yaml` keeps its comments.
  Saving settings never writes the mapping.
- `./desktop-dev` renders the checkout-root `users.yaml` for the checkout's own `dev.desktop.yaml`
  only.

See [tddy-desktop-tauri.md § Signing in](../tddy-desktop-tauri.md#signing-in).
