# zbr

<!-- Note: The README is duplicated in doc/overview.md -->

zbr is a tool for managing auto-expanding abbreviations for ZSH. With the
default configuration, typing `gco`<kbd>Space</kbd> into ZSH results in `git
checkout `; the abbreviation is expanded in place. Don't want an abbreviation
to expand? Just use <kbd>Ctrl</kbd><kbd>Space</kbd> instead.

Quoting from the README of [zsh-abbr][zsh-abbr]:

> Why? Like aliases, abbreviations **save keystrokes**. Unlike aliases, abbreviations can leave you with a **transparently understandable command history** ready for using on a different computer or sharing with a colleague. And where aliases can let you forget the full command, abbreviations may **help you learn** the full command even as you type the shortened version.

## Features

### Extraction

zbr supports *extracting* abbreviations from command-line tools. Running

```sh
zbr extract cargo conf/cargo.toml
```

generates a configuration file that contains all of the following abbreviations
and more!

```
cg --> cargo
cg -e --> cargo --explain
cg -f --> cargo --frozen
cg -h --> cargo --help
...
cga --> cargo add
cgb --> cargo build
cgbe --> cargo bench
cgc --> cargo check
...
cargo -e --> cargo --explain
cargo -f --> cargo --frozen
cargo -h --> cargo --help
...
cargo a --> cargo add
cargo b --> cargo build
cargo be --> cargo bench
cargo c --> cargo check
...
```

### Discoverability

zbr will also *show you abbreviations as you type*, typing `git s` will display
applicable abbreviations just below the command prompt:

```
git s --> git status
git see --> git send-email
git sep --> git send-pack
...
```

### Smart abbreviations

zbr detects the build system of the project you're working on, and creates
the abbreviations `b` for "build", `r` for "run", and `t` for "test". For
example, when working in a directory with a `Cargo.toml`, zbr will use the
abbreviations

```
b --> cargo build
r --> cargo run
t --> cargo test
```

but when working on a Haskell project, `cargo` would be replace by `cabal`.

### Unique prefixes for subcommands

In addition to pithy abbreviations like `gsu --> git submodule`, zbr
abbreviates all uniquely identifying prefixes of subcommands. For example, it
will automatically abbreviate

```
git sub --> git submodule
git subm --> git submodule
git submo --> git submodule
```

and so on, meaning you can just press <kbd>Space</kbd> at any point to expand
to the full subcommand name.

## Documentation

Documentation is available online at <https://langston-barrett.github.io/zbr/>,
or locally in [`doc/`](./doc).

## Related tools

- [zsh-abbr][zsh-abbr] is quite similar, but doesn't support extracting
  abbreviations nor contextual abbreviations.
- [zabrze][zabrze] is quite similar, but doesn't support extracting
  abbreviations

[zabrze]: https://github.com/Ryooooooga/zabrze
[zsh-abbr]: https://github.com/olets/zsh-abbr
