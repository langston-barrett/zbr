# Installation

## Pre-compiled binaries

Pre-compiled binaries are available on the [releases page][releases].

### Fetching binaries with cURL

You can download binaries with `curl` like so (replace `X.Y.Z` with a real
version number and `TARGET` with your OS):
```sh
curl -sSL https://github.com/langston-barrett/zbr/releases/download/vX.Y.Z/zbr_TARGET -o zbr
```

[releases]: https://github.com/langston-barrett/zbr/releases

## Build from source

To install from source, you'll need to install Rust and [Cargo][cargo]. Follow
the instructions on the [Rust installation page][install-rust].

[install-rust]: https://www.rust-lang.org/tools/install

### From the latest unreleased version on Github

To build and install the very latest unreleased version, run:

```sh
cargo install --git https://github.com/langston-barrett/zbr.git zbr
```

### From a local checkout

See the [developer's guide](dev.md).

[cargo]: https://doc.rust-lang.org/cargo/
