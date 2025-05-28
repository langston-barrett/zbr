use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
};

use tracing::{debug, warn};

use crate::zle::abbrev;

#[derive(Debug, clap::Parser)]
pub struct Config {
    cmd: String,
    conf: Option<PathBuf>,
    #[clap(long)]
    print_subs: bool,
}

/// For use with serde's [serialize_with] attribute
fn ordered_map<S: serde::Serializer, K: Ord + serde::Serialize, V: serde::Serialize>(
    value: &HashMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let ordered: BTreeMap<_, _> = value.iter().collect();
    serde::Serialize::serialize(&ordered, serializer)
}

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub(super) struct Cmds(pub(super) BTreeMap<String, Cmd>);

impl Cmds {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(super) struct Cmd {
    pub(super) short: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    #[serde(serialize_with = "ordered_map")]
    pub(super) flags: HashMap<String, Flag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub(super) no_args: bool,
    #[serde(default)]
    #[serde(skip_serializing_if = "Cmds::is_empty")]
    pub(super) subs: Cmds,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == T::default()
}

#[derive(
    Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub(super) struct Flag {
    pub(super) short: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default")]
    pub(super) squish: bool,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigFile {
    short: Option<String>,
    #[serde(default)]
    devowel: bool,
    #[serde(default)]
    deny: Vec<String>,
    #[serde(default)]
    extract_flags: bool,
    #[serde(default)]
    extract_subs: bool,
    #[serde(default)]
    exact_subs: Vec<String>,
    #[serde(default)]
    extra_subs: Vec<String>,
    #[serde(default)]
    flags: HashMap<String, Flag>,
    #[serde(default)]
    no_args: bool,
    #[serde(default)]
    stop: bool,
    #[serde(default)]
    subs: HashMap<String, ConfigFile>,
}

impl ConfigFile {
    pub(super) fn from_file<P: AsRef<Path>>(p: P) -> Self {
        toml::from_str::<ConfigFile>(&std::fs::read_to_string(p).unwrap()).unwrap()
    }
}

// TODO: Optionally add a prefix '-' to all short versions of flags, and
// deconflict separately
fn deconflict(
    conf: &ConfigFile,
    flags: &[String],
    subs: &[String],
    deny: &[String],
) -> HashMap<String, String> {
    debug_assert!(flags.len() == HashSet::<&String>::from_iter(flags).len());
    debug_assert!(subs.len() == HashSet::<&String>::from_iter(subs).len());
    debug_assert!(conf.flags.len() <= flags.len());

    let total_len = flags.len() + subs.len();
    let mut result = HashMap::with_capacity(total_len);
    let mut rest = Vec::with_capacity(total_len - conf.flags.len());
    let mut denylist = HashSet::<&str>::with_capacity(conf.flags.len() + deny.len());
    denylist.extend(deny.iter().map(String::as_str));

    for flag in flags.iter() {
        if let Some(flag_conf) = conf.flags.get(flag) {
            let short = &flag_conf.short;
            debug_assert!(!result.contains_key(flag));
            result.insert(flag.clone(), short.clone());
            debug_assert!(!denylist.contains(short.as_str()));
            denylist.insert(short.as_str());
        } else {
            debug_assert!(!rest.contains(flag));
            rest.push(flag.clone());
        }
    }
    debug_assert!(rest.len() == HashSet::<&String>::from_iter(rest.iter()).len());

    let mut rest_subs = Vec::with_capacity(total_len - conf.flags.len());
    for sub in subs.iter() {
        if let Some(sub_conf) = conf.subs.get(sub) {
            if let Some(short) = &sub_conf.short {
                debug_assert!(!result.contains_key(sub));
                result.insert(sub.clone(), short.clone());
                debug_assert!(!denylist.contains(short.as_str()));
                denylist.insert(short.as_str());
            } else {
                // TODO: How to account for flags and subcommands with the same name?
                // debug_assert!(!rest.contains(sub));
                rest_subs.push(sub.clone());
            }
        } else {
            // TODO: How to account for flags and subcommands with the same name?
            // debug_assert!(!rest.contains(sub));
            rest_subs.push(sub.clone());
        }
    }
    debug_assert!(rest_subs.len() == HashSet::<&String>::from_iter(rest_subs.iter()).len());
    rest.extend(rest_subs);

    if conf.devowel {
        let rmvd = abbrev::do_remove_vowels(rest.as_slice());
        let pfxs = abbrev::unique_prefixes(&rmvd, &denylist);
        debug_assert_eq!(rmvd.len(), rest.len());
        for (rmd, long) in rmvd.iter().zip(rest.iter()) {
            result.insert(long.clone(), pfxs.get(rmd).unwrap().clone());
        }
    } else {
        let pfxs_map = abbrev::unique_prefixes(&rest, &denylist);
        let mut pfxs = pfxs_map.values().cloned().collect::<Vec<_>>();
        pfxs.sort();
        let shorter_map = abbrev::shorten_unique_prefixes(pfxs.as_slice(), &denylist);
        result.extend(
            pfxs_map
                .into_iter()
                .map(|(s, pfx)| (s, shorter_map.get(&pfx).cloned().unwrap_or(pfx.clone()))),
        );
    }
    result
}

fn extract_sub(words: &[&str]) -> Option<String> {
    if words.len() < 2 {
        return None;
    }
    let first = &words[0];
    debug_assert!(!first.is_empty());
    let first_char = first.chars().next().unwrap();

    if first_char.is_lowercase()
            // The description comes immediately after the subcommand, or, in the
            // case of cargo aliases, just after the alias. The description
            // usually starts with a capital.
            && (words[1].chars().next().unwrap().is_uppercase()
                || (words.len() > 2 && words[2].chars().next().unwrap().is_uppercase()))
            && first.is_ascii()
    {
        let long = first
            .chars()
            .filter(|c| *c == '-' || c.is_alphanumeric())
            .collect::<String>();
        return Some(long);
    }
    None
}

#[derive(Debug, Eq, Hash, PartialEq, PartialOrd)]
struct Opt {
    #[allow(dead_code)]
    short: Option<String>,
    long: String,
}

fn extract_opt(mut words: &[&str]) -> Option<Opt> {
    const LONG: &[char] = &['-', '-'];
    if words.is_empty() {
        return None;
    }
    let first = &words[0];
    let short = if first.starts_with('-') && !first.starts_with(LONG) {
        words = &words[1..];
        let mut chars = first.chars();
        chars.next();
        Some(chars.collect::<String>())
    } else {
        None
    };

    while let Some((first, rest)) = words.split_first() {
        words = rest;
        if !first.starts_with(LONG) || first.len() <= LONG.len() {
            continue;
        }
        debug!("Found potential option (`--`) in {words:?}");
        let mut opt = &first[LONG.len()..];
        if opt.starts_with('-') {
            continue;
        }
        for delim in ['=', '[', ']'] {
            if let Some(idx) = opt.find(delim) {
                opt = &opt[..idx];
            }
        }
        let long = opt
            .chars()
            .filter(|c| *c == '-' || c.is_alphanumeric())
            .collect::<String>();
        if long.len() <= 2 {
            continue;
        }
        return Some(Opt { short, long });
    }
    None
}

fn extract_text(conf: &ConfigFile, text: String) -> (HashMap<String, Flag>, Cmds) {
    let mut opts = HashSet::new();
    let mut sub_names = HashSet::<String>::from_iter(conf.extra_subs.iter().cloned());
    if conf.extract_subs {
        for mut line in text.lines() {
            if !line.starts_with([' ', ' ']) {
                continue;
            }
            line = line.trim_start();
            let words = line.split_whitespace().collect::<Vec<_>>();
            if let Some(long) = extract_sub(words.as_slice()) {
                sub_names.insert(long);
            }
        }
    }
    if conf.extract_flags {
        for mut line in text.lines() {
            line = line.trim_start();
            let words = line.split_whitespace().collect::<Vec<_>>();
            if let Some(opt) = extract_opt(words.as_slice()) {
                opts.insert(opt);
            }
        }
    }

    if !conf.exact_subs.is_empty() {
        sub_names = HashSet::<String>::from_iter(conf.exact_subs.iter().cloned());
    }

    let sub_name_vec = Vec::from_iter(sub_names.iter().cloned());
    opts.extend(conf.flags.iter().map(|(long, flag)| Opt {
        short: Some(flag.short.clone()),
        long: long.clone(),
    }));
    let opt_names = opts.into_iter().map(|o| o.long).collect::<Vec<_>>();
    let deconflicted = deconflict(
        conf,
        opt_names.as_slice(),
        sub_name_vec.as_slice(),
        conf.deny.as_slice(),
    );

    let mut subs = Cmds(BTreeMap::new());
    for long in sub_names {
        let short = deconflicted.get(&long).unwrap().clone();
        if long == short {
            debug!("Couldn't abbreviate {short}");
        }
        subs.0.insert(
            long,
            Cmd {
                short,
                flags: HashMap::new(),
                no_args: conf.no_args,
                subs: Cmds::default(),
            },
        );
    }

    let mut flags = HashMap::<String, Flag>::new();
    for mut long in opt_names {
        let short = deconflicted.get(&long).unwrap().clone();
        if long == short {
            debug!("Couldn't abbreviate {short}");
        }
        if let Some(f) = flags.get(&short) {
            assert_eq!(f.short, long);
        }
        let flag = Flag {
            short,
            squish: conf.flags.get(&long).map(|f| f.squish).unwrap_or(false),
        };
        if !long.starts_with(['-', '-']) {
            long = format!("--{long}");
        }
        flags.insert(long, flag);
    }
    subs = if conf.stop { Cmds::default() } else { subs };
    (flags, subs)
}

fn help(args: &[String]) -> Option<String> {
    let mut builder = Command::new(&args[0]);
    builder.args(&args[1..]).arg("--help");
    debug!("Running {builder:?}");
    let output = builder.output().unwrap();
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

pub(super) fn extract_recursive(
    mut prefix: Vec<String>,
    conf: ConfigFile,
    long: String,
) -> Option<Cmd> {
    prefix.push(long.clone());
    let h = if conf.extract_subs || conf.extract_flags {
        help(&prefix)?
    } else {
        String::new()
    };
    let (flags, subs0) = extract_text(&conf, h);

    let mut subs = Cmds(BTreeMap::new());
    for (long, sub0) in subs0.0 {
        let sub_conf = conf.subs.get(&long).cloned().unwrap_or_default();
        if let Some(mut sub) = extract_recursive(prefix.clone(), sub_conf, long.clone()) {
            sub.short = sub0.short; // already deconflicted
            subs.0.insert(long, sub);
        } else {
            subs.0.insert(long, sub0);
        }
    }

    if conf.no_args && !subs.is_empty() {
        warn!("`no_args` specified, but {long} has subcommands");
    }
    Some(Cmd {
        short: conf
            .short
            .unwrap_or_else(|| String::from(long.chars().next().unwrap())),
        flags,
        no_args: conf.no_args,
        subs,
    })
}

pub(super) fn extract(conf: ConfigFile, long: String) -> Option<Cmd> {
    // conf.extract_flags = true;
    // conf.extract_subs = true;
    extract_recursive(Vec::new(), conf, long)
}

pub(super) fn go(conf: Config) {
    let conf_file = if let Some(conf) = conf.conf {
        ConfigFile::from_file(conf)
    } else {
        ConfigFile::default()
    };
    if let Some(extracted) = extract(conf_file, conf.cmd.clone()) {
        if conf.print_subs {
            let subs = BTreeMap::from_iter(extracted.subs.0);
            for (long, sub) in subs {
                println!("{long} --> {}", sub.short);
            }
        } else {
            let gen_conf = super::expand::ConfigFile {
                cmds: Cmds(BTreeMap::from([(conf.cmd, extracted)])),
            };
            println!("{}", toml::to_string(&gen_conf).unwrap())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use expect_test::expect;

    use super::{ConfigFile, deconflict, extract_text};

    const CABAL_HELP: &str = r#"
Command line interface to the Haskell Cabal infrastructure.

See http://www.haskell.org/cabal/ for more information.

Usage: cabal [GLOBAL FLAGS] [COMMAND [FLAGS]]

Commands:
 [global]
  user-config            Display and update the user's global cabal configuration.
  help                   Help about commands.

 [package database]
  update                 Updates list of known packages.
  list                   List packages matching a search string.
  info                   Display detailed information about a particular package.

 [initialization and download]
  init                   Create a new cabal package.
  fetch                  Downloads packages for later installation.
  get                    Download/Extract a package's source code (repository).

 [project configuration]
  configure              Add extra project configuration.
  freeze                 Freeze dependencies.
  gen-bounds             Generate dependency bounds.
  outdated               Check for outdated dependencies.

 [project building and installing]
  build                  Compile targets within the project.
  install                Install packages.
  haddock                Build Haddock documentation.
  haddock-project        Generate Haddocks HTML documentation for the cabal project.
  clean                  Clean the package store and remove temporary files.

 [running and testing]
  list-bin               List the path to a single executable.
  repl                   Open an interactive session for the given component.
  run                    Run an executable.
  bench                  Run benchmarks.
  test                   Run test-suites.
  exec                   Give a command access to the store.

 [sanity checks and shipping]
  check                  Check the package for common mistakes.
  sdist                  Generate a source distribution file (.tar.gz).
  upload                 Uploads source packages or documentation to Hackage.
  report                 Upload build reports to a remote server.

 [deprecated]
  unpack                 Deprecated alias for 'get'.
  hscolour               Generate HsColour colourised code, in HTML format.

 [new-style projects (forwards-compatible aliases)]
  v2-build               Compile targets within the project.
  v2-configure           Add extra project configuration.
  v2-repl                Open an interactive session for the given component.
  v2-run                 Run an executable.
  v2-test                Run test-suites.
  v2-bench               Run benchmarks.
  v2-freeze              Freeze dependencies.
  v2-haddock             Build Haddock documentation.
  v2-exec                Give a command access to the store.
  v2-update              Updates list of known packages.
  v2-install             Install packages.
  v2-clean               Clean the package store and remove temporary files.
  v2-sdist               Generate a source distribution file (.tar.gz).

 [legacy command aliases]
  v1-build               Compile all/specific components.
  v1-configure           Prepare to build the package.
  v1-repl                Open an interpreter session for the given component.
  v1-run                 Builds and runs an executable.
  v1-test                Run all/specific tests in the test suite.
  v1-bench               Run all/specific benchmarks.
  v1-freeze              Freeze dependencies.
  v1-haddock             Generate Haddock HTML documentation.
  v1-install             Install packages.
  v1-clean               Clean up after a build.
  v1-copy                Copy the files of all/specific components to install locations.
  v1-register            Register this package with the compiler.
  v1-reconfigure         Reconfigure the package if necessary.

 [other]
  haddock-project        Generate Haddocks HTML documentation for the cabal project.
  new-haddock-project    Generate Haddocks HTML documentation for the cabal project.
  v2-haddock-project     Generate Haddocks HTML documentation for the cabal project.

For more information about a command use:
   cabal COMMAND --help
or cabal help COMMAND

To install Cabal packages from hackage use:
  cabal install foo [--dry-run]

Occasionally you need to update the list of available packages:
  cabal update

Global flags:
 -h, --help                     Show this help text
 -V, --version                  Print version information
 --numeric-version              Print just the version number
 --config-file=FILE             Set an alternate location for the config file
 --ignore-expiry                Ignore expiry dates on signed metadata (use
                                only in exceptional circumstances)
 --http-transport=HttpTransport
                                Set a transport for http(s) requests. Accepts
                                'curl', 'wget', 'powershell', and
                                'plain-http'. (default: 'curl')
 --nix[=(True or False)]        Nix integration: run commands through
                                nix-shell if a 'shell.nix' file exists
                                (default is False)
 --enable-nix                   Enable Nix integration: run commands through
                                nix-shell if a 'shell.nix' file exists
 --disable-nix                  Disable Nix integration
 --store-dir=DIR                The location of the build store
 --active-repositories=REPOS    The active package repositories (set to
                                ':none' to disable all repositories)"#;

    const CARGO_HELP: &str = r#"
Rust's package manager

Usage: cargo [+toolchain] [OPTIONS] [COMMAND]

Options:
  -V, --version             Print version info and exit
      --list                List installed commands
      --explain <CODE>      Run `rustc --explain CODE`
  -v, --verbose...          Use verbose output (-vv very verbose/build.rs output)
  -q, --quiet               Do not print cargo log messages
      --color <WHEN>        Coloring: auto, always, never
  -C <DIRECTORY>            Change to DIRECTORY before doing anything (nightly-only)
      --frozen              Require Cargo.lock and cache are up to date
      --locked              Require Cargo.lock is up to date
      --offline             Run without accessing the network
      --config <KEY=VALUE>  Override a configuration value
  -Z <FLAG>                 Unstable (nightly-only) flags to Cargo, see 'cargo -Z help' for details
  -h, --help                Print help

Some common cargo commands are (see all commands with --list):
    build, b    Compile the current package
    check, c    Analyze the current package and report errors, but don't build object files
    clean       Remove the target directory
    doc, d      Build this package's and its dependencies' documentation
    new         Create a new cargo package
    init        Create a new cargo package in an existing directory
    add         Add dependencies to a manifest file
    remove      Remove dependencies from a manifest file
    run, r      Run a binary or example of the local package
    test, t     Run the tests
    bench       Run the benchmarks
    update      Update dependencies listed in Cargo.lock
    search      Search registry for crates
    publish     Package and upload this package to the registry
    install     Install a Rust binary. Default location is $HOME/.cargo/bin
    uninstall   Uninstall a Rust binary

See 'cargo help <command>' for more information on a specific command."#;

    const DOCKER_HELP: &str = r#"
Usage:  docker [OPTIONS] COMMAND

A self-sufficient runtime for containers

Options:
      --config string      Location of client config files (default "/home/langston/.docker")
  -c, --context string     Name of the context to use to connect to the daemon (overrides
                           DOCKER_HOST env var and default context set with "docker context use")
  -D, --debug              Enable debug mode
  -H, --host list          Daemon socket(s) to connect to
  -l, --log-level string   Set the logging level ("debug"|"info"|"warn"|"error"|"fatal") (default
                           "info")
      --tls                Use TLS; implied by --tlsverify
      --tlscacert string   Trust certs signed only by this CA (default
                           "/home/langston/.docker/ca.pem")
      --tlscert string     Path to TLS certificate file (default "/home/langston/.docker/cert.pem")
      --tlskey string      Path to TLS key file (default "/home/langston/.docker/key.pem")
      --tlsverify          Use TLS and verify the remote
  -v, --version            Print version information and quit

Management Commands:
  builder     Manage builds
  buildx*     Docker Buildx (Docker Inc., 0.0.0+unknown)
  compose*    Docker Compose (Docker Inc., 2.5.1)
  config      Manage Docker configs
  container   Manage containers
  context     Manage contexts
  image       Manage images
  manifest    Manage Docker image manifests and manifest lists
  network     Manage networks
  node        Manage Swarm nodes
  plugin      Manage plugins
  secret      Manage Docker secrets
  service     Manage services
  stack       Manage Docker stacks
  swarm       Manage Swarm
  system      Manage Docker
  trust       Manage trust on Docker images
  volume      Manage volumes

Commands:
  attach      Attach local standard input, output, and error streams to a running container
  build       Build an image from a Dockerfile
  commit      Create a new image from a container's changes
  cp          Copy files/folders between a container and the local filesystem
  create      Create a new container
  diff        Inspect changes to files or directories on a container's filesystem
  events      Get real time events from the server
  exec        Run a command in a running container
  export      Export a container's filesystem as a tar archive
  history     Show the history of an image
  images      List images
  import      Import the contents from a tarball to create a filesystem image
  info        Display system-wide information
  inspect     Return low-level information on Docker objects
  kill        Kill one or more running containers
  load        Load an image from a tar archive or STDIN
  login       Log in to a Docker registry
  logout      Log out from a Docker registry
  logs        Fetch the logs of a container
  pause       Pause all processes within one or more containers
  port        List port mappings or a specific mapping for the container
  ps          List containers
  pull        Pull an image or a repository from a registry
  push        Push an image or a repository to a registry
  rename      Rename a container
  restart     Restart one or more containers
  rm          Remove one or more containers
  rmi         Remove one or more images
  run         Run a command in a new container
  save        Save one or more images to a tar archive (streamed to STDOUT by default)
  search      Search the Docker Hub for images
  start       Start one or more stopped containers
  stats       Display a live stream of container(s) resource usage statistics
  stop        Stop one or more running containers
  tag         Create a tag TARGET_IMAGE that refers to SOURCE_IMAGE
  top         Display the running processes of a container
  unpause     Unpause all processes within one or more containers
  update      Update configuration of one or more containers
  version     Show the Docker version information
  wait        Block until one or more containers stop, then print their exit codes

Run 'docker COMMAND --help' for more information on a command."#;

    const GIT_HELP: &str = "
usage: git [-v | --version] [-h | --help] [-C <path>] [-c <name>=<value>]
           [--exec-path[=<path>]] [--html-path] [--man-path] [--info-path]
           [-p | --paginate | -P | --no-pager] [--no-replace-objects] [--bare]
           [--git-dir=<path>] [--work-tree=<path>] [--namespace=<name>]
           [--super-prefix=<path>] [--config-env=<name>=<envvar>]
           <command> [<args>]

These are common Git commands used in various situations:

start a working area (see also: git help tutorial)
   clone     Clone a repository into a new directory
   init      Create an empty Git repository or reinitialize an existing one

work on the current change (see also: git help everyday)
   add       Add file contents to the index
   mv        Move or rename a file, a directory, or a symlink
   restore   Restore working tree files
   rm        Remove files from the working tree and from the index

examine the history and state (see also: git help revisions)
   bisect    Use binary search to find the commit that introduced a bug
   diff      Show changes between commits, commit and working tree, etc
   grep      Print lines matching a pattern
   log       Show commit logs
   show      Show various types of objects
   status    Show the working tree status

grow, mark and tweak your common history
   branch    List, create, or delete branches
   commit    Record changes to the repository
   merge     Join two or more development histories together
   rebase    Reapply commits on top of another base tip
   reset     Reset current HEAD to the specified state
   switch    Switch branches
   tag       Create, list, delete or verify a tag object signed with GPG

collaborate (see also: git help workflows)
   fetch     Download objects and refs from another repository
   pull      Fetch from and integrate with another repository or a local branch
   push      Update remote refs along with associated objects

'git help -a' and 'git help -g' list available subcommands and some
concept guides. See 'git help <command>' or 'git help <concept>'
to read about a specific subcommand or concept.
See 'git help git' for an overview of the system.";

    const GIT_SUBMODULE_HELP: &str = r#"GIT-SUBMODULE(1)                              Git Manual                             GIT-SUBMODULE(1)

NNAAMMEE
       git-submodule - Initialize, update or inspect submodules

SSYYNNOOPPSSIISS
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] [--cached]
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] add [<options>] [--] <repository> [<path>]
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] status [--cached] [--recursive] [--] [<path>...]
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] init [--] [<path>...]
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] deinit [-f|--force] (--all|[--] <path>...)
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] update [<options>] [--] [<path>...]
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] set-branch [<options>] [--] <path>
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] set-url [--] <path> <newurl>
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] summary [<options>] [--] [<path>...]
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] foreach [--recursive] <command>
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] sync [--recursive] [--] [<path>...]
       _g_i_t _s_u_b_m_o_d_u_l_e [--quiet] absorbgitdirs [--] [<path>...]

DDEESSCCRRIIPPTTIIOONN
       Inspects, updates and manages submodules.

       For more information about submodules, see ggiittssuubbmmoodduulleess(7).

CCOOMMMMAANNDDSS
       With no arguments, shows the status of existing submodules. Several subcommands are available
       to perform operations on the submodules.

       add [-b <branch>] [-f|--force] [--name <name>] [--reference <repository>] [--depth <depth>]
       [--] <repository> [<path>]
           Add the given repository as a submodule at the given path to the changeset to be committed
           next to the current project: the current project is termed the "superproject".

           <repository> is the URL of the new submodule’s origin repository. This may be either an
           absolute URL, or (if it begins with ./ or ../), the location relative to the
           superproject’s default remote repository (Please note that to specify a repository _f_o_o_._g_i_t
           which is located right next to a superproject _b_a_r_._g_i_t, you’ll have to use ....//ffoooo..ggiitt
           instead of ..//ffoooo..ggiitt - as one might expect when following the rules for relative URLs -
           because the evaluation of relative URLs in Git is identical to that of relative
           directories).

           The default remote is the remote of the remote-tracking branch of the current branch. If
           no such remote-tracking branch exists or the HEAD is detached, "origin" is assumed to be
           the default remote. If the superproject doesn’t have a default remote configured the
           superproject is its own authoritative upstream and the current working directory is used
           instead.

           The optional argument <path> is the relative location for the cloned submodule to exist in
           the superproject. If <path> is not given, the canonical part of the source repository is
           used ("repo" for "/path/to/repo.git" and "foo" for "host.xz:foo/.git"). If <path> exists
           and is already a valid Git repository, then it is staged for commit without cloning. The
           <path> is also used as the submodule’s logical name in its configuration entries unless
           ----nnaammee is used to specify a logical name.

           The given URL is recorded into ..ggiittmmoodduulleess for use by subsequent users cloning the
           superproject. If the URL is given relative to the superproject’s repository, the
           presumption is the superproject and submodule repositories will be kept together in the
           same relative location, and only the superproject’s URL needs to be provided.
           git-submodule will correctly locate the submodule using the relative URL in ..ggiittmmoodduulleess.

       status [--cached] [--recursive] [--] [<path>...]
           Show the status of the submodules. This will print the SHA-1 of the currently checked out
           commit for each submodule, along with the submodule path and the output of _g_i_t _d_e_s_c_r_i_b_e
           for the SHA-1. Each SHA-1 will possibly be prefixed with -- if the submodule is not
           initialized, ++ if the currently checked out submodule commit does not match the SHA-1
           found in the index of the containing repository and UU if the submodule has merge
           conflicts.

           If ----ccaacchheedd is specified, this command will instead print the SHA-1 recorded in the
           superproject for each submodule.

           If ----rreeccuurrssiivvee is specified, this command will recurse into nested submodules, and show
           their status as well.

           If you are only interested in changes of the currently initialized submodules with respect
           to the commit recorded in the index or the HEAD, ggiitt--ssttaattuuss(1) and ggiitt--ddiiffff(1) will
           provide that information too (and can also report changes to a submodule’s work tree).

       init [--] [<path>...]
           Initialize the submodules recorded in the index (which were added and committed elsewhere)
           by setting ssuubbmmoodduullee..$$nnaammee..uurrll in .git/config. It uses the same setting from ..ggiittmmoodduulleess
           as a template. If the URL is relative, it will be resolved using the default remote. If
           there is no default remote, the current repository will be assumed to be upstream.

           Optional <path> arguments limit which submodules will be initialized. If no path is
           specified and submodule.active has been configured, submodules configured to be active
           will be initialized, otherwise all submodules are initialized.

           When present, it will also copy the value of ssuubbmmoodduullee..$$nnaammee..uuppddaattee. This command does not
           alter existing information in .git/config. You can then customize the submodule clone URLs
           in .git/config for your local setup and proceed to ggiitt ssuubbmmoodduullee uuppddaattee; you can also just
           use ggiitt ssuubbmmoodduullee uuppddaattee ----iinniitt without the explicit _i_n_i_t step if you do not intend to
           customize any submodule locations.

           See the add subcommand for the definition of default remote.

       deinit [-f|--force] (--all|[--] <path>...)
           Unregister the given submodules, i.e. remove the whole ssuubbmmoodduullee..$$nnaammee section from
           .git/config together with their work tree. Further calls to ggiitt ssuubbmmoodduullee uuppddaattee, ggiitt
           ssuubbmmoodduullee ffoorreeaacchh and ggiitt ssuubbmmoodduullee ssyynncc will skip any unregistered submodules until they
           are initialized again, so use this command if you don’t want to have a local checkout of
           the submodule in your working tree anymore.

           When the command is run without pathspec, it errors out, instead of deinit-ing everything,
           to prevent mistakes.

           If ----ffoorrccee is specified, the submodule’s working tree will be removed even if it contains
           local modifications.

           If you really want to remove a submodule from the repository and commit that use ggiitt--rrmm(1)
           instead. See ggiittssuubbmmoodduulleess(7) for removal options.

       update [--init] [--remote] [-N|--no-fetch] [--[no-]recommend-shallow] [-f|--force]
       [--checkout|--rebase|--merge] [--reference <repository>] [--depth <depth>] [--recursive]
       [--jobs <n>] [--[no-]single-branch] [--filter <filter spec>] [--] [<path>...]
           Update the registered submodules to match what the superproject expects by cloning missing
           submodules, fetching missing commits in submodules and updating the working tree of the
           submodules. The "updating" can be done in several ways depending on command line options
           and the value of ssuubbmmoodduullee..<<nnaammee>>..uuppddaattee configuration variable. The command line option
           takes precedence over the configuration variable. If neither is given, a _c_h_e_c_k_o_u_t is
           performed. The _u_p_d_a_t_e procedures supported both from the command line as well as through
           the ssuubbmmoodduullee..<<nnaammee>>..uuppddaattee configuration are:

           checkout
               the commit recorded in the superproject will be checked out in the submodule on a
               detached HEAD.

               If ----ffoorrccee is specified, the submodule will be checked out (using ggiitt cchheecckkoouutt
               ----ffoorrccee), even if the commit specified in the index of the containing repository
               already matches the commit checked out in the submodule.

           rebase
               the current branch of the submodule will be rebased onto the commit recorded in the
               superproject.

           merge
               the commit recorded in the superproject will be merged into the current branch in the
               submodule.

           The following _u_p_d_a_t_e procedures are only available via the ssuubbmmoodduullee..<<nnaammee>>..uuppddaattee
           configuration variable:

           custom command
               arbitrary shell command that takes a single argument (the sha1 of the commit recorded
               in the superproject) is executed. When ssuubbmmoodduullee..<<nnaammee>>..uuppddaattee is set to _!_c_o_m_m_a_n_d, the
               remainder after the exclamation mark is the custom command.

           none
               the submodule is not updated.

           If the submodule is not yet initialized, and you just want to use the setting as stored in
           ..ggiittmmoodduulleess, you can automatically initialize the submodule with the ----iinniitt option.

           If ----rreeccuurrssiivvee is specified, this command will recurse into the registered submodules, and
           update any nested submodules within.

           If ----ffiilltteerr <<ffiilltteerr ssppeecc>> is specified, the given partial clone filter will be applied to
           the submodule. See ggiitt--rreevv--lliisstt(1) for details on filter specifications.

       set-branch (-b|--branch) <branch> [--] <path>, set-branch (-d|--default) [--] <path>
           Sets the default remote tracking branch for the submodule. The ----bbrraanncchh option allows the
           remote branch to be specified. The ----ddeeffaauulltt option removes the submodule.<name>.branch
           configuration key, which causes the tracking branch to default to the remote _H_E_A_D.

       set-url [--] <path> <newurl>
           Sets the URL of the specified submodule to <newurl>. Then, it will automatically
           synchronize the submodule’s new remote URL configuration.

       summary [--cached|--files] [(-n|--summary-limit) <n>] [commit] [--] [<path>...]
           Show commit summary between the given commit (defaults to HEAD) and working tree/index.
           For a submodule in question, a series of commits in the submodule between the given super
           project commit and the index or working tree (switched by ----ccaacchheedd) are shown. If the
           option ----ffiilleess is given, show the series of commits in the submodule between the index of
           the super project and the working tree of the submodule (this option doesn’t allow to use
           the ----ccaacchheedd option or to provide an explicit commit).

           Using the ----ssuubbmmoodduullee==lloogg option with ggiitt--ddiiffff(1) will provide that information too.

       foreach [--recursive] <command>
           Evaluates an arbitrary shell command in each checked out submodule. The command has access
           to the variables $name, $sm_path, $displaypath, $sha1 and $toplevel: $name is the name of
           the relevant submodule section in ..ggiittmmoodduulleess, $sm_path is the path of the submodule as
           recorded in the immediate superproject, $displaypath contains the relative path from the
           current working directory to the submodules root directory, $sha1 is the commit as
           recorded in the immediate superproject, and $toplevel is the absolute path to the
           top-level of the immediate superproject. Note that to avoid conflicts with _$_P_A_T_H on
           Windows, the _$_p_a_t_h variable is now a deprecated synonym of _$_s_m___p_a_t_h variable. Any
           submodules defined in the superproject but not checked out are ignored by this command.
           Unless given ----qquuiieett, foreach prints the name of each submodule before evaluating the
           command. If ----rreeccuurrssiivvee is given, submodules are traversed recursively (i.e. the given
           shell command is evaluated in nested submodules as well). A non-zero return from the
           command in any submodule causes the processing to terminate. This can be overridden by
           adding _|_| _: to the end of the command.

           As an example, the command below will show the path and currently checked out commit for
           each submodule:

               git submodule foreach 'echo $sm_path `git rev-parse HEAD`'

       sync [--recursive] [--] [<path>...]
           Synchronizes submodules' remote URL configuration setting to the value specified in
           ..ggiittmmoodduulleess. It will only affect those submodules which already have a URL entry in
           .git/config (that is the case when they are initialized or freshly added). This is useful
           when submodule URLs change upstream and you need to update your local repositories
           accordingly.

           ggiitt ssuubbmmoodduullee ssyynncc synchronizes all submodules while ggiitt ssuubbmmoodduullee ssyynncc ---- AA synchronizes
           submodule "A" only.

           If ----rreeccuurrssiivvee is specified, this command will recurse into the registered submodules, and
           sync any nested submodules within.

       absorbgitdirs
           If a git directory of a submodule is inside the submodule, move the git directory of the
           submodule into its superproject’s $$GGIITT__DDIIRR//mmoodduulleess path and then connect the git directory
           and its working directory by setting the ccoorree..wwoorrkkttrreeee and adding a .git file pointing to
           the git directory embedded in the superprojects git directory.

           A repository that was cloned independently and later added as a submodule or old setups
           have the submodules git directory inside the submodule instead of embedded into the
           superprojects git directory.

           This command is recursive by default.

OOPPTTIIOONNSS
       -q, --quiet
           Only print error messages.

       --progress
           This option is only valid for add and update commands. Progress status is reported on the
           standard error stream by default when it is attached to a terminal, unless -q is
           specified. This flag forces progress status even if the standard error stream is not
           directed to a terminal.

       --all
           This option is only valid for the deinit command. Unregister all submodules in the working
           tree.

       -b <branch>, --branch <branch>
           Branch of repository to add as submodule. The name of the branch is recorded as
           ssuubbmmoodduullee..<<nnaammee>>..bbrraanncchh in ..ggiittmmoodduulleess for uuppddaattee ----rreemmoottee. A special value of ..  is used
           to indicate that the name of the branch in the submodule should be the same name as the
           current branch in the current repository. If the option is not specified, it defaults to
           the remote _H_E_A_D.

       -f, --force
           This option is only valid for add, deinit and update commands. When running add, allow
           adding an otherwise ignored submodule path. When running deinit the submodule working
           trees will be removed even if they contain local changes. When running update (only
           effective with the checkout procedure), throw away local changes in submodules when
           switching to a different commit; and always run a checkout operation in the submodule,
           even if the commit listed in the index of the containing repository matches the commit
           checked out in the submodule.

       --cached
           This option is only valid for status and summary commands. These commands typically use
           the commit found in the submodule HEAD, but with this option, the commit stored in the
           index is used instead.

       --files
           This option is only valid for the summary command. This command compares the commit in the
           index with that in the submodule HEAD when this option is used.

       -n, --summary-limit
           This option is only valid for the summary command. Limit the summary size (number of
           commits shown in total). Giving 0 will disable the summary; a negative number means
           unlimited (the default). This limit only applies to modified submodules. The size is
           always limited to 1 for added/deleted/typechanged submodules.

       --remote
           This option is only valid for the update command. Instead of using the superproject’s
           recorded SHA-1 to update the submodule, use the status of the submodule’s remote-tracking
           branch. The remote used is branch’s remote (bbrraanncchh..<<nnaammee>>..rreemmoottee), defaulting to oorriiggiinn.
           The remote branch used defaults to the remote HHEEAADD, but the branch name may be overridden
           by setting the ssuubbmmoodduullee..<<nnaammee>>..bbrraanncchh option in either ..ggiittmmoodduulleess or ..ggiitt//ccoonnffiigg (with
           ..ggiitt//ccoonnffiigg taking precedence).

           This works for any of the supported update procedures (----cchheecckkoouutt, ----rreebbaassee, etc.). The
           only change is the source of the target SHA-1. For example, ssuubbmmoodduullee uuppddaattee ----rreemmoottee
           ----mmeerrggee will merge upstream submodule changes into the submodules, while ssuubbmmoodduullee uuppddaattee
           ----mmeerrggee will merge superproject gitlink changes into the submodules.

           In order to ensure a current tracking branch state, uuppddaattee ----rreemmoottee fetches the
           submodule’s remote repository before calculating the SHA-1. If you don’t want to fetch,
           you should use ssuubbmmoodduullee uuppddaattee ----rreemmoottee ----nnoo--ffeettcchh.

           Use this option to integrate changes from the upstream subproject with your submodule’s
           current HEAD. Alternatively, you can run ggiitt ppuullll from the submodule, which is equivalent
           except for the remote branch name: uuppddaattee ----rreemmoottee uses the default upstream repository
           and ssuubbmmoodduullee..<<nnaammee>>..bbrraanncchh, while ggiitt ppuullll uses the submodule’s bbrraanncchh..<<nnaammee>>..mmeerrggee.
           Prefer ssuubbmmoodduullee..<<nnaammee>>..bbrraanncchh if you want to distribute the default upstream branch with
           the superproject and bbrraanncchh..<<nnaammee>>..mmeerrggee if you want a more native feel while working in
           the submodule itself.

       -N, --no-fetch
           This option is only valid for the update command. Don’t fetch new objects from the remote
           site.

       --checkout
           This option is only valid for the update command. Checkout the commit recorded in the
           superproject on a detached HEAD in the submodule. This is the default behavior, the main
           use of this option is to override ssuubbmmoodduullee..$$nnaammee..uuppddaattee when set to a value other than
           cchheecckkoouutt. If the key ssuubbmmoodduullee..$$nnaammee..uuppddaattee is either not explicitly set or set to
           cchheecckkoouutt, this option is implicit.

       --merge
           This option is only valid for the update command. Merge the commit recorded in the
           superproject into the current branch of the submodule. If this option is given, the
           submodule’s HEAD will not be detached. If a merge failure prevents this process, you will
           have to resolve the resulting conflicts within the submodule with the usual conflict
           resolution tools. If the key ssuubbmmoodduullee..$$nnaammee..uuppddaattee is set to mmeerrggee, this option is
           implicit.

       --rebase
           This option is only valid for the update command. Rebase the current branch onto the
           commit recorded in the superproject. If this option is given, the submodule’s HEAD will
           not be detached. If a merge failure prevents this process, you will have to resolve these
           failures with ggiitt--rreebbaassee(1). If the key ssuubbmmoodduullee..$$nnaammee..uuppddaattee is set to rreebbaassee, this
           option is implicit.

       --init
           This option is only valid for the update command. Initialize all submodules for which "git
           submodule init" has not been called so far before updating.

       --name
           This option is only valid for the add command. It sets the submodule’s name to the given
           string instead of defaulting to its path. The name must be valid as a directory name and
           may not end with a _/.

       --reference <repository>
           This option is only valid for add and update commands. These commands sometimes need to
           clone a remote repository. In this case, this option will be passed to the ggiitt--cclloonnee(1)
           command.

           NNOOTTEE: Do nnoott use this option unless you have read the note for ggiitt--cclloonnee(1)'s ----rreeffeerreennccee,
           ----sshhaarreedd, and ----ddiissssoocciiaattee options carefully.

       --dissociate
           This option is only valid for add and update commands. These commands sometimes need to
           clone a remote repository. In this case, this option will be passed to the ggiitt--cclloonnee(1)
           command.

           NNOOTTEE: see the NOTE for the ----rreeffeerreennccee option.

       --recursive
           This option is only valid for foreach, update, status and sync commands. Traverse
           submodules recursively. The operation is performed not only in the submodules of the
           current repo, but also in any nested submodules inside those submodules (and so on).

       --depth
           This option is valid for add and update commands. Create a _s_h_a_l_l_o_w clone with a history
           truncated to the specified number of revisions. See ggiitt--cclloonnee(1)

       --[no-]recommend-shallow
           This option is only valid for the update command. The initial clone of a submodule will
           use the recommended ssuubbmmoodduullee..<<nnaammee>>..sshhaallllooww as provided by the ..ggiittmmoodduulleess file by
           default. To ignore the suggestions use ----nnoo--rreeccoommmmeenndd--sshhaallllooww.

       -j <n>, --jobs <n>
           This option is only valid for the update command. Clone new submodules in parallel with as
           many jobs. Defaults to the ssuubbmmoodduullee..ffeettcchhJJoobbss option.

       --[no-]single-branch
           This option is only valid for the update command. Clone only one branch during update:
           HEAD or one specified by --branch.

       <path>...
           Paths to submodule(s). When specified this will restrict the command to only operate on
           the submodules found at the specified paths. (This argument is required with add).

FFIILLEESS
       When initializing submodules, a ..ggiittmmoodduulleess file in the top-level directory of the containing
       repository is used to find the url of each submodule. This file should be formatted in the
       same way as $$GGIITT__DDIIRR//ccoonnffiigg. The key to each submodule url is "submodule.$name.url". See
       ggiittmmoodduulleess(5) for details.

SSEEEE AALLSSOO
       ggiittssuubbmmoodduulleess(7), ggiittmmoodduulleess(5).

GGIITT
       Part of the ggiitt(1) suite

Git 2.37.2                                    08/11/2022                             GIT-SUBMODULE(1)"#;

    const GLAB_HELP: &str = "
GLab is an open source GitLab CLI tool bringing GitLab to your command line

USAGE
  glab <command> <subcommand> [flags]

CORE COMMANDS
  alias:       Create, list and delete aliases
  api:         Make an authenticated request to GitLab API
  auth:        Manage glab's authentication state
  check-update: Check for latest glab releases
  ci:          Work with GitLab CI pipelines and jobs
  completion:  Generate shell completion scripts
  config:      Set and get glab settings
  help:        Help about any command
  issue:       Work with GitLab issues
  label:       Manage labels on remote
  mr:          Create, view and manage merge requests
  release:     Manage GitLab releases
  repo:        Work with GitLab repositories and projects
  ssh-key:     Manage SSH keys
  user:        Interact with user
  variable:    Manage GitLab Project and Group Variables
  version:     show glab version information

FLAGS
      --help      Show help for command
  -v, --version   show glab version information

ENVIRONMENT VARIABLES
  GITLAB_TOKEN: an authentication token for API requests. Setting this avoids being
  prompted to authenticate and overrides any previously stored credentials.
  Can be set in the config with 'glab config set token xxxxxx'
  
  GITLAB_HOST or GL_HOST: specify the url of the gitlab server if self hosted (eg: https://gitlab.example.com). Default is https://gitlab.com.
  
  REMOTE_ALIAS or GIT_REMOTE_URL_VAR: git remote variable or alias that contains the gitlab url.
  Can be set in the config with 'glab config set remote_alias origin'
  
  VISUAL, EDITOR (in order of precedence): the editor tool to use for authoring text.
  Can be set in the config with 'glab config set editor vim'
  
  BROWSER: the web browser to use for opening links.
  Can be set in the config with 'glab config set browser mybrowser'
  
  GLAMOUR_STYLE: environment variable to set your desired markdown renderer style
  Available options are (dark|light|notty) or set a custom style
  https://github.com/charmbracelet/glamour#styles
  
  NO_PROMPT: set to 1 (true) or 0 (false) to disable and enable prompts respectively
  
  NO_COLOR: set to any value to avoid printing ANSI escape sequences for color output.
  
  FORCE_HYPERLINKS: set to 1 to force hyperlinks to be output, even when not outputing to a TTY
  
  GLAB_CONFIG_DIR: set to a directory path to override the global configuration location 

LEARN MORE
  Use 'glab <command> <subcommand> --help' for more information about a command.

FEEDBACK
  Encountered a bug or want to suggest a feature?
  Open an issue using 'glab issue create -R profclems/glab'";

    const SYSTEMCTL_HELP: &str = "
systemctl [OPTIONS...] COMMAND ...

Query or send control commands to the system manager.

Unit Commands:
  list-units [PATTERN...]             List units currently in memory
  list-automounts [PATTERN...]        List automount units currently in memory,
                                      ordered by path
  list-paths [PATTERN...]             List path units currently in memory,
                                      ordered by path
  list-sockets [PATTERN...]           List socket units currently in memory,
                                      ordered by address
  list-timers [PATTERN...]            List timer units currently in memory,
                                      ordered by next elapse
  is-active PATTERN...                Check whether units are active
  is-failed [PATTERN...]              Check whether units are failed or
                                      system is in degraded state
  status [PATTERN...|PID...]          Show runtime status of one or more units
  show [PATTERN...|JOB...]            Show properties of one or more
                                      units/jobs or the manager
  cat PATTERN...                      Show files and drop-ins of specified units
  help PATTERN...|PID...              Show manual for one or more units
  list-dependencies [UNIT...]         Recursively show units which are required
                                      or wanted by the units or by which those
                                      units are required or wanted
  start UNIT...                       Start (activate) one or more units
  stop UNIT...                        Stop (deactivate) one or more units
  reload UNIT...                      Reload one or more units
  restart UNIT...                     Start or restart one or more units
  try-restart UNIT...                 Restart one or more units if active
  reload-or-restart UNIT...           Reload one or more units if possible,
                                      otherwise start or restart
  try-reload-or-restart UNIT...       If active, reload one or more units,
                                      if supported, otherwise restart
  isolate UNIT                        Start one unit and stop all others
  kill UNIT...                        Send signal to processes of a unit
  clean UNIT...                       Clean runtime, cache, state, logs or
                                      configuration of unit
  freeze PATTERN...                   Freeze execution of unit processes
  thaw PATTERN...                     Resume execution of a frozen unit
  set-property UNIT PROPERTY=VALUE... Sets one or more properties of a unit
  bind UNIT PATH [PATH]               Bind-mount a path from the host into a
                                      unit's namespace
  mount-image UNIT PATH [PATH [OPTS]] Mount an image from the host into a
                                      unit's namespace
  service-log-level SERVICE [LEVEL]   Get/set logging threshold for service
  service-log-target SERVICE [TARGET] Get/set logging target for service
  reset-failed [PATTERN...]           Reset failed state for all, one, or more
                                      units
  whoami [PID...]                     Return unit caller or specified PIDs are
                                      part of

Unit File Commands:
  list-unit-files [PATTERN...]        List installed unit files
  enable [UNIT...|PATH...]            Enable one or more unit files
  disable UNIT...                     Disable one or more unit files
  reenable UNIT...                    Reenable one or more unit files
  preset UNIT...                      Enable/disable one or more unit files
                                      based on preset configuration
  preset-all                          Enable/disable all unit files based on
                                      preset configuration
  is-enabled UNIT...                  Check whether unit files are enabled
  mask UNIT...                        Mask one or more units
  unmask UNIT...                      Unmask one or more units
  link PATH...                        Link one or more units files into
                                      the search path
  revert UNIT...                      Revert one or more unit files to vendor
                                      version
  add-wants TARGET UNIT...            Add 'Wants' dependency for the target
                                      on specified one or more units
  add-requires TARGET UNIT...         Add 'Requires' dependency for the target
                                      on specified one or more units
  edit UNIT...                        Edit one or more unit files
  get-default                         Get the name of the default target
  set-default TARGET                  Set the default target

Machine Commands:
  list-machines [PATTERN...]          List local containers and host

Job Commands:
  list-jobs [PATTERN...]              List jobs
  cancel [JOB...]                     Cancel all, one, or more jobs

Environment Commands:
  show-environment                    Dump environment
  set-environment VARIABLE=VALUE...   Set one or more environment variables
  unset-environment VARIABLE...       Unset one or more environment variables
  import-environment VARIABLE...      Import all or some environment variables

Manager State Commands:
  daemon-reload                       Reload systemd manager configuration
  daemon-reexec                       Reexecute systemd manager
  log-level [LEVEL]                   Get/set logging threshold for manager
  log-target [TARGET]                 Get/set logging target for manager
  service-watchdogs [BOOL]            Get/set service watchdog state

System Commands:
  is-system-running                   Check whether system is fully running
  default                             Enter system default mode
  rescue                              Enter system rescue mode
  emergency                           Enter system emergency mode
  halt                                Shut down and halt the system
  poweroff                            Shut down and power-off the system
  reboot                              Shut down and reboot the system
  kexec                               Shut down and reboot the system with kexec
  soft-reboot                         Shut down and reboot userspace
  exit [EXIT_CODE]                    Request user instance or container exit
  switch-root [ROOT [INIT]]           Change to a different root file system
  suspend                             Suspend the system
  hibernate                           Hibernate the system
  hybrid-sleep                        Hibernate and suspend the system
  suspend-then-hibernate              Suspend the system, wake after a period of
                                      time, and hibernate
Options:
  -h --help              Show this help
     --version           Show package version
     --system            Connect to system manager
     --user              Connect to user service manager
  -H --host=[USER@]HOST  Operate on remote host
  -M --machine=CONTAINER Operate on a local container
  -t --type=TYPE         List units of a particular type
     --state=STATE       List units with particular LOAD or SUB or ACTIVE state
     --failed            Shortcut for --state=failed
  -p --property=NAME     Show only properties by this name
  -P NAME                Equivalent to --value --property=NAME
  -a --all               Show all properties/all units currently in memory,
                         including dead/empty ones. To list all units installed
                         on the system, use 'list-unit-files' instead.
  -l --full              Don't ellipsize unit names on output
  -r --recursive         Show unit list of host and local containers
     --reverse           Show reverse dependencies with 'list-dependencies'
     --with-dependencies Show unit dependencies with 'status', 'cat',
                         'list-units', and 'list-unit-files'.
     --job-mode=MODE     Specify how to deal with already queued jobs, when
                         queueing a new job
  -T --show-transaction  When enqueuing a unit job, show full transaction
     --show-types        When showing sockets, explicitly show their type
     --value             When showing properties, only print the value
     --check-inhibitors=MODE
                         Whether to check inhibitors before shutting down,
                         sleeping, or hibernating
  -i                     Shortcut for --check-inhibitors=no
     --kill-whom=WHOM    Whom to send signal to
     --kill-value=INT    Signal value to enqueue
  -s --signal=SIGNAL     Which signal to send
     --what=RESOURCES    Which types of resources to remove
     --now               Start or stop unit after enabling or disabling it
     --dry-run           Only print what would be done
                         Currently supported by verbs: halt, poweroff, reboot,
                             kexec, soft-reboot, suspend, hibernate, 
                             suspend-then-hibernate, hybrid-sleep, default,
                             rescue, emergency, and exit.
  -q --quiet             Suppress output
     --no-warn           Suppress several warnings shown by default
     --wait              For (re)start, wait until service stopped again
                         For is-system-running, wait until startup is completed
     --no-block          Do not wait until operation finished
     --no-wall           Don't send wall message before halt/power-off/reboot
     --no-reload         Don't reload daemon after en-/dis-abling unit files
     --legend=BOOL       Enable/disable the legend (column headers and hints)
     --no-pager          Do not pipe output into a pager
     --no-ask-password   Do not ask for system passwords
     --global            Edit/enable/disable/mask default user unit files
                         globally
     --runtime           Edit/enable/disable/mask unit files temporarily until
                         next reboot
  -f --force             When enabling unit files, override existing symlinks
                         When shutting down, execute action immediately
     --preset-mode=      Apply only enable, only disable, or all presets
     --root=PATH         Edit/enable/disable/mask unit files in the specified
                         root directory
     --image=PATH        Edit/enable/disable/mask unit files in the specified
                         disk image
     --image-policy=POLICY
                         Specify disk image dissection policy
  -n --lines=INTEGER     Number of journal entries to show
  -o --output=STRING     Change journal output mode (short, short-precise,
                             short-iso, short-iso-precise, short-full,
                             short-monotonic, short-unix, short-delta,
                             verbose, export, json, json-pretty, json-sse, cat)
     --firmware-setup    Tell the firmware to show the setup menu on next boot
     --boot-loader-menu=TIME
                         Boot into boot loader menu on next boot
     --boot-loader-entry=NAME
                         Boot into a specific boot loader entry on next boot
     --plain             Print unit dependencies as a list instead of a tree
     --timestamp=FORMAT  Change format of printed timestamps (pretty, unix,
                             us, utc, us+utc)
     --read-only         Create read-only bind mount
     --mkdir             Create directory before mounting, if missing
     --marked            Restart/reload previously marked units
     --drop-in=NAME      Edit unit files using the specified drop-in file name
     --when=TIME         Schedule halt/power-off/reboot/kexec action after
                         a certain timestamp

See the systemctl(1) man page for details.";

    #[allow(clippy::type_complexity)]
    fn go(conf: &ConfigFile, s: String) -> (Vec<(String, String)>, Vec<(String, String)>) {
        let (flags, subs) = extract_text(conf, s);
        let mut subs = subs
            .0
            .iter()
            .map(|(long, s)| (s.short.clone(), long.clone()))
            .collect::<Vec<_>>();
        subs.sort();
        let mut flags = flags
            .into_iter()
            .map(|(k, v)| (v.short, k.strip_prefix("--").unwrap().to_string()))
            .collect::<Vec<_>>();
        flags.sort();
        (flags, subs)
    }

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
    fn extract_cabal() {
        let conf = ConfigFile::from_file(PathBuf::from("conf/cabal.toml"));
        let (flags, subs) = go(&conf, String::from(CABAL_HELP));
        let expected = expect![[r#"
            a -> active-repositories
            c- -> config-file
            d -> disable-nix
            en -> enable-nix
            hp -> help
            ht -> http-transport
            ig -> ignore-expiry
            ni -> nix
            nu -> numeric-version
            st -> store-dir
            ve -> version
        "#]];
        expected.assert_eq(&serialize(&flags));
        let expected = expect![[r#"
            b -> build
            be -> bench
            ca -> cabal
            ch -> check
            cl -> clean
            cu -> configure
            ex -> exec
            fe -> fetch
            fr -> freeze
            gn -> gen-bounds
            gt -> get
            h- -> haddock-project
            hk -> haddock
            hp -> help
            hs -> hscolour
            if -> info
            ii -> init
            is -> install
            l -> list
            lb -> list-bin
            ne -> new-haddock-project
            o -> outdated
            r -> run
            rl -> repl
            ro -> report
            sd -> sdist
            t -> test
            ud -> update
            ul -> upload
            un -> unpack
            us -> user-config
            v- -> v2-haddock-project
            v1c -> v1-reconfigure
            v1e -> v1-bench
            v1g -> v1-register
            v1p -> v1-repl
            v1u -> v1-build
            v2-e -> v2-exec
            v2-f -> v2-freeze
            v2-i -> v2-install
            v2-re -> v2-repl
            v2-ru -> v2-run
            v2-s -> v2-sdist
            v2-t -> v2-test
            v2-u -> v2-update
            v2e -> v2-bench
            v2l -> v2-clean
            v2o -> v2-configure
            v2u -> v2-build
            vf -> v1-freeze
            vh -> v1-haddock
            vi -> v1-install
            vk -> v2-haddock
            vl -> v1-clean
            vn -> v1-configure
            vp -> v1-copy
            vt -> v1-test
            vu -> v1-run
        "#]];
        expected.assert_eq(&serialize(&subs));
    }

    #[test]
    fn extract_cargo() {
        let conf = ConfigFile::from_file(PathBuf::from("conf/cargo.toml"));
        let (flags, subs) = go(&conf, String::from(CARGO_HELP));
        let expected = expect![[r#"
            cg -> config
            col -> color
            e -> explain
            fr -> frozen
            hp -> help
            li -> list
            lk -> locked
            of -> offline
            q -> quiet
            vb -> verbose
            vern -> version
        "#]];
        expected.assert_eq(&serialize(&flags));
        let expected = expect![[r#"
            a -> add
            b -> build
            be -> bench
            bi -> bisect-rustc
            c -> check
            ca -> careful
            cg -> config
            ci -> clippy
            cl -> clean
            d -> doc
            de -> depgraph
            fe -> fetch
            fi -> fix
            fm -> fmt
            ge -> generate-lockfile
            gi -> git-checkout
            hp -> help
            i -> install
            in -> init
            la -> locate-project
            loi -> login
            loo -> logout
            me -> metadata
            mi -> miri
            n -> new
            ow -> owner
            pa -> package
            pk -> pkgid
            pu -> publish
            r -> run
            ra -> read-manifest
            rc -> rustc
            rd -> rustdoc
            rm -> remove
            rp -> report
            sa -> search
            st -> set-version
            t -> test
            tr -> tree
            ud -> update
            ug -> upgrade
            un -> uninstall
            vern -> version
            vi -> verify-project
            vn -> vendor
            y -> yank
        "#]];
        expected.assert_eq(&serialize(&subs));
    }

    #[test]
    fn extract_docker() {
        let conf = ConfigFile {
            extract_flags: true,
            extract_subs: true,
            ..ConfigFile::default()
        };
        let (flags, subs) = go(&conf, String::from(DOCKER_HELP));
        let expected = expect![[r#"
            cg -> config
            ct -> context
            de -> debug
            he -> help
            ho -> host
            l- -> log-level
            tk -> tlskey
            tla -> tlscacert
            tle -> tlscert
            ts -> tls
            tv -> tlsverify
            vn -> version
        "#]];
        expected.assert_eq(&serialize(&flags));
        let expected = expect![[r#"
            a -> attach
            bd -> build
            be -> builder
            bx -> buildx
            ca -> container
            cg -> config
            com -> commit
            cop -> compose
            cp -> cp
            cr -> create
            ct -> context
            di -> diff
            ee -> exec
            ep -> export
            ev -> events
            hi -> history
            ie -> image
            inf -> info
            ins -> inspect
            ip -> import
            is -> images
            k -> kill
            la -> load
            li -> login
            lo -> logout
            ls -> logs
            m -> manifest
            ne -> network
            no -> node
            pa -> pause
            pl -> plugin
            po -> port
            ps -> ps
            pul -> pull
            pus -> push
            ri -> rmi
            rm -> rm
            rn -> rename
            rs -> restart
            ru -> run
            sa -> save
            sc -> stack
            sea -> search
            sec -> secret
            ser -> service
            so -> stop
            sr -> start
            st -> stats
            sw -> swarm
            sy -> system
            ta -> tag
            to -> top
            tr -> trust
            un -> unpause
            up -> update
            vn -> version
            vo -> volume
            w -> wait
        "#]];
        expected.assert_eq(&serialize(&subs));
    }

    #[test]
    fn extract_git() {
        let conf = ConfigFile::from_file(PathBuf::from("conf/git.toml"));
        let (flags, subs) = go(&conf, String::from(GIT_HELP));
        let expected = expect![[r#"
            pg -> paginate
            vs -> version
        "#]];
        expected.assert_eq(&serialize(&flags));
        let expected = expect![[r#"
            a -> add
            am -> am
            an -> annotate
            ap -> apply
            arm -> archimport
            arv -> archive
            b -> branch
            bg -> bugreport
            bi -> bisect
            bl -> blame
            bn -> bundle
            c- -> cherry-pick
            ca -> cat-file
            cc -> credential-cache
            ce -> clean
            cg -> commit-graph
            cha -> check-attr
            chi -> check-ignore
            chm -> check-mailmap
            cho -> checkout-index
            chr -> check-ref-format
            ci -> citool
            cl -> clone
            cm -> commit
            cn -> credential-netrc
            co -> checkout
            col -> column
            con -> config
            cou -> count-objects
            crl -> credential
            cs -> credential-store
            ct -> commit-tree
            cve -> cvsexportcommit
            cvi -> cvsimport
            cvs -> cvsserver
            cy -> cherry
            d -> diff
            de -> describe
            df -> diff-files
            di -> diff-index
            dit -> difftool
            dt -> diff-tree
            f -> fetch
            fae -> fast-export
            fai -> fast-import
            fe -> fetch-pack
            ff -> for-each-ref
            fi -> filter-branch
            fm -> fmt-merge-msg
            fom -> format-patch
            fp -> for-each-repo
            fs -> fsck
            gc -> gc
            ge -> get-tar-commit-id
            gk -> gitk
            gr -> grep
            gu -> gui
            gw -> gitweb
            ha -> hash-object
            he -> help
            ho -> hook
            ht -> http-backend
            i -> init
            id -> index-pack
            im -> imap-send
            is -> instaweb
            it -> interpret-trailers
            lf -> lfs
            lg -> log
            lsf -> ls-files
            lsr -> ls-remote
            lst -> ls-tree
            m -> merge
            ma -> mktag
            meb -> merge-base
            mef -> merge-file
            mei -> merge-index
            meo -> merge-one-file
            met -> merge-tree
            mi -> mailinfo
            mn -> maintenance
            mr -> mktree
            ms -> mailsplit
            mt -> mergetool
            mu -> multi-pack-index
            mv -> mv
            na -> name-rev
            no -> notes
            p -> push
            p- -> prune-packed
            p4 -> p4
            pad -> pack-redundant
            paf -> pack-refs
            pe -> prune
            pl -> pull
            po -> pack-objects
            pt -> patch-id
            q -> quiltimport
            ra -> range-diff
            rb -> rebase
            re -> revert
            rea -> read-tree
            ref -> reflog
            rem -> remote
            repa -> repack
            repl -> replace
            req -> request-pull
            rer -> rerere
            res -> restore
            rl -> rev-list
            rm -> rm
            rp -> rev-parse
            rs -> reset
            s -> status
            see -> send-email
            sep -> send-pack
            sh -> stash
            showb -> show-branch
            showi -> show-index
            showr -> show-ref
            shr -> shortlog
            shw -> show
            si -> sh-i18n
            sp -> sparse-checkout
            ss -> sh-setup
            st -> stripspace
            su -> submodule
            sv -> svn
            sw -> switch
            sy -> symbolic-ref
            t -> tag
            uf -> unpack-file
            ui -> update-index
            uo -> unpack-objects
            ur -> update-ref
            us -> update-server-info
            va -> var
            vc -> verify-commit
            vp -> verify-pack
            vt -> verify-tag
            wh -> whatchanged
            wo -> worktree
            wr -> write-tree
        "#]];
        expected.assert_eq(&serialize(&subs));
    }

    #[test]
    fn extract_git_submodule() {
        let conf = ConfigFile {
            extract_flags: true,
            extract_subs: true,
            ..ConfigFile::default()
        };
        let (flags, subs) = go(&conf, String::from(GIT_SUBMODULE_HELP));
        let expected = expect![[r#"
            al -> all
            bh -> branch
            ca -> cached
            ch -> checkout
            di -> dissociate
            dp -> depth
            fc -> force
            fi -> files
            ii -> init
            j -> jobs
            me -> merge
            na -> name
            no -> no-fetch
            pr -> progress
            q -> quiet
            rb -> rebase
            rf -> reference
            rm -> remote
            ru -> recursive
            sum -> summary-limit
        "#]];
        expected.assert_eq(&serialize(&flags));
        // TODO: These are all wrong
        // TODO: cchheecckkoouutt ???
        let expected = expect![[r#"
            ab -> absolute
            bh -> branch
            cc -> cchheecckkoouutt
            co -> command
            cu -> current
            df -> default
            dt -> detached
            fr -> for
            g -> git-submodule
            is -> instead
            ma -> many
            pe -> performed
            ro -> recorded
            rs -> resolution
            sp -> specified
            sub -> submodules
            sup -> superproject
            t -> the
            w -> when
        "#]];
        expected.assert_eq(&serialize(&subs));
    }

    #[test]
    fn extract_glab() {
        let conf = ConfigFile {
            extract_flags: true,
            extract_subs: true,
            ..ConfigFile::default()
        };
        let (flags, subs) = go(&conf, String::from(GLAB_HELP));
        let expected = expect![[r#"
            hp -> help
            ve -> version
        "#]];
        expected.assert_eq(&serialize(&flags));
        let expected = expect![[r#"
            al -> alias
            ap -> api
            au -> auth
            ch -> check-update
            ci -> ci
            cm -> completion
            cn -> config
            hp -> help
            i -> issue
            l -> label
            m -> mr
            rl -> release
            rp -> repo
            s -> ssh-key
            u -> user
            va -> variable
        "#]];
        expected.assert_eq(&serialize(&subs));
    }

    #[test]
    fn extract_systemctl() {
        let conf = ConfigFile::from_file(PathBuf::from("conf/systemctl.toml"));
        let (flags, subs) = go(&conf, String::from(SYSTEMCTL_HELP));
        let expected = expect![[r#"
            al -> all
            be -> boot-loader-entry
            bm -> boot-loader-menu
            ch -> check-inhibitors
            do -> drop-in
            dy -> dry-run
            fa -> failed
            fi -> firmware-setup
            fo -> force
            fu -> full
            gl -> global
            ho -> host
            hp -> help
            i- -> image-policy
            ie -> image
            j -> job-mode
            kv -> kill-value
            kw -> kill-whom
            le -> legend
            lie -> lines
            mc -> machine
            mk -> mkdir
            mr -> marked
            na -> no-ask-password
            nb -> no-block
            nol -> no-wall
            nor -> no-warn
            np -> no-pager
            nr -> no-reload
            nw -> now
            o -> output
            pl -> plain
            pm -> preset-mode
            pro -> property
            q -> quiet
            ra -> read-only
            rc -> recursive
            ro -> root
            rs -> reverse
            ru -> runtime
            shr -> show-transaction
            shy -> show-types
            si -> signal
            stt -> state
            sy -> system
            ti -> timestamp
            ty -> type
            u -> user
            va -> value
            ve -> version
            wa -> wait
            wha -> what
            whe -> when
            wi -> with-dependencies
        "#]];
        expected.assert_eq(&serialize(&flags));
        let expected = expect![[r#"
            ar -> add-requires
            aw -> add-wants
            bi -> bind
            cl -> clean
            cn -> cancel
            ct -> cat
            dae -> daemon-reexec
            dal -> daemon-reload
            de -> default
            di -> disable
            ed -> edit
            em -> emergency
            en -> enable
            ex -> exit
            fr -> freeze
            ge -> get-default
            ha -> halt
            hi -> hibernate
            hp -> help
            hy -> hybrid-sleep
            io -> isolate
            ip -> import-environment
            isa -> is-active
            ise -> is-enabled
            isf -> is-failed
            iss -> is-system-running
            ke -> kexec
            kl -> kill
            lgl -> log-level
            lgt -> log-target
            lik -> link
            lsa -> list-automounts
            lsd -> list-dependencies
            lsj -> list-jobs
            lsm -> list-machines
            lsp -> list-paths
            lss -> list-sockets
            lst -> list-timers
            lsu -> list-units
            lsuf -> list-unit-files
            mo -> mount-image
            ms -> mask
            pa -> preset-all
            po -> poweroff
            pt -> preset
            r- -> reload-or-restart
            rb -> reboot
            rd -> reload
            re -> reenable
            resc -> rescue
            rese -> reset-failed
            rest -> restart
            rt -> revert
            s -> status
            s- -> suspend-then-hibernate
            sd -> suspend
            se -> show-environment
            sed -> set-default
            see -> set-environment
            sep -> set-property
            shw -> show
            sl -> service-log-level
            so -> soft-reboot
            sp -> stop
            st -> service-log-target
            str -> start
            sw -> service-watchdogs
            th -> thaw
            tl -> try-reload-or-restart
            ts -> try-restart
            unm -> unmask
            uns -> unset-environment
            who -> whoami
        "#]];
        expected.assert_eq(&serialize(&subs));
    }

    #[test]
    fn test_deconflict() {
        let conf = ConfigFile::default();
        assert_eq!(deconflict(&conf, &[], &[], &[]), HashMap::new());
        assert_eq!(
            deconflict(&conf, &[String::from("foo")], &[], &[]),
            HashMap::from([(String::from("foo"), String::from("f"))])
        );
        assert_eq!(
            deconflict(&conf, &[String::from("bar"), String::from("baz")], &[], &[]),
            HashMap::from([
                (String::from("bar"), String::from("br")),
                (String::from("baz"), String::from("bz"))
            ])
        );
        let conf = ConfigFile::from_file(PathBuf::from("conf/git.toml"));
        assert_eq!(
            deconflict(
                &conf,
                &[String::from("show"), String::from("status")],
                &[],
                &[]
            ),
            HashMap::from([
                (String::from("status"), String::from("st")),
                (String::from("show"), String::from("sh"))
            ])
        );
    }
}
