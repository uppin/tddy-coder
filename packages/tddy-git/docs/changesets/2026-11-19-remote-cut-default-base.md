# 2026-11-19 — A new branch on a target that names no base is cut from its clone's fetched main

**Type:** Fix

`RemoteBranch::NewFrom.base_ref` is `Option<String>`. `None` means the clone's default integration
base: after the fetch, one call on the target finds the first of `DEFAULT_BASE_REFS`
(`origin/master`, `origin/main`, `origin/HEAD`) the clone holds, printed between
`TDDY_REMOTE_BASE_BEGIN:`/`:TDDY_REMOTE_BASE_END` sentinels. A clone of a URL holding none is
`RemoteCheckoutError::NoDefaultBase { root }`. A project with no URL, whose clone is initialised in
place with no remote, branches from its `HEAD`. Full record:
[`docs/dev/changesets/2026-11-19-target-start-host-paths.md`](../../../../docs/dev/changesets/2026-11-19-target-start-host-paths.md).
