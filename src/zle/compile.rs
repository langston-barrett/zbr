use std::collections::{BTreeMap, BTreeSet, HashSet};

use tracing::{debug, warn};

use super::abbrev::unique_prefixes;
use super::extract::{Cmd, Cmds};

pub(super) fn compile_recursive(
    mut pfx: String,
    cmd: &Cmd,
    long: &str,
    lbuf: &str,
    all: bool,
) -> BTreeMap<String, String> {
    debug!("Prefix: {pfx}");
    let mut m = BTreeMap::new();
    let mut bind = |k: String, v: String| -> bool {
        debug!("considering binding '{k}' to '{v}'");
        debug_assert!(!k.ends_with(' '));
        debug_assert!(v.ends_with(' '));
        if !all && !k.starts_with(lbuf) && !v.starts_with(lbuf) {
            // Ideally, we could avoid ever being in this case
            // warn!("Irrelevant to {lbuf}: {k} {v}");
            return false;
        }
        debug!("binding '{k}' to '{v}'");
        if let Some(existing) = m.get(k.as_str()) {
            warn!("Map already contained key! {k} -> {v}, {existing}");
        }
        m.insert(k, v);
        true
    };

    if !pfx.is_empty() && !pfx.ends_with(' ') {
        pfx.push(' ');
    }
    if !all && pfx.len() > lbuf.len() + 1 {
        return m;
    }

    // Shouldn't have made the recursive call if this isn't the case
    debug_assert!(all || lbuf.starts_with(&pfx) || pfx.starts_with(lbuf));
    let short = &cmd.short;
    let pre_short = format!("{pfx}{short}");
    let mut pre_long = format!("{pfx}{long}");
    debug_assert!(pre_short.len() <= pre_long.len());
    let doesnt_start_with_prefix = !lbuf.starts_with(&pre_long);
    if !all
        && !pre_short.starts_with(lbuf)
        && !pre_long.starts_with(lbuf)
        && !lbuf.starts_with(&pre_short)
        && doesnt_start_with_prefix
    {
        // warn!("Irrelevant to {lbuf}: {pre_short} {pre_long}");
        return m;
    }
    debug!("binding root");
    pre_long.push(' ');
    bind(pre_short, pre_long);

    for (f, fl) in &cmd.flags {
        let expanded = format!("{pfx}{long} {f} ");
        bind(format!("{pfx}{long} -{}", fl.short), expanded.clone());
        if fl.squish {
            bind(format!("{pfx}{short}{}", fl.short), expanded);
        }
    }
    for (sub_long, sub) in &cmd.subs.0 {
        let sub_short = &sub.short;
        debug!("binding sub: {sub_long}");

        let k = format!("{pfx}{short}{sub_short}");
        let starts_with_key = lbuf.starts_with(&k);
        bind(k, format!("{pfx}{long} {sub_long} "));
        if !starts_with_key {
            continue;
        }

        // e.g., bind `gsuu` to `git submodule update`
        // TODO: Do this recursively
        for (sub_sub_long, sub_sub) in &sub.subs.0 {
            let k = format!("{sub_short}{}", sub_sub.short);
            if !cmd.subs.0.contains_key(&k) {
                bind(
                    format!("{pfx}{short}{k}"),
                    format!("{pfx}{long} {sub_long} {sub_sub_long} "),
                );
            }
        }
        for (f, fl) in &sub.flags {
            if fl.squish {
                bind(
                    format!("{pfx}{short}{sub_short}{}", fl.short),
                    format!("{pfx}{long} {sub_long} {f} "),
                );
            }
        }
    }
    for (sub_long, sub) in &cmd.subs.0 {
        debug!("considering binding sub: {sub_long}");
        if !all && doesnt_start_with_prefix {
            continue;
        }
        debug!("binding sub: {sub_long}");
        let prefix = format!("{pfx}{long}");
        for (short, long) in compile_recursive(prefix, sub, sub_long, lbuf, all) {
            bind(short, long);
        }
    }
    m
}

pub(super) fn compile(cmds: &Cmds, lbuf: &str, all: bool) -> BTreeMap<String, String> {
    let mut r = BTreeMap::new();
    for (long, cmd) in &cmds.0 {
        if lbuf.is_empty() || lbuf.starts_with(&cmd.short) || lbuf.starts_with(long) {
            r.extend(compile_recursive(String::new(), cmd, long, lbuf, all))
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
