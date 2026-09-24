# 2026-09-24 — A strict Content Security Policy, and sign-in without a secret

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`src-tauri/tauri.conf.json` replaces `"csp": null` with a strict policy (no `unsafe-eval`, no inline script, `'unsafe-inline'` only in `style-src-elem`, `'wasm-unsafe-eval'` and `data:` for the terminal's WASM, `ws:` / `wss:` for a runtime LiveKit URL). It applies to `custom-protocol` builds only, with no `devCsp`, and is **not yet verified in a launched production build**. The embedded daemon enrols its first window login into the config file the application loaded, and `runtime::build` refuses an embedded host serving sign-in to an empty `users:` with no config file. `desktop.yaml.production` renders the public `client_id` — see [2026-09-24-keyring-desktop-client-id.md](2026-09-24-keyring-desktop-client-id.md). Detail: [config-resolution-and-install.md](../config-resolution-and-install.md).
