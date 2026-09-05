# 2026-03-24 — Connection screen: delete inactive sessions

- **ConnectionScreen**: Inactive session rows include **Resume** and **Delete** (project session tables and **Other sessions**). **Delete** uses a browser **confirm**, invokes **`DeleteSession`**, refreshes the session list after success, and surfaces RPC errors in the shared connection error area. Active rows show **Connect** and **Signal** only.
- **Feature doc**: [web-terminal.md](../web-terminal.md) (Inactive session deletion).
