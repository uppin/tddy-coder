# Quitting Tddy Desktop

The application's process hosts the daemon, so quitting it stops the daemon too. On `RunEvent::Exit`,
`stop_daemon` (`src/lib.rs`) shuts the daemon's children down and aborts its runtime tasks.

That runs on the main (UI) thread, so it is **bounded**: `bounded_shutdown::shut_down_within` waits
at most `QUIT_DEADLINE` (10 s — `kill_all`'s 5 s SIGKILL grace plus slack) for the children, then
the process exits anyway and logs `daemon children did not stop within …`. Without the bound, a
shutdown stuck behind a saturated blocking pool held the window "not responding" until the process
was killed.

Exiting without waiting loses nothing that matters: tmux-run agents outlive the daemon by design, and
the next start re-attaches them.
