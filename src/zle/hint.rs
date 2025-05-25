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
    use expect_test::expect;

    use super::{expand, hint};

    fn serialize(v: &[(String, String)]) -> String {
        let mut s = String::with_capacity(v.len());
        for (k, v) in v {
            s.push_str(&k);
            s.push_str(" -> ");
            s.push_str(&v);
            s.push('\n');
        }
        s
    }

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
        let hints = hint(&conf, String::from("gsu"), usize::MAX);
        let expected = expect![[r#"
            gsu -> git submodule 
            gsuab -> git submodule absorbgitdirs 
            gsuad -> git submodule add 
            gsud -> git submodule deinit 
            gsuf -> git submodule foreach 
            gsui -> git submodule init 
            gsuseb -> git submodule set-branch 
            gsuseu -> git submodule set-url 
            gsust -> git submodule status 
            gsusu -> git submodule summary 
            gsusy -> git submodule sync 
            gsuu -> git submodule update 
        "#]];
        expected.assert_eq(&serialize(&hints));
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
            hint(&conf, String::from("git rebase -"), 30)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [
                ("git rebase --a", "git rebase --abort "),
                ("git rebase --c", "git rebase --continue "),
                ("git rebase --i", "git rebase --interactive "),
                ("git rebase -a", "git rebase --abort "),
                ("git rebase -c", "git rebase --continue "),
                ("git rebase -i", "git rebase --interactive ")
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
