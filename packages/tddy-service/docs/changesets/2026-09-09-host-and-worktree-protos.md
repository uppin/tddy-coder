# 2026-09-09 — `connection.proto` is cut for the first time

**Type:** Architecture

Root node of the `#unbundle` stack ([#470](https://github.com/uppin/tddy-coder/pull/470)). Full
story in the cross-package entry: [2026-09-09-unbundle-host-worktree-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md).

`connection.proto` goes **90 → 73 rpcs** and loses the 51 messages and enums that move with them. The
diff is **pure deletion**, so the remaining 73 are wire-identical. `host.proto` (8 rpcs, 31 messages)
and `worktree.proto` (9 rpcs, 20 messages) appear, each with a prost and a tonic `build.rs` pass and
each added to the descriptor set, so reflection lists them like any other service. `src/lib.rs`
exports `HostServiceServer` and `WorktreeServiceServer`.

**There is no `types.proto`, and that is a finding rather than an omission.** Walking the field types
of `connection.proto`'s 238 messages: the closure the host methods reach is 31, the worktree methods
reach 20, and there is zero overlap between them and zero overlap with the closure of everything that
stays. `WorktreeRow`, `ProbeOutcome` and `WorktreeSizeStatus` were all on the planning-time list of
cross-family shared messages and are reached only from inside the moving set. Creating a shared file
here would mean moving messages this node does not use so a later node can.
`tests/unbundle_service_split.rs` pins the absence, so a later node cannot re-couple the protos by
importing one out of habit.

**The sandbox tonic pass's three `.connection.*` extern paths were left alone** — nothing they name
moves in this node. Getting that wrong fails the sandbox codegen with a message that names neither
cause, which is why it is stated rather than assumed.

104 passing / 2 failing (the completion criterion — the 17 methods still declared on
`connection.ConnectionService`) → 109 / 0.
