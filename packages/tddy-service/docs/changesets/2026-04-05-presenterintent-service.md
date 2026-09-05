# 2026-04-05 — `PresenterIntent` service

**Type:** Feature

**`presenter_intent.proto`**: localhost gRPC from **`tddy-daemon`** to **`tddy-coder`** for clarification answers, document review actions, **`SubmitFeatureText`**. Implemented in **`presenter_intent_service.rs`**; **`build.rs`** / **`lib.rs`** register the service with the child’s gRPC server. Feature docs: [telegram-session-control.md](../../../../docs/ft/daemon/telegram-session-control.md), [daemon changelog](../../../../docs/ft/daemon/changelog/). (tddy-service, tddy-coder, tddy-daemon)
