//! tddy-demo — Same app as tddy-coder, with StubBackend.
//!
//! Identical TUI, CLI, and workflow. Only the backend differs (StubBackend).
//! Uses tddy-coder lib with DemoArgs (agent: stub only).

use std::env;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clap::Parser;
use tddy_coder::{disable_raw_mode, run_with_args, Args, DemoArgs};
use tddy_core::init_tddy_logger;

fn main() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        );
        let _ = tddy_coder::disable_raw_mode();
        original_hook(info);
    }));

    let mut cli_args: Vec<String> = env::args().collect();
    // Inject --agent stub if not already specified
    let has_agent = cli_args
        .iter()
        .skip(1)
        .position(|a| a == "--agent" || a == "-a");
    if has_agent.is_none() {
        cli_args.insert(1, "stub".to_string());
        cli_args.insert(1, "--agent".to_string());
    }
    let demo_args = DemoArgs::parse_from(cli_args);
    let args: Args = demo_args.into();
    init_tddy_logger(args.debug, args.debug_output.as_deref());

    ctrlc::set_handler(|| {
        tddy_core::kill_child_process();
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        );
        let _ = disable_raw_mode();
        std::process::exit(130);
    })
    .expect("failed to set Ctrl+C handler");
    let shutdown = Arc::new(AtomicBool::new(false));

    let result = run_with_args(&args, shutdown);
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
