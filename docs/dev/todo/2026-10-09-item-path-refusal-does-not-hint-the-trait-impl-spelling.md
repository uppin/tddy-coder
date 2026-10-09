# 2026-10-09 — An item path that spells `<Trait for Type>` is refused as absent, with no hint of `<Type as Trait>`

**Category:** Future enhancement (authoring ergonomics)
**Source:** #reshape 12/19 (`feature/reshape/anchors-outline`), discovery Exploration 2 (index-daemon logs)

An index daemon logged this on 2026-10-08:

```
anchors … refused as InvalidArgument (+316ms): plan is malformed:
  `tddy_session_lifecycle::connection_service::svc_demo_vm_ports::<DemoVmService for DemoVmServiceImpl>`
  is not declared in packages/tddy-session-lifecycle/src/connection_service/svc_demo_vm_ports.rs:
  nothing there is named `<DemoVmService for DemoVmServiceImpl>`
```

The author wrote the impl the way the outline labels it (`impl DemoVmService for DemoVmServiceImpl`).
The item-path syntax wants `<DemoVmServiceImpl as DemoVmService>`. The refusal is correct, but it
does not say what to write instead. `Miss::Absent`
(`packages/tddy-code-restructuring/src/backends/rust/item_path.rs:41-43`) could recognise a segment of
the shape `<A for B>` and add "write it `<B as A>`". `ambiguity_hint` (`:62-74`) already does this kind
of thing for ambiguous segments.

The same log has two more refusals: an impl member passed to `--items`
(`DaemonSessionHost::announce_worktree_ready`). That refusal already names the fix
(`qualified_in_module`, `item_anchor.rs:370-385`), but it does not say that impl members take
`anchors --at`.

**Why deferred.** #reshape 12 fixes the cold path and the static check. Changing what `anchors`
reports for a request mistake is outside its boundary, which says those refusals stay as they are.
Small: one match arm and one test over `walk_outline`.
