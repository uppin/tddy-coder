//! Tddy Desktop: the daemon and its dashboard in one process.
//!
//! Everything this binary does lives in [`tddy_desktop::run`] — the entry point is kept bare so
//! nothing runs before the spawn worker is forked (see the module docs there for why that
//! ordering is not negotiable).

// A release build must not pop a console window behind the app on Windows. The app is not built
// for Windows today (the daemon's local socket is unix-only), but the attribute costs nothing and
// is wrong to remember later.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    tddy_desktop::run()
}
