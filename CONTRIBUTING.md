# Contributing to concordance

Thanks for your interest in contributing. This document covers the basics.

## Getting started

Relevant docs for contributors:

- [`doc/setup.md`](doc/setup.md) — build, test, and coverage commands
- [`doc/testing.md`](doc/testing.md) — testing strategy, fixture corpus, and CI expectations
- [`doc/architecture.md`](doc/architecture.md) — pipeline structure, constraint rules, and design principles
- [`doc/enumerator.md`](doc/enumerator.md) — candidate enumeration and iteration order
- [`doc/roadmap.md`](doc/roadmap.md) — planned features and known gaps

## Issues and pull requests

**Open an issue first** if you're unsure whether something is a bug or if you want to
discuss a change before implementing it. For small, self-contained fixes a PR on its own
is fine.

- Bug reports: describe the capability triple that produces the wrong result. If you can
  attach a serialized `SinkCapabilities` / `SourceCapabilities` / `CableCapabilities`
  (via the `serde` feature), that's ideal.
- Feature requests: a brief description of what you need and why is enough to start a
  conversation.
- PRs: keep them focused. One logical change per PR makes review faster and keeps
  history readable.

## Coding standards

- Run `cargo fmt` and `cargo clippy -- -D warnings` before pushing.
- Public items need rustdoc comments (`cargo rustdoc -- -D missing_docs` must pass).
- `#![forbid(unsafe_code)]` is enforced; no unsafe code.
- Keep `no_std` compatibility. `is_config_viable` and all constraint rules must compile
  without `alloc` or `std`. The ranked pipeline is gated behind
  `#[cfg(any(feature = "alloc", feature = "std"))]`.
- All public structs and enums must be `#[non_exhaustive]`.

## Commit and PR expectations

- Write commit messages in the imperative mood ("Add support for …", not "Added …").
- Keep commits logically atomic. A PR that touches three unrelated things should be
  three commits (or three PRs).
- Tests are expected for new constraint logic. A unit test with handcrafted capability
  structs calling `check` directly is usually sufficient; an `is_config_viable`
  integration test is a bonus.
- CI must be green before a PR can merge: fmt, clippy, docs, all test and build
  targets, and coverage must not drop more than 0.1% below the baseline (stored in
  `.coverage-baseline`). New constraint logic without tests will likely trip this.

## Review process

PRs are reviewed on a best-effort basis. Expect feedback within a few days; if you
haven't heard back in a week feel free to ping the thread. Reviews aim to be
constructive — if something needs to change, the reviewer will explain why. Approval
from the maintainer is required to merge.

## Code of Conduct

This project follows the [Contributor Covenant 3.0](https://github.com/ashlar-display/.github/blob/HEAD/CODE_OF_CONDUCT.md),
the code of conduct of every [Ashlar](https://ashlar-display.org) repository. Please read it
before participating.
