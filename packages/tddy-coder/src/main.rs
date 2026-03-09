//! tddy-coder CLI binary.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clap::Parser;
use tddy_coder::{disable_raw_mode, run_with_args, Args, CoderArgs};
use tddy_core::init_tddy_logger;

fn main() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(
            std::io::stderr(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show,
        );
        let _ = disable_raw_mode();
        original_hook(info);
    }));

    let coder_args = CoderArgs::parse();
    let args: Args = coder_args.into();
    init_tddy_logger(args.debug, args.debug_output.as_deref());

    ctrlc::set_handler(|| {
        tddy_core::kill_child_process();
        // Restore terminal before exit; otherwise raw mode leaves it broken.
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
