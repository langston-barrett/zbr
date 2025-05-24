use std::path::PathBuf;
use std::process::exit;

mod abbrev;
mod aliases;
mod compile;
mod expand;
mod extract;
mod hint;

use self::expand::ConfigFileError;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    Aliases {
        conf: PathBuf,
    },
    Expand {
        conf: PathBuf,
        lbuf: String,
        rbuf: String,
    },
    Extract(extract::Config),
    Hint {
        #[arg(long, default_value_t = u8::MAX)]
        max: u8,

        conf: PathBuf,
        buf: String,
    },
    Init {
        conf: PathBuf,
    },
}

pub fn go(cmd: Command) -> Result<(), ConfigFileError> {
    match cmd {
        Command::Aliases { conf } => {
            let conf = expand::ConfigFile::from_file(conf)?;
            aliases::go(conf)
        }
        Command::Expand { conf, lbuf, rbuf } => {
            let conf = expand::ConfigFile::from_file(conf)?;
            if let Some(result) = expand::expand(conf, lbuf, rbuf) {
                println!("{}", result);
                exit(0);
            }
            exit(1)
        }
        Command::Extract(conf) => extract::go(conf),
        Command::Hint { conf, buf, max } => {
            let conf = expand::ConfigFile::from_file(conf)?;
            for (k, v) in hint::hint(&conf, buf, max as usize) {
                println!("{k} --> {v}");
            }
        }
        Command::Init { conf } => {
            println!(
                "{}",
                include_str!("init.zsh").replace("${ZBR_CONF}", &conf.to_string_lossy())
            )
        }
    }
    Ok(())
}
