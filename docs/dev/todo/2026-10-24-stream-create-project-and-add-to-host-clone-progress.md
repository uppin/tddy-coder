# 2026-10-24 — Stream CreateProject and AddProjectToHost clone progress

**Category:** Future enhancement
**Source:** #start-progress 5/8 (clone progress)

- `CreateProject` and `AddProjectToHost` clone through `ProjectRpcHandler::clone_without_deadline`
  (`tddy-daemon-rpc`, `project/coordinate_handlers.rs`). Since #start-progress 5/8 those clones have
  no deadline and leave no partial destination, but the RPCs are unary: a large clone is silent until
  it ends, and the operator cannot stop it.
- **Why deferred:** the developer scoped streaming to session creation. Both RPCs would need a
  streaming variant (as `StreamStartSession` is for `StartSession`), a client that renders it, and a
  stop path that reaches `clone_run::CloneHandle::stop`.
- The pieces already exist: `clone_with_progress` reports throttled `GitProgress`, and the worker
  relays it as `WorkerEvent::CloneProgress`. The supervised backend's clone reports none until
  #start-progress 6/8.
