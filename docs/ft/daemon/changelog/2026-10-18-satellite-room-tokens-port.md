# 2026-10-18 — Room tokens are minted through one port

- **Same tokens, one minter.** The token service, the git-command room token and a screen-sharing
  bridge's token are minted by the one minter the daemon builds from its `livekit:` block. A block
  missing a credential refuses every mint and names the field; a daemon with no LiveKit refuses with a
  precondition failure.
