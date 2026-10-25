# Development Setup

**Requirements:** Rust 1.85+ (stable). Install via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/ashlar-display/concordance.git
cd concordance
cargo build
```

## Running tests

```sh
cargo test                                           # std (default)
cargo test --features serde                          # std + serde
cargo build --no-default-features --features alloc  # alloc-only build check
cargo build --no-default-features                   # bare no_std build check
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
cargo llvm-cov --no-report
cargo llvm-cov --no-report --features serde
cargo llvm-cov report --summary-only
```

To update `.coverage-baseline` after improving coverage:

```sh
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report
cargo llvm-cov --no-report --features serde
cargo llvm-cov report --json --summary-only \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print(round(d['data'][0]['totals']['lines']['percent'], 2))" \
  > .coverage-baseline
```

Commit `.coverage-baseline` alongside the tests that raised it. CI will open a PR
automatically when coverage improves on a push to `main` or `develop`, but you can also
update it manually as part of your feature branch.
