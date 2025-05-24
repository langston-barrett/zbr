use super::compile::{self};
use super::expand::{self, clean_buf};

pub(super) fn hint(conf: &expand::ConfigFile, buf: String, max: usize) -> Vec<(String, String)> {
    let (_prefix, buf) = clean_buf(buf);
    let mut compiled = compile::compile_with_prefixes(&conf.cmds, &buf, false)
        .into_iter()
        .collect::<Vec<_>>();
    compiled.sort();
    compiled
        .into_iter()
        .filter(|(k, _v)| k.starts_with(&buf))
        .take(max)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{expand, hint};

    #[test]
    fn test_hint() {
        let conf = expand::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            hint(&conf, String::from("git s"), 5)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [
                ("git s", "git status "),
                ("git see", "git send-email "),
                ("git send-e", "git send-email "),
                ("git send-p", "git send-pack "),
                ("git sep", "git send-pack ")
            ]
        );
        assert_eq!(
            hint(&conf, String::from("git shor"), 5)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [
                ("git shor", "git shortlog "),
                ("git short", "git shortlog ")
            ]
        );
    }

    #[test]
    fn test_hint_git_submo() {
        let conf = expand::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            hint(&conf, String::from("git submo"), 5)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [
                ("git submo", "git submodule "),
                ("git submod", "git submodule ")
            ]
        );
    }

    #[test]
    fn test_hint_git_submodule() {
        let conf = expand::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            hint(&conf, String::from("git submodule"), 5)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [
                ("git submodule ab", "git submodule absorbgitdirs "),
                ("git submodule ad", "git submodule add "),
                ("git submodule d", "git submodule deinit "),
                ("git submodule f", "git submodule foreach "),
                ("git submodule i", "git submodule init ")
            ]
        );
    }

    #[test]
    fn test_hint_gsu() {
        let conf = expand::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            hint(&conf, String::from("gsu"), 5)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            // TODO more
            [("gsu", "git submodule "),]
        );
    }

    #[test]
    fn test_hint_grb() {
        let conf = expand::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            hint(&conf, String::from("grb"), 5)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [
                ("grb", "git rebase "),
                ("grba", "git rebase --abort "),
                ("grbc", "git rebase --continue "),
                ("grbi", "git rebase --interactive ")
            ]
        );
        assert_eq!(
            hint(&conf, String::from("git rb"), 5)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [
                ("git rb", "git rebase "),
                ("git rba", "git rebase --abort "),
                ("git rbc", "git rebase --continue "),
                ("git rbi", "git rebase --interactive ")
            ]
        );
    }

    #[test]
    fn test_hint_compound() {
        let conf = expand::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            hint(&conf, String::from("echo foo && git shor"), 5).len(),
            2
        );
        // TODO
        assert_eq!(
            hint(&conf, String::from("echo foo && git status && git shor"), 5).len(),
            0
        );
    }

    #[test]
    fn test_hint_flag() {
        let conf = expand::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            hint(&conf, String::from("docker -"), 30)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [
                ("docker --conf", "docker --config "),
                ("docker --cont", "docker --context "),
                ("docker --d", "docker --debug "),
                ("docker --h", "docker --host "),
                ("docker --l", "docker --log-level "),
                ("docker --tlsca", "docker --tlscacert "),
                ("docker --tlsce", "docker --tlscert "),
                ("docker --tlsk", "docker --tlskey "),
                ("docker --tlsv", "docker --tlsverify "),
                ("docker --v", "docker --version "),
                ("docker -cf", "docker --config "),
                ("docker -ct", "docker --context "),
                ("docker -dg", "docker --debug "),
                ("docker -ho", "docker --host "),
                ("docker -l-", "docker --log-level "),
                ("docker -tk", "docker --tlskey "),
                ("docker -tla", "docker --tlscacert "),
                ("docker -tle", "docker --tlscert "),
                ("docker -ts", "docker --tls "),
                ("docker -tv", "docker --tlsverify "),
                ("docker -vn", "docker --version ")
            ]
        );
    }

    #[test]
    fn test_hint_flag_compound() {
        let conf = expand::ConfigFile::from_file("conf/conf.toml").unwrap();
        assert_eq!(
            hint(&conf, String::from("cargo --verbose b"), 5)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            // TODO any
            []
        );
    }
}
