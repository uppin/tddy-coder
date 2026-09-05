# 2026-08-16 — models-and-assistants

**Type:** Feature

new `models.proto` (`ModelRegistryService`: provider CRUD, model enumeration/refresh, residency, assistant CRUD, assignable tools) with its own `TddyServiceGenerator` pass. `acp.proto`'s `NewSessionRequest` gains an **optional** `ModelSessionTarget` carrying a session token plus provider+model or assistant id — optional so the session-hosted `TddyAcpService` is byte-for-byte unchanged, while the daemon-hosted surface refuses a `new_session` without one rather than guessing a model. (tddy-service)
