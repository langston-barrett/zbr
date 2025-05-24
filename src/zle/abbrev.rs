use std::collections::{HashMap, HashSet};

use tracing::debug;

fn all_but_last(s: &str) -> String {
    if s.len() <= 1 {
        return s.to_string();
    }
    let mut r = String::with_capacity(s.len() - 1);
    for (i, c) in s.chars().enumerate() {
        if i == s.len() - 1 {
            break;
        }
        r.push(c);
    }
    r
}

// Once strings have been shortened using `unique_prefixes`, shorten them
// further.
pub(super) fn shorten_unique_prefixes(
    pfxs: &[String],
    denylist: &HashSet<&str>,
) -> HashMap<String, String> {
    debug_assert!(pfxs.len() == HashSet::<&String>::from_iter(pfxs).len());
    debug!("Denylist: {denylist:?}");
    let mut result = HashMap::with_capacity(pfxs.len());
    let mut taken = HashSet::<String>::from_iter(pfxs.iter().cloned());
    for pfx in pfxs {
        if pfx.len() <= 2 || result.contains_key(pfx) {
            continue;
        }
        let mut chars = pfx.chars();
        let first = chars.next().unwrap();
        let but_last = all_but_last(pfx);
        let len = pfx.len();
        let similar = pfxs
            .iter()
            .filter(|p| p.starts_with(&but_last) && p.len() == len)
            .collect::<Vec<_>>();
        debug!("Similar: {similar:?}");
        let mut saved = String::from(first);
        'outer: loop {
            if saved.len() + 1 == pfx.len() {
                break;
            }
            let mut tentative = Vec::<(String, String)>::with_capacity(similar.len());
            for sim in &similar {
                let last = sim.chars().last().unwrap();
                let new = format!("{saved}{last}");
                debug!("Abbreviating {sim} as {new}");
                if taken.contains(new.as_str()) || denylist.contains(new.as_str()) {
                    let next_char = sim.chars().nth(saved.len()).unwrap();
                    saved.push(next_char);
                    continue 'outer;
                } else {
                    debug_assert!(new.chars().all(|c| sim.contains(c)));
                    tentative.push((sim.to_string(), new));
                }
            }
            debug_assert_eq!(tentative.len(), similar.len());
            for (_, new) in &tentative {
                taken.insert(new.clone());
            }
            result.extend(tentative);
            break;
        }
    }
    debug_assert!(result.len() <= pfxs.len());
    result
}

pub(super) fn unique_prefixes(
    strings: &[String],
    denylist: &HashSet<&str>,
) -> HashMap<String, String> {
    // debug_assert!(strings.len() == HashSet::<&String>::from_iter(strings).len());
    let mut result = HashMap::with_capacity(strings.len());
    for string in strings.iter() {
        let mut pfx = String::new();
        for c in string.chars() {
            pfx.push(c);
            if denylist.contains(pfx.as_str()) {
                continue;
            }
            let is_unique = strings.iter().filter(|s| s.starts_with(&pfx)).count() == 1;
            if is_unique {
                break;
            }
        }
        result.insert(string.clone(), pfx);
    }
    // TODO: Why not equal?
    debug_assert!(result.len() <= strings.len());
    result
}

fn is_vowel(c: char) -> bool {
    c == 'a' || c == 'e' || c == 'i' || c == 'o' || c == 'u'
}

fn remove_vowels(strings: &[String]) -> HashMap<&str, String> {
    let mut result = HashMap::with_capacity(strings.len());
    let mut seen = HashSet::with_capacity(strings.len());
    for string in strings.iter() {
        if string.is_empty() {
            continue;
        }
        let first = string.chars().next().unwrap();
        if is_vowel(first) {
            continue;
        }
        let vowelless = string.chars().filter(|c| !is_vowel(*c)).collect::<String>();
        if seen.contains(&vowelless) {
            result.remove(string.as_str());
            continue;
        }
        result.insert(string.as_str(), vowelless.clone());
        seen.insert(vowelless);
    }
    debug_assert_eq!(strings.len(), result.len());
    result
}

pub(super) fn do_remove_vowels(strings: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(strings.len());
    let mut shortened = remove_vowels(strings);
    for s in strings {
        result.push(if let Some(shorter) = shortened.remove(s.as_str()) {
            shorter
        } else {
            s.clone()
        });
    }
    debug_assert_eq!(strings.len(), result.len());
    result
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use super::shorten_unique_prefixes;

    #[test]
    fn test_shorten_unique() {
        let denylist = HashSet::new();
        assert_eq!(shorten_unique_prefixes(&[], &denylist), HashMap::new());
        assert_eq!(
            BTreeMap::from_iter(
                shorten_unique_prefixes(
                    &[
                        String::from("stac"), // stack
                        String::from("star"), // start
                        String::from("stat"), // stats
                    ],
                    &denylist
                )
                .iter()
            )
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
            [("stac", "sc"), ("star", "sr"), ("stat", "st")]
        );
        assert_eq!(
            BTreeMap::from_iter(
                shorten_unique_prefixes(
                    &[
                        String::from("stac"), // stack
                        String::from("star"), // start
                        String::from("stat"), // stats
                    ],
                    &HashSet::from(["sc"])
                )
                .iter()
            )
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect::<Vec<_>>(),
            [("stac", "stc"), ("star", "str"), ("stat", "stt")]
        );
    }
}
