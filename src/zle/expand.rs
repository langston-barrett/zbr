use std::path::{Path, PathBuf};
use std::{fs, io};

use tracing::debug;

use crate::build;

use super::compile::compile_with_prefixes;
use super::extract::Cmds;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(crate) struct ConfigFile {
    #[serde(default)]
    pub(super) cmds: Cmds,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigFileError {
    #[error("i/o error for config file at {1}: {0}")]
    Io(io::Error, PathBuf),
    #[error("toml error")]
    Toml(#[from] toml::de::Error),
}

impl ConfigFile {
    pub(super) fn from_file<P: AsRef<Path>>(p: P) -> Result<Self, ConfigFileError> {
        let path = p.as_ref();
        let s = fs::read_to_string(path).map_err(|e| ConfigFileError::Io(e, path.to_path_buf()))?;
        Ok(toml::from_str::<ConfigFile>(&s)?)
    }
}

fn expand_pre(conf: ConfigFile, lbuf: String) -> Option<String> {
    let compiled = compile_with_prefixes(&conf.cmds, &lbuf, false);
    if let Some(r) = compiled.get(lbuf.as_str()) {
        debug!("Expanding {lbuf} to {r}");
        return Some(r.clone());
    }
    if lbuf == "b" {
        let pwd = std::env::current_dir().ok()?;
        match build::System::detect(pwd) {
            Some(build::System::Cabal) => Some(String::from("cabal build ")),
            Some(build::System::Cargo) => Some(String::from("cargo build ")),
            Some(build::System::Make) => Some(String::from("make ")),
            None => None,
        }
    } else if lbuf == "r" {
        let pwd = std::env::current_dir().ok()?;
        match build::System::detect(pwd) {
            Some(build::System::Cabal) => Some(String::from("cabal run ")),
            Some(build::System::Cargo) => Some(String::from("cargo -q run ")),
            Some(build::System::Make) => None,
            None => None,
        }
    } else if lbuf == "t" {
        let pwd = std::env::current_dir().ok()?;
        match build::System::detect(pwd) {
            Some(build::System::Cabal) => Some(String::from("cabal test ")),
            Some(build::System::Cargo) => Some(String::from("cargo test ")),
            Some(build::System::Make) => Some(String::from("make test ")),
            None => None,
        }
    } else if lbuf == "w" {
        let pwd = std::env::current_dir().ok()?;
        match build::System::detect(pwd) {
            Some(build::System::Cabal) => Some(String::from(
                "ls ./**/*.cabal ./**/*.hs | entr -c -s 'cabal build'",
            )),
            Some(build::System::Cargo) => Some(String::from(
                "ls ./**/Cargo.toml ./**/*.rs | entr -c -s 'cargo fmt && cargo clippy -- --deny warnings'",
            )),
            Some(build::System::Make) => Some(String::from("make test")),
            None => None,
        }
    } else {
        None
    }
}

pub(super) fn clean_buf(mut lbuf: String) -> (String, String) {
    let mut prefix = String::new();
    for delim in [" || ", " && ", "; "] {
        debug!("Searching for {delim}");
        if let Some(idx) = lbuf.rfind(delim) {
            debug!("Found {delim} at {idx}");
            let after = idx + delim.len();
            let (pre, post) = lbuf.split_at(after);
            prefix = String::from(pre);
            lbuf = String::from(post);
        }
    }
    (prefix, lbuf)
}

pub(crate) fn expand(conf: ConfigFile, lbuf: String, rbuf: String) -> Option<String> {
    if !rbuf.is_empty() {
        return None;
    }
    let (prefix, lbuf) = clean_buf(lbuf);
    let expanded = expand_pre(conf, lbuf);
    debug!("expanded = {expanded:?}");
    expanded.map(|s| format!("{prefix}{s}"))
}

#[cfg(test)]
mod tests {
    use super::expand;

    fn test_expand(l: &str, r: &str) {
        let conf = super::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            expand(conf, String::from(l), String::new()).as_deref(),
            Some(r)
        );
    }

    #[test]
    fn test_expand_cargo_doc_o() {
        test_expand("cargo doc o", "cargo doc --open ");
    }

    #[test]
    fn test_expand_grba() {
        test_expand("grba", "git rebase --abort ");
    }

    #[test]
    fn test_expand_gsu() {
        test_expand("gsu", "git submodule ");
    }

    #[test]
    fn test_expand_gsuui() {
        // TODO
        // test_expand("gsuui", "git submodule update --init");
    }

    #[test]
    fn test_expand_git_commit_m() {
        test_expand("git commit -m", "git commit --message ");
    }

    // TODO: Squished flags before subcommands
    #[test]
    fn test_expand_sysus() {
        // test_expand("sysus", "systemctl --user status");
    }
}
