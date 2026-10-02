# 2026-10-02 — `LspClient::root_uri`

**Type:** Feature

`#live-plan` 1/7, PR [#537](https://github.com/uppin/tddy-coder/pull/537). Cross-package entry:
[2026-10-02-item-anchors.md](../../../../docs/dev/changesets/2026-10-02-item-anchors.md).

`LspClient` reports the root it was initialized against (`root_uri()`, from the `file://` uri the
handshake was given). The restructuring backend holds only the client, and an item resolver is given a
workspace-relative file, so the root is how it finds the package manifest that names the crate. One
accessor; no change to behaviour or to any request.

No code-issue records exist for this package.
