use std::collections::{BTreeMap, BTreeSet, HashSet};

use tracing::{debug, warn};

use super::abbrev::unique_prefixes;
use super::extract::{Cmd, Cmds};

pub(super) fn compile_recursive(
    // TODO: Make this a plain string
    mut prefix: Vec<String>,
    cmd: &Cmd,
    long: &str,
    lbuf: &str,
    all: bool,
) -> BTreeMap<String, String> {
    debug!("Prefix: {prefix:?}");
    let mut m = BTreeMap::new();
    let mut bind = |k: String, mut v: String| {
        if !all && !k.starts_with(lbuf) && !v.starts_with(lbuf) {
            // Ideally, we could avoid ever being in this case
            // warn!("Irrelevant to {lbuf}: {k} {v}");
            return;
        }
        debug!("binding '{k}' to '{v}'");
        if let Some(existing) = m.get(k.as_str()) {
            warn!("Map already contained key! {k} -> {v}, {existing}");
        }
        if !v.ends_with(' ') {
            v = format!("{v} ");
        }
        m.insert(k, v);
    };

    let short = &cmd.short;
    let mut prefix_str = prefix.join(" ");
    if !prefix.is_empty() {
        prefix_str.push(' ');
    }
    // Don't make the recursive call if this isn't the case
    debug_assert!(all || lbuf.starts_with(&prefix_str) || prefix_str.starts_with(lbuf));

    let pre_short = format!("{prefix_str}{short}");
    let pre_long = format!("{prefix_str}{long}");
    debug_assert!(pre_short.len() <= pre_long.len());
    if !all && prefix_str.len() > lbuf.len() + 1 {
        return m;
    }
    if !all
        && !pre_short.starts_with(lbuf)
        && !pre_long.starts_with(lbuf)
        && !lbuf.starts_with(&pre_short)
        && !lbuf.starts_with(&pre_long)
    {
        // warn!("Irrelevant to {lbuf}: {pre_short} {pre_long}");
        return m;
    }
    debug!("binding root");
    bind(pre_short, pre_long);

    for (f, fl) in &cmd.flags {
        let expanded = format!("{prefix_str}{long} {f}");
        bind(
            format!("{prefix_str}{long} -{}", fl.short),
            expanded.clone(),
        );
        if fl.squish {
            bind(format!("{prefix_str}{short}{}", fl.short), expanded);
        }
    }
    prefix.push(long.to_string());
    let doesnt_start_with_prefix = !lbuf.starts_with(&format!("{prefix_str}{long}"));
    for (sub_long, sub) in &cmd.subs.0 {
        let sub_short = &sub.short;
        debug!("binding sub: {sub_long}");
        bind(
            format!("{prefix_str}{short}{sub_short}"),
            format!("{prefix_str}{long} {sub_long}"),
        );
        for (f, fl) in &sub.flags {
            if fl.squish {
                bind(
                    format!("{prefix_str}{short}{sub_short}{}", fl.short),
                    format!("{prefix_str}{long} {sub_long} {f}"),
                );
            }
        }
        if !all && doesnt_start_with_prefix {
            continue;
        }
        for (short, long) in compile_recursive(prefix.clone(), sub, sub_long, lbuf, all) {
            bind(short, long);
        }
    }
    m
}

pub(super) fn compile(cmds: &Cmds, lbuf: &str, all: bool) -> BTreeMap<String, String> {
    let mut r = BTreeMap::new();
    for (long, cmd) in &cmds.0 {
        if lbuf.is_empty() || lbuf.starts_with(&cmd.short) || lbuf.starts_with(long) {
            r.extend(compile_recursive(Vec::new(), cmd, long, lbuf, all))
        }
    }
    r
}

pub(super) fn compile_with_prefixes(
    cmds: &Cmds,
    lbuf: &str,
    all: bool,
) -> BTreeMap<String, String> {
    let compiled = compile(cmds, lbuf, all);
    add_prefixes(compiled, lbuf, all)
}

fn prefixes(s: &str, l: usize) -> Vec<String> {
    assert!(l < s.len());
    let mut r = Vec::with_capacity(s.len() - l);
    for len in l..s.len() - 1 {
        r.push(s[..len].to_string());
    }
    r
}

fn add_prefixes(
    mut compiled: BTreeMap<String, String>,
    lbuf: &str,
    all: bool,
) -> BTreeMap<String, String> {
    let words = lbuf.split_whitespace().count();
    let strings = compiled
        .values()
        .filter(|s| s.starts_with(lbuf) && s.split_whitespace().count() == words)
        .cloned()
        .collect::<BTreeSet<String>>();
    let strings = strings.into_iter().collect::<Vec<_>>();
    let denylist = HashSet::from_iter(compiled.keys().map(String::as_str));
    let pfxs = unique_prefixes(strings.as_slice(), &denylist);
    for (s, pfx) in BTreeMap::from_iter(pfxs.into_iter()).into_iter() {
        debug_assert!(pfx.len() <= s.len());
        if s == pfx {
            continue;
        }
        for p in prefixes(&s, pfx.len()) {
            let plen = p.len();
            if !all && lbuf.len() > plen {
                continue;
            }
            debug!("binding prefix '{p}' to '{s}'");
            compiled.insert(p, s.clone());
            if !all && plen - lbuf.len() > 0 {
                break;
            }
        }
    }
    compiled
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use crate::zle::extract::{Cmd, Cmds};

    use super::compile;

    #[test]
    fn test_compile() {
        let cmds = Cmds(BTreeMap::new());
        assert_eq!(compile(&cmds, "", false).iter().collect::<Vec<_>>(), []);

        let cmds = Cmds(BTreeMap::from([(
            String::from("git"),
            Cmd {
                short: String::from("g"),
                flags: HashMap::new(),
                subs: Cmds(BTreeMap::from([(
                    String::from("submodule"),
                    Cmd {
                        short: String::from("su"),
                        flags: HashMap::new(),
                        subs: Cmds::default(),
                    },
                )])),
            },
        )]));
        assert_eq!(
            compile(&cmds, "g", false)
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect::<Vec<_>>(),
            [("g", "git "), ("gsu", "git submodule ")]
        );
    }
}
