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
  rather than honoured, so the callback always reaches the listener that was actually opened.
- **`web_bundle_path` is absent**, because `tauri.conf.json` points `frontendDist` at
  `packages/tddy-web/dist` and the bundle is baked into the binary at build time.
- **An identity is three blocks at once.** `github:` decides whether `build_auth_entries` returns a
  session-user resolver at all; `livekit.api_secret` is the only source of the token signer, so
  without it every token-gated RPC refuses even with a resolver present; `users:` maps a login to an
  OS user with no fallback. Every session service in `tddy_daemon::runtime` is assembled inside
  `if let Some(user_resolver)`, so missing any of the three leaves an application that starts, shows
  its settings, and offers no sessions, hosts or screen sharing.

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

`--build` runs `./release`, then the `tddy-web` bundle, then `tauri build`, in that order, because the
application embeds the bundle; building the app first yields a stale dashboard and no error. Without
`--build` the install preflights for every artifact and fails naming the exact command.

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
