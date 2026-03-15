//! tddy-coder CLI binary.

use clap::Parser;
use tddy_coder::{load_config, merge_config_into_args, run_main, Args, CoderArgs};
use tddy_core::init_tddy_logger;

fn main() {
    let coder_args = CoderArgs::parse();
    let config_path = coder_args.config.clone();
    let mut args: Args = coder_args.into();

    if let Some(ref path) = config_path {
        match load_config(path) {
            Ok(config) => merge_config_into_args(&mut args, config),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    init_tddy_logger(args.debug, args.debug_output.as_deref());
    run_main(args);
}
