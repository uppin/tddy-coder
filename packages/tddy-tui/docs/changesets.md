# Changesets Applied

Wrapped changeset history for tddy-tui.

- **2026-03-09** [Feature] gRPC Remote Control — run_event_loop accepts optional external_intents and debug flag. Drains external intents via try_recv; passes to presenter.handle_intent(). Debug area shown only when debug=true (--debug). Enables gRPC clients to inject intents alongside crossterm keyboard input. (tddy-tui)
