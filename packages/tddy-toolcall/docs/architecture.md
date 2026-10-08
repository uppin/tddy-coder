# tddy-toolcall architecture

## Overview

The tool-call protocol between an agent's `tddy-tools` process and the host running its session:
the relay listener, its client, and the channels submit results and transitions travel through.

Workspace dependencies: `tddy-workflow`, `tddy-session-actions` (the listener answers
`ListActions`/`InvokeAction` itself), `tddy-rpc`, `tddy-stdio`. It sits below the backends and the
presenter, so it names neither; the presenter drains its queue through
`Presenter::poll_tool_calls`.

`tddy-core` re-exports this crate whole (`pub use tddy_toolcall::*;`), so every `tddy_core::toolcall::…` path consumers name resolves unchanged. New code should name `tddy_toolcall` directly.

## Toolcall (`toolcall`)

- **store_submit_result / take_submit_result_for_goal**: Shared storage for submit results. Presenter writes via tool executor; workflow reads. Key: goal name; Value: JSON string.
- **ToolCallRequest / ToolCallResponse**: IPC types. **SubmitActivity** (goal, data) notifies the presenter for activity-log lines only—the relay has already acknowledged `submit` on the wire. **Ask** (questions, response_tx) and **Approve** (tool_name, input, response_tx) block until `Presenter::poll_tool_calls` completes the oneshot. Responses: SubmitOk, SubmitError, AskAnswer, ApproveResult, Error.
- **start_toolcall_listener**: Unix domain socket listener. Each accepted connection is served by a **`ToolcallRpcService`** (`toolcall/listener.rs`) hosted over `tddy-rpc`/`tddy-stdio` framing (`StdioEndpoint::from_duplex`, opened as `RequestTransport::UnixSocket`, so every request the listener hosts is stamped as arriving over a Unix socket) — not a raw JSON line — dispatching by RPC method name (`Submit`/`Ask`/`Approve`/`ListActions`/`InvokeAction`/`Build`/`BuildList`). The wire *payloads* are the same JSON shapes the original newline-delimited protocol used (the `*Wire` structs, `ToolCallResponse::to_json_line()`); only the framing changed. For **`Submit`**: persists via `store_submit_result`, returns **`SubmitOk` immediately**, then `try_send`s `SubmitActivity` to the presenter queue (full queue or disconnect skips activity notification but does not affect the client). For **`Ask`** / **`Approve`**: forwards to the presenter with a oneshot and waits for the response before returning it. `ListActions`/`InvokeAction` are handled directly in the listener (no presenter involved); `Build`/`BuildList` dispatch to the registered `BuildExecutor`.
- **`toolcall::client::dispatch_toolcall`**: the client-side counterpart, in this crate beside the listener it speaks to — connects to `TDDY_SOCKET`, wraps the stream via `StdioEndpoint::from_duplex` (`RequestTransport::UnixSocket`), and calls the RPC method matching the wire request's `"type"` field. `tddy-tools` calls it; both ends of one wire are defined together, so a change to the framing cannot be made to one and not the other.
- **`toolcall::client_wire`**: the CLI's request/response shapes, beside their `*RequestWire` counterparts. `AskQuestionItem` re-exports **`tddy_workflow::QuestionOption`** rather than declaring a second copy of it.
- **TDDY_SOCKET**: Env var set by tddy-coder when spawning agent; tddy-tools connects to this path.

## The `github-token` verb

A tool asks for the session's GitHub token at the moment it needs one; the token is never delivered in
an environment variable or a file.

- **Wire.** `github-token` is a JSON verb on the existing toolcall service (RPC method
  `GithubToken`, `GITHUB_TOKEN_METHOD`), not a `.proto` message: nothing to regenerate. The request
  names no session, project or account. The listener's socket is bound per session and carries its
  handlers per instance, as `spawn-child` and `spawn-conversation` do, so the socket **is** the
  identity: a tool can only ask about its own session.
- **Handler.** `GithubCredentialHandler`, registered on the listener per instance. A listener with no
  handler answers with a *no credential handler* error; there is no fallback.
- **Answer.** `ToolCallResponse::GithubTokenOk` holds a `RedactedToken`, whose `Debug` is fixed text,
  and the listener's `[send]` log line withholds it. A refusal travels as
  `{"status":"error","message":…}` carrying the host's words verbatim.
- **Client.** `request_github_token(socket)` and `request_github_token_from_session()` (which reads
  `TDDY_SOCKET` only to learn whom to ask) live here, beside the listener they speak to, because both
  `tddy-tools` and the workflow actions use them and `tddy-tools` depends on `tddy-workflow-recipes`.
  A missing `TDDY_SOCKET` is an error naming it.
- **Who answers.** Where the session's agent talks to the daemon's own listener, the daemon's
  `SessionGithubCredential` answers
  ([session identity](../../tddy-session-lifecycle/docs/session-identity.md)). A `tddy-coder` tool
  session's listener binds a handler that relays to the daemon over the per-OS-user host-session socket
  ([coder wiring](../../tddy-coder/docs/host-session-wiring.md)).

Pinned by `tests/github_token_request_acceptance.rs`.
