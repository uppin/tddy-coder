# 2026-03-22 — Connection screen: connected participants (common room)

- **Daemon config**: `livekit.common_room` names a shared LiveKit room; **`/api/config`** exposes it as **`common_room`** alongside **`livekit_url`**.
- **ConnectionScreen** (daemon mode): After GitHub auth, the browser joins that room as **`web-{githubLogin}`** and shows a **Connected participants** table (identity, role, joined time, metadata), updated live via LiveKit events. Terminal full-screen session view is unchanged; the presence connection stays active while the terminal is open.
- **Feature doc**: [web-terminal.md](../web-terminal.md) (Daemon mode: shared LiveKit room).
