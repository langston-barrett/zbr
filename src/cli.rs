#[derive(Debug, clap::Parser)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: crate::zle::Command,

    /// Verbose mode: use multiple times for increased verbosity
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
    )]
    pub(crate) verbose: u8,
}
