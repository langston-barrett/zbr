use std::error::Error;

use tracing::{Level, debug};
use tracing_subscriber::fmt::format::FmtSpan;

mod build;
mod cli;
pub mod zle;

use cli::Cli;

fn verbosity_to_log_level(verbosity: u8) -> Level {
    match verbosity {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    }
}

fn initialize_tracing(cli: &Cli) {
    tracing_subscriber::fmt::fmt()
        .with_span_events(FmtSpan::NONE)
        .with_target(false)
        .with_max_level(verbosity_to_log_level(cli.verbose))
        .with_writer(std::io::stderr)
        .init();
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli: Cli = clap::Parser::parse();

    initialize_tracing(&cli);
    debug!(?cli);
    zle::go(cli.cmd)?;
    Ok(())
}
