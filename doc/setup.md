# Development Setup

**Requirements:** Rust 1.85+ (stable). Install via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/ashlar-display/concordance.git
cd concordance
cargo build
```

## Running checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo clippy --all-targets --no-default-features --features alloc -- -D warnings
cargo clippy --all-targets --features serde -- -D warnings
cargo rustdoc --all-features -- -D warnings
```

## Running tests

```sh
cargo test                                         # std (default)
cargo test --features serde                        # std + serde
cargo test --no-default-features --features alloc  # alloc only
cargo test --no-default-features                   # bare no_std
```

The README's examples and the crate documentation's run as doctests with the others. See
[testing.md](testing.md) for what the tests cover.

## `no_std` builds

Without `std` the crate is `no_std`, with or without `alloc`. CI checks both on a bare
target, which has no `std` for a dependency to pull in:

```sh
rustup target add thumbv7em-none-eabi
cargo build --no-default-features --features alloc --target thumbv7em-none-eabi
cargo build --no-default-features --target thumbv7em-none-eabi
```

## Checking the MSRV

The MSRV is `rust-version` in `Cargo.toml`. CI checks it with
[`cargo-msrv`](https://github.com/foresterre/cargo-msrv), which runs the checks on that
toolchain:

```sh
cargo install cargo-msrv --locked
cargo msrv verify -- cargo test --locked
cargo msrv verify -- cargo test --locked --features serde
cargo msrv verify -- cargo test --locked --no-default-features --features alloc
cargo msrv verify -- cargo test --locked --no-default-features
cargo msrv verify --target thumbv7em-none-eabi \
  -- cargo build --locked --no-default-features --features alloc --target thumbv7em-none-eabi
cargo msrv verify --target thumbv7em-none-eabi \
  -- cargo build --locked --no-default-features --target thumbv7em-none-eabi
```

## The negotiate example

`examples/negotiate` reads the displays in `/sys/class/drm` (Linux), so CI builds it
without running it:

```sh
cargo run --locked --manifest-path examples/negotiate/Cargo.toml
```

It is its own workspace with a tracked `Cargo.lock`, which records concordance's version
and dependencies through the path dependency. After changing `Cargo.toml`, refresh it and
commit it, or the `--locked` build (here and in CI) fails:

```sh
cargo check --manifest-path examples/negotiate/Cargo.toml
```

## Coverage

Install [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) and the LLVM
tools component:

```sh
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
```

Measure coverage across all feature sets:

```sh
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report --locked
cargo llvm-cov --no-report --locked --features serde
cargo llvm-cov report --summary-only
```

To update `.coverage-baseline` after improving coverage:

```sh
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report --locked
cargo llvm-cov --no-report --locked --features serde
cargo llvm-cov report --json --summary-only \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(round(d['data'][0]['totals']['lines']['percent'], 2))" \
  > .coverage-baseline
```

Commit `.coverage-baseline` alongside the tests that raised it. CI will open a PR
automatically when coverage improves on a push to `main` or `develop`, but you can also
update it manually as part of your feature branch.
