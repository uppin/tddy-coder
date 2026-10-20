# 2026-10-20 — `tddy-tools extensions` installs Zed extensions

- **Install, list and remove.** `tddy-tools extensions install <id>[@<version>]` downloads a Zed
  extension from Zed's registry; `install --path <dir>` copies a dev extension; `list` shows what each
  provides; `remove` deletes one. Everything lives under `<tddy data dir>/extensions` in Zed's layout.
- **Atomic, validated install.** A refused install — an unsafe archive entry or symlink chain, a missing
  grammar WASM, a mismatched id, an unparsable language config — leaves the previous version in place.
- Nothing loads an installed extension yet. Feature doc: [Zed extensions](../zed-extensions.md).
