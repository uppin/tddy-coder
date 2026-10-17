# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

`useHostPromptAnswer` is extracted from `HostAddKeyAction` and shared with `StartPassphrasePrompt`, mounted on `CreateSessionPane`; `useHostPromptsFor` takes the session token as an argument. A `keepAlive` start event is ignored. See [the passphrase prompt while a session starts](../start-passphrase-prompt.md).
