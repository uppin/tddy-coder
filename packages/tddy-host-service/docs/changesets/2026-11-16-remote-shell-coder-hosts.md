# 2026-11-16 — Coder workspaces list as derived hosts

**Type:** Feature

`CoderDerivedHosts` is a second `DerivedHostSource` beside the SSH-config one, built per call for one subject over the `CoderAccountSource`, `CoderCredentialSource` and `CoderWorkspaceSource` ports. Each workspace of each of the caller's Coder accounts is a derived host (`<parent>/coder/<account_id>/<owner>/<workspace>`, labelled `<workspace> (<account label>)`), usable when it is running and the daemon has the `coder` CLI, with the reason on the host otherwise. An account that cannot be listed is one unusable host standing for it; a locked vault fails the listing and still carries the other sources' hosts. See [the cross-package entry](../../../docs/dev/changesets/2026-11-16-remote-shell-coder-hosts.md).
