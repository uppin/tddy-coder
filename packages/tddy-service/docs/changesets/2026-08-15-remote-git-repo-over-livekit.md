# 2026-08-15 — remote-git-repo-over-livekit

**Type:** Feature

new `proto/remote_git.proto` (`RemoteGitService.Serve`, a bidi stream whose frames keep `stdout` and `stderr` in separate fields and end with one `exit_code`+`done` frame, because git needs both and the terminal proto carries neither); `proto/auth.proto` gains a second service, `LiveKitTokenService.MintLiveKitToken`, whose request carries only a `session_token` — room and identity are the daemon's choice, so a caller cannot ask to be a daemon; `proto/token.proto` gains `session_token` on both requests, and `TokenServiceImpl::new` is replaced by `unauthenticated`/`authenticated` constructors so no registration acquires the open behaviour by accident, with a `daemon-*` identity refusal enforced in the service itself. (tddy-service)
