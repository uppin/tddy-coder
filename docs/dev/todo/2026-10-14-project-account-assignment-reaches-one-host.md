# 2026-10-14 — A project's account assignment reaches only the host it was written to

**Category:** Defect / deferred scope
**Source:** [2026-10-14-local-gh-github-source.md](../1-WIP/2026-10-14-local-gh-github-source.md) (planning discovery, Exploration 2 gap A)

## What is wrong

`SetProjectAccounts` (and `SetProjectDefaultBranch`) route to exactly one host:
- the handler serves `req.daemon_instance_id` (`tddy-daemon-rpc/src/project/coordinate_handlers.rs:487-528`);
- the web sends it to `group.hosts[0]` (`tddy-web/src/components/projects/ProjectsScreen.tsx:269,301-306`).

Every other host owning the same `project_id` keeps its own `projects.yaml` row without the
assignment. A session started there resolves **not assigned**. The documentation says the opposite:
`docs/ft/daemon/project-concept.md:28`, `packages/tddy-projects/docs/project-service.md:41-46` and
`docs/ft/web/projects-screen-multi-host.md:67-70` all claim it applies to every host.

Vault *records* do propagate ([credential-sync.md](../../ft/daemon/credential-sync.md)); the
*assignment* is what does not.

## Why it was not fixed in the Local gh change

The developer chose to carry the Local gh fact on a forwarded `StartSession` instead of fanning
writes out. Fixing the fan-out changes `SetProjectAccounts` semantics and grows
`coordinate_handlers.rs`, which is already 650/500 lines (`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`).
It is a separate decision with its own blast radius.

## What would close it

Pick one, deliberately:
- **Web writes every host.** The card sends to each host in the group and shows each host's stored
  value.
- **Daemon fans out.** The receiving daemon forwards to every eligible peer owning the `project_id`.
  Split `coordinate_handlers.rs` first (Seam B).
- **The assignment travels with a forwarded start**, like Local gh.

Then correct the three documents above.
