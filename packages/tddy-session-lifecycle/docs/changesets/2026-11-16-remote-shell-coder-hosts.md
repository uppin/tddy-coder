# 2026-11-16 — A session starts and resumes on a Coder workspace

**Type:** Feature

The start and resume paths pass the caller's subject to the target resolution, so a Coder target's account is read from that caller's vault at the start and again at every resume. A stopped workspace is refused at launch with the `coder` CLI's own complaint; a resume whose vault has since locked is `FailedPrecondition` — the vault holding this session's Coder account is locked: unlock it and try again — and the session is kept. The daemon wires its credential source through `DaemonSessionHost::with_coder_credentials`. See [the cross-package entry](../../../docs/dev/changesets/2026-11-16-remote-shell-coder-hosts.md).
