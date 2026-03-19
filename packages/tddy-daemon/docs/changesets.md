# Changesets Applied

Wrapped changeset history for tddy-daemon.

- **2026-03-19** [Feature] tddy-daemon Binary Extraction — New binary crate. DaemonConfig (listen, livekit, github, users, allowed_tools). AuthService from config. ConnectionService: ListTools, ListSessions, StartSession, ConnectSession, ResumeSession. ProcessSpawner with fork+setuid/setgid, LiveKit credential passing. Session reader from ~user/.tddy/sessions. TokenService when LiveKit configured. serve_web_bundle via tddy-coder. (tddy-daemon)
