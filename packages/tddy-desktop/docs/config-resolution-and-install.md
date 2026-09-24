# Configuration resolution and the local install

Where the embedded daemon's configuration comes from, and what `./install --desktop` lays down so
that it is there.

## Two profiles, one function

`src-tauri/src/config_source.rs` resolves a `DaemonConfigSource` — a workspace root and a config
path — and the choice is made on `cfg!(debug_assertions)` rather than on anything observed at
runtime. The two profiles share no fallback in either direction.

| | Development build | Release build |
|---|---|---|
| Config path | `TDDY_DAEMON_CONFIG`, else `dev.desktop.yaml` found by walking up from the working directory | `$HOME/.tddy/desktop.yaml`, and nothing else |
| Workspace root | the checkout the config was found in | `$HOME/.tddy` |
| Config absent | the walk fails and names the checkout | refuses to start, naming `./install --desktop` |

A release build's rule is one file because a launched `.app` has no useful working directory: Finder
and the Dock start it with `/`, and there is no checkout above that. An upward walk would therefore
either find nothing or find a checkout by accident, and "by accident" is the failure mode — an
operator's installed application silently reading a developer's `dev.desktop.yaml`.

Both profiles then `chdir` to the workspace root, so relative paths inside the YAML mean what they
meant when the daemon ran as a child process, and both load that root's `.env` **without overriding**
anything already exported.

## What the rendered configuration has to contain

`desktop.yaml.production` is the template, and its comments are the authority operators read. Three
things about it are not obvious:

- **`listen.web_port` is required**, although the application serves no HTTP and binds no port of its
  own. `tddy_daemon::runtime::build` refuses to assemble a daemon without it (`config.listen.web_port
  is required`), and in this deployment the value names the loopback port a GitHub sign-in comes back
  on: `src-tauri/src/oauth_callback.rs` opens a one-path `/auth/callback` listener on 127.0.0.1 for
  the duration of a sign-in and closes it again. `github.redirect_uri` is overridden from this port
  rather than honoured, so the callback always reaches the listener that was actually opened. The
  listener is opened whenever `github:` is set, but only a **redirect-flow** sign-in (a
  `client_secret` configured) comes back through it; a device-flow sign-in returns to no callback.
- **`web_bundle_path` is absent**, because `tauri.conf.json` points `frontendDist` at
  `packages/tddy-web/dist` and the bundle is baked into the binary at build time.
- **Signing in needs `github:` with a public `client_id`, and nothing else.** `github:` decides
  whether the daemon loads a signing key and returns a session-user resolver at all; every session
  service in `tddy_daemon::runtime` is assembled inside `if let Some(user_resolver)`, so without it
  the application starts, shows its settings, and offers no sessions, hosts or screen sharing. A
  `client_id` with **no** `client_secret` is a public client: `tddy-daemon-auth` registers the GitHub
  **device flow** for it, and the dashboard signs in by device code, so no secret ships in the
  application. **`users:` is written by the first sign-in**, not by hand: an embedded host enrols
  the first GitHub login completed from its own window against the OS user the application runs as,
  and persists the row into this file ([auth-service.md](../../tddy-daemon-auth/docs/auth-service.md#first-login-enrolment)).
  **No `livekit:` block is needed**: the daemon signs session tokens with an Ed25519 key it generates
  on first boot (`signing_key.pem`, mode `0600`, in `auth_storage`) and verifies them with the same
  key — a desktop has no peers, so its key directory is `StandaloneKeyDirectory`.
  `livekit.api_secret`, when present, signs room JWTs only.
- **The desktop passes the file it loaded to the runtime** (`RuntimeOptions::with_config_path`),
  because that is where the first login is enrolled into. `runtime::build` refuses to assemble an
  embedded host that serves GitHub sign-in to an empty `users:` and names no config file — its first
  login could never be written down, so every sign-in would appear to work and then be refused by
  every RPC.

### What a fresh install still lacks

The code side of all three requirements a fresh install once had to meet by hand is in place: a
daemon with no `livekit:` block signs its own tokens, a `client_id` alone serves the device flow, and
the first sign-in writes `users:`. **`desktop.yaml.production` still ships `github:` unset**, so a
freshly installed application reports "no sign-in configured" until the OAuth App's public
`client_id` is rendered into the template. Its `github:` / `users:` comments also still describe a
`client_id` + `client_secret` pair and a hand-written `users:` row. Rendering that `client_id` and
checking a fresh install end to end is owned by the developer, and tracked in the backlog
(`docs/dev/todo/2026-09-18-desktop-install-configures-no-identity.md`).

## Content Security Policy

`src-tauri/tauri.conf.json` sets a strict policy under `app.security.csp`. A login from the
desktop's own window is "the person at the machine" — the only kind that enrols — so script injected
into the dashboard before sign-in must not be able to run.

| Directive | Value | Why |
|---|---|---|
| `default-src` | `'self'` | |
| `script-src` | `'self' 'wasm-unsafe-eval'` | no `unsafe-eval`, no inline script; `'wasm-unsafe-eval'` is for ghostty-web's inlined WASM |
| `style-src` | `'self'` | also governs `style-src-attr`, so a `style="…"` attribute is blocked. `tddy-web` writes none; React's `style` prop goes through CSSOM and is unaffected |
| `style-src-elem` | `'self' 'unsafe-inline'` | runtime `<style>` elements: the `ConnectionTerminalChrome` / `SessionDrawer` dot styles, react-remove-scroll-bar, react-resizable-panels |
| `img-src` | `'self' https://avatars.githubusercontent.com` | the signed-in user's avatar |
| `font-src` | `'self'` | |
| `connect-src` | `'self' ipc: http://ipc.localhost data: ws: wss:` | Tauri IPC; `data:` for the inlined WASM; `ws:` / `wss:` to any host because the LiveKit URL is per-deployment and typed at runtime |
| `frame-src`, `object-src` | `'none'` | |
| `base-uri` | `'self'` | |
| `form-action`, `frame-ancestors` | `'none'` | |

Tauri delivers it as a response header on `tauri://` HTML, adding sha256 hashes for the bundled
scripts and a nonce for `index.html`'s one `<style>`. **It applies only to `custom-protocol` (release)
builds.** There is no `devCsp`: a development build loads Vite directly, and Tauri applies no policy
there.

⚠ **Not yet verified in a launched production build.** What has been checked: `cargo check -p
tddy-desktop` with and without `tauri/custom-protocol`, the header Tauri serves for `index.html`
(read through an asset-resolver probe), and the built bundle grepped for inline script and `eval`.
What has not: a running `custom-protocol` build — the terminal's WASM, IPC, LiveKit `ws://` from
`tauri://localhost`, and the console for CSP errors. The wdio e2e runs a development build, where no
policy applies. Two narrowings are possible later: moving the two dot-style strings into bundled CSS
and handing Tauri's nonce to `get-nonce` would drop `'unsafe-inline'`, and `connect-src` could be
narrowed to the configured `livekit.url`.

## What `./install --desktop` installs

A different deployment from `./install --systemd`, and the two are an error in one run: this
application's own process **is** a `tddy-daemon`, so there is no unit, no service user, no web bundle
directory, no served port, and neither `tddy-daemon` nor `tddy-supervisor` is installed.

| | macOS | Linux |
|---|---|---|
| Application | `Tddy Desktop.app` → `~/Applications` | binary → `$BIN_DIR`; `.desktop` entry and hicolor icon → `$XDG_DATA_HOME` |
| `tddy-desktop` on `PATH` | launcher script that `exec`s the in-bundle binary | the installed binary |
| `tddy-coder`, `tddy-tools`, `tddy-sandbox-runner`, `tddy-index-daemon`, `tddy-remote-git-repo`, `tddy-session-sync` | `Contents/MacOS` **and** `$BIN_DIR` | `$BIN_DIR` |

The launcher is a script and not a symlink: a symlink makes the process's executable path
`$BIN_DIR/tddy-desktop`, and both bundle identity and the sibling-of-`current_exe()` lookup that
finds `tddy-sandbox-runner` and `tddy-index-daemon` read that path. The same lookup is why macOS gets
two copies of the CLI binaries — the in-bundle set is what the daemon resolves, the `$BIN_DIR` set is
what `allowed_tools` names and what an operator runs by hand.

`./release --desktop` owns the build and its order: the CLI binaries, then the `tddy-web` bundle,
then `tauri build`. The application embeds the bundle, so building the app first yields a stale
dashboard and no error. `--build` delegates to that script rather than repeating the sequence, which
is what keeps a build-then-install and a `--build` install producing the same artifacts; without
`--build` the install preflights for every artifact and fails naming the script.

`./release --desktop` distinguishes a failed application build from a failed *later* bundle target:
if the `.app` is present it names it and points at `./install --desktop`, because on macOS the `dmg`
target drives Finder over AppleScript and times out without Automation permission while the `.app`
itself is valid.

The config is rendered only when absent — a reinstall keeps the operator's file untouched — and the
install warns when `INSTALL_TDDY_HOME` points away from `$HOME/.tddy`, since a release build has no
second place to look. Path overrides: `INSTALL_TDDY_HOME`, `INSTALL_BIN_DIR`,
`INSTALL_DESKTOP_APP_DIR`, `INSTALL_XDG_DATA_DIR`, `INSTALL_DAEMON_LOG_DIR`,
`INSTALL_AUTH_STORAGE_DIR`.

## Coverage

`packages/tddy-e2e` owns the script's contract: the flag's presence and its mutual exclusion with
`--systemd`, the binary set (which must exclude `tddy-daemon` and `tddy-supervisor`), the template's
absent and required keys, and that every `__NAME__` placeholder in the template is one `install`
substitutes. `tests/install_script.rs` runs the installer end to end into a redirected `HOME` on both
platforms. `config_source.rs`'s own tests pin the release profile against a fake home, including that
it never reads the development configuration.
