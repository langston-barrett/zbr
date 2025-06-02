use std::collections::BTreeMap;

use super::compile::compile_with_prefixes;
use super::expand;

// TODO: handle subcommands properly

fn make_case(prefix: &str, compiled: &BTreeMap<&str, &str>) {
    println!("function {prefix}() {{");
    println!("  case $* in");
    for (short, long) in compiled {
        let words = short.split_ascii_whitespace().collect::<Vec<_>>();
        if words.len() != 2 {
            continue;
        }
        println!("    {}) shift 1; command {long} \"$@\" ;;", words[1]);
    }
    println!("    *) command {prefix} \"$@\"");
    println!("  esac");
    println!("}}");
}

pub(super) fn go(conf: expand::ConfigFile) {
    let mut compiled = compile_with_prefixes(&conf.cmds, "", true)
        .into_iter()
        .collect::<Vec<_>>();
    compiled.sort();
    for (cmd_long, cmd) in conf.cmds.0 {
        let mut multi_word = BTreeMap::new();
        for (short, long) in &compiled {
            // println!("COMP {short} {long}");
            let long = long.trim_end();
            if !long.starts_with(&cmd_long) {
                continue;
            }
            if *short != cmd.short && !short.contains(' ') {
                println!("alias {short}=\"{long}\"");
            } else if short.starts_with(&cmd_long) {
                multi_word.insert(short.as_str(), long);
            }
        }
        make_case(&cmd_long, &multi_word);
        println!("function {}() {{ {cmd_long} \"$@\"; }}", cmd.short);
    }
}
