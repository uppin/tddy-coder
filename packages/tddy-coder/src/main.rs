//! tddy-coder CLI binary.

use clap::Parser;
use tddy_coder::{run_main, Args, CoderArgs};
use tddy_core::init_tddy_logger;

fn main() {
    let coder_args = CoderArgs::parse();
    let args: Args = coder_args.into();
    init_tddy_logger(args.debug, args.debug_output.as_deref());
    run_main(args);
}
