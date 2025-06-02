# Developer's guide

## Build

To install from source, you'll need to install Rust and [Cargo][cargo]. Follow
the instructions on the [Rust installation page][install-rust]. Then, get
the source:

```bash
git clone https://github.com/langston-barrett/zbr
cd zbr
```

Finally, build everything:

```bash
cargo build --release
```

You can find binaries in `target/release`. Run tests with `cargo test`.

[cargo]: https://doc.rust-lang.org/cargo/
[install-rust]: https://www.rust-lang.org/tools/install

<!--
### PGO

Build the instrumented binary:

```sh
cargo install cargo-pgo
rustup component add llvm-tools-preview
cargo pgo build
```

Collect data:

```sh
target/x86_64-unknown-linux-gnu/release/zbr TODO
```

Finally, build and install the optimized binary:

```sh
cargo pgo optimize
mv target/x86_64-unknown-linux-gnu/release/zbr /wherever
```
-->

## Docs

HTML documentation can be built with [mdBook][mdbook]:

```sh
cd doc
mdbook build
```

[mdbook]: https://rust-lang.github.io/mdBook/

## Format

All code should be formatted with [rustfmt][rustfmt]. You can install rustfmt
with [rustup][rustup] like so:

```sh
rustup component add rustfmt
```

and then run it like this:

```sh
cargo fmt
```

[rustfmt]: https://rust-lang.github.io/rustfmt
[rustup]: https://rustup.rs/

## Lint

All code should pass [Clippy][clippy]. You can install Clippy with rustup
like so:

```sh
rustup component add clippy
```

and then run it like this:

```sh
cargo clippy --workspace -- --deny warnings
```

[clippy]: https://doc.rust-lang.org/stable/clippy/

## Profile

TODO

<!--
### dhat

```sh
cargo build -q --release --features dhat-heap
./target/release/zbr --quiet --signatures signatures.json jackson.bc
```

Then, go to <https://nnethercote.github.io/dh_view/dh_view.html> to upload
`dhat-heap.json`.

### perf

```sh
cargo build -q --release
perf record ./target/release/zbr --signatures signatures.json jackson.bc
```

### Poor Man's Profiler

```sh
cargo build -q --release
./target/release/zbr --signatures signatures.json jackson.bc &
./scripts/poor-mans-profiler.sh 10 > prof.txt
```

### Samply

```sh
cargo install samply
cargo build --profile=profiling
samply record ./target/release/zbr --signatures signatures.json jackson.bc
```
-->

## Warnings

Certain warnings are disallowed in the CI build. To allow a lint in one spot,
use:

```rust
#[allow(name_of_lint)]
```
