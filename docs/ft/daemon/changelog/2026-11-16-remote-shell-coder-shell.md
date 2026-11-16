# 2026-11-16 — Coder workspaces as a remote-shell transport

A Coder workspace is a place commands run: the daemon drives the `coder` CLI on its host
with a connected account's URL and token, in that child's environment alone, and never
starts the workspace. The daemon lists the workspaces an account can reach — name, owner,
status, and whether a command can run on it — by asking the deployment over HTTP, so the
list answers on a host with no `coder` CLI; the host's tooling report says whether the CLI
is there. A locked vault, a rejected token and an unreachable deployment read as three
different failures.

See [Provider accounts § Coder accounts](../provider-accounts.md#coder-accounts) and
[Host tooling on the Hosts screen § The Coder CLI the report carries](../../web/hosts-screen-tooling.md#the-coder-cli-the-report-carries).
