# 2026-11-27 — `publish.sh` does not ship `tddy-ios-emulator`

**Category:** Deferred packaging
**Source:** `CS-2026-11-26-ios-simulator-screen-sharing` (PR #1060), § Refactoring Needed (/validate-changes)

- `./release`, `./install` (`INSTALLED_BINARIES`), `./install --desktop` (`DESKTOP_BINARIES`)
  and `scripts/dev-runtime-binaries.sh` build and ship `tddy-ios-emulator`; `publish.sh`'s `.deb`
  does not. The daemon resolves the bridge as its sibling, so a `.deb` install lists no simulators
  (the probe's `--list-devices` fails, which lists none) and could not bridge one.
- **Why deferred:** the `.deb` targets Linux, where the bridge lists nothing and refuses to bridge,
  so shipping it there has no effect today. Revisit if `publish.sh` ever packages for macOS, or to
  keep the shipped sets identical (see `2026-09-16-the-shipped-binary-set-is-restated-in-eight-places-and-one-is-pinned.md`).
