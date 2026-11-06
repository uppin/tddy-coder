# `svc_host_builders.rs` is 503 production lines — split after the `#agent-tools` stack lands

**Detected:** 2026-11-02, `/pr-wrap` step 3.5 file-length gate on
[PR #790](https://github.com/uppin/tddy-coder/pull/790) (`#agent-tools 1/12`):
`packages/tddy-session-lifecycle/src/connection_service/svc_host_builders.rs` measured
**500 → 503** production lines (budget 500). The growth is the workspace provisioner built with
`JailedWorkspaceSandboxProvisioner::for_tddy_home(&tddy_data_dir)`.

**Deferred by:** the stack-shared-file rule (`/pr-wrap` step 3.5 and step 4), applied by the
pipeline's standing instruction for this stack. PR #791 (`#agent-tools 2/12`, claude-host) also
modifies this file, so a split here would cascade rename conflicts through every later node.

**What would close it:** the file under 500 production lines, split with the `code-restructuring`
engine on a follow-up branch once the `#agent-tools` stack (#790–#801) has merged. Record at
`packages/tddy-session-lifecycle/docs/code-issues/oversized-file-svc-host-builders.md`.
