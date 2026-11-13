# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **docs.rs marks the items that need a feature** (`alloc`, `std`, `serde`) with the
  `doc_cfg` badge, enabled only in docs.rs builds.

### Changed

- **The README's Ashlar block links the stack's crate list** instead of naming every crate,
  so it no longer goes stale on crates.io when a crate is added.

### Fixed

- **The custom rule example in the crate documentation compiles without `alloc`**: it uses
  `NegotiatorBuilder`, which needs `alloc`, so `cargo test --no-default-features` failed on
  it; it is now compiled only where that type exists.
- **The README's examples compile against the current API.** The ranked pipeline example
  printed `mode.refresh_rate` with `{}`, but it is an `Option<RefreshRate>`.

### Internal

- **The README's Rust examples run as doctests** (`ReadmeDoctests` in `lib.rs`, compiled
  only for `cargo test`), on default capabilities; the examples that need `alloc` run only
  with it. That is how the error above was found.
- **The tests pass `clippy --all-targets`**: struct update syntax instead of assigning fields
  after `Default::default()`, `slice::from_ref` instead of a cloned one-element slice, and
  `#[expect]` with a reason where a lint does not fit (the test of the hand-written
  `Clone`, a ranking-test helper with one argument per field).
- **Tests for the paths Rust 1.100 counts as uncovered**: a cable TMDS limit of 0 is no
  limit (as for the source); under the performance and power saving policies, configs
  equal on every quality criterion fall through to the progressive-before-interlaced
  tiebreak; a warning other than an FRL capacity warning does not deprioritize a config.
  Rust 1.100 counts the same code's lines differently from 1.99 (1592 lines instead of
  1624, measured from a clean `target/`), which put line coverage below the baseline
  (98.87% against 99.08%) without a code change; with these it is 99.12%.
- **The published `.crate` holds only what the crate needs**: `include` in `Cargo.toml`
  limits it to the sources, `doc/`, the README, changelog and license; it also shipped the
  CI workflows, Dependabot configuration, coverage baseline, `.gitignore`, `CONTRIBUTING.md` and the tests.
- **The documentation is built as docs.rs builds it, weekly**: `docs.yml` runs
  `cargo doc --all-features` with nightly and `--cfg docsrs` (and `-D warnings`), so a
  nightly change to the `doc_cfg` feature badges shows up before it breaks a docs.rs build.
- **CI runs with a read-only token by default**: `ci.yml` sets `permissions: contents: read`;
  the coverage job keeps the write permissions it needs to open its ratchet PR. The test job
  ran with the repository's default token permissions.
- **CI's test job does not keep the checkout's git credentials** (`persist-credentials:
  false`); it never pushes. The coverage job keeps them to push its ratchet branch.
- **CI fails on `main` when `Cargo.toml` has a `[patch]` section**: `cargo package` drops
  it, so a release built with one would not build from crates.io.
- **Clippy lints the tests too, in every feature set**: `--all-targets`, once each with the
  default features, none, `alloc` only and `serde`, in CI and in the publish workflow's
  `verify` job; it linted only the library with the default features.
- **rustdoc runs with every feature and `-D warnings`**, as docs.rs builds, in CI and in
  the publish workflow's `verify` job: `-D missing_docs` alone let broken intra-doc links
  through, and the `serde` items were not checked.
- **The tests run in every feature set**: also with `alloc` only and with no default
  features, in CI and in the publish workflow's `verify` job; those builds were only
  compiled, which is how the custom rule example's failure went unnoticed.
- **The `no_std` builds target a bare-metal target** (`thumbv7em-none-eabi`), in CI and in
  the publish workflow's `verify` job; on the host, `std` was still there for a dependency
  that pulls it in, so the builds did not show that the crate needs none.
- **The MSRV is checked**: an `MSRV` job in CI and in the publish workflow (which waits for
  it) runs the tests in every feature set and the bare-metal builds on the `rust-version`
  from `Cargo.toml` (1.85) with `cargo msrv verify`; the crate declared it, and nothing
  checked it.
- **`doc/setup.md` lists the checks CI runs**: clippy and the tests in every feature set,
  rustdoc with every feature, the bare-metal `no_std` builds and the MSRV check.
- **`doc/testing.md` describes the feature sets** CI runs the tests in, and how the
  README's and the crate documentation's examples are compiled as doctests.
- **`CONTRIBUTING.md` matches the CI checks**: clippy over all targets, rustdoc with
  `-D warnings`, every feature set, the bare-metal builds and the MSRV.
- **The coverage job's cache is keyed on the Rust version** as well as `Cargo.lock`, so a
  `target/` built by an older Rust is not restored: `cargo llvm-cov clean --workspace`
  leaves its instrumented binaries behind, and the report would count their lines as
  uncovered (it did so in other Ashlar crates after the update to Rust 1.100).
- **The publish workflow's `verify` job fails on a `[patch]` section** too, as `ci.yml`
  does on `main`: the release does not wait for `main`'s CI.
- **`CONTRIBUTING.md` states the provenance rule** for claims about HDMI behaviour (two
  independent public implementations, cited), and gives the coverage tolerance as 0.1
  percentage points, as CI checks it, not 0.1%.

## [0.3.0] - 2026-10-30

### Breaking changes

- **Requires display-types 0.5.** concordance's public types (`VideoMode` and the colour
  and audio types in `SinkCapabilities`, `CandidateConfig` and `NegotiatedConfig`) are
  display-types', so callers move to 0.5 with it. The FRL capacity check reads
  `VideoMode::h_blank`, and `sink_capabilities_from_display` reads 0.5's
  `HdmiAudioBlock` (stream count and 3D audio).
- **Constraint check results have private storage and the same type in every build.**
  `CheckResult<W, V>` is now `Result<Warnings<W>, Violations<V>>` (it was
  `Result<Vec<W>, Vec<V>>` with alloc and `Result<[Option<W>; MAX_WARNINGS], V>`
  without). `Warnings` and `Violations` offer `iter`, `len`, `push` and `dropped` in every
  build, `Violations` also `first`, `is_complete` and `stopped_early`, and with alloc
  `into_vec`. Without alloc, warnings beyond `MAX_WARNINGS` are counted in
  `Warnings::dropped` instead of lost, and the default engine marks a rejection
  `stopped_early` (it stops at the first violation). Custom engines construct their
  results with `Warnings::new`/`Violations::new`. The no-alloc result `is_config_viable`
  returns by value is 144 bytes (8 warnings of 16 bytes and the counters).
- **`ConstraintRule` takes a warning type and reports warnings:
  `ConstraintRule<V, W = Warning>` with a required `warnings` method** (an empty body for
  a rule with nothing to report). Every rule implementation must add it; a rule that wraps
  another must forward it, which the requirement makes impossible to forget.
  `TaggingAdapter` and both `Layered` forms forward warnings.
- **`Layered<E, R>` requires `R: ConstraintRule<E::Violation, E::Warning>`**, and
  `with_extra_rule` requires its rule to be one for the engine's warning type (`W =
  E::Warning`; the built-in `Warning` for the default engine). Generic code bounding
  `R: ConstraintRule<E::Violation>` must add `E::Warning`. A rule for the built-in
  `Warning` no longer combines with an engine that has its own warning type: implement it
  for that type.
- **The default engine rejects FRL rates too slow for the timing** (`FrlCapacityCheck`,
  last in `DEFAULT_CHECKS`). Configurations accepted before can now be rejected.

### Added

- **`frl` module: FRL capacity.** `frl_capacity` decides whether an HDMI 2.1 FRL rate can
  carry an uncompressed timing: audio in the blanking, borrowing from the blanking (at most
  400 tribytes), and the line's characters with the link overhead. It follows the public
  implementations (amdgpu's `dml1_frl_cap_chk`, i915's FRL DFM). Where they disagree and neither is visibly wrong, the point is an
  `Assumptions` flag; every combination is evaluated, and the `FrlCapacityReport` gives the
  strict and permissive evaluations, a `Fits`/`Marginal`/`Exceeded` verdict and the
  deciding assumptions. Exact integer arithmetic, `no_std`, no allocation.
- **Audio for the FRL capacity check.** `FrlAudio` borrows the audio loads to count (none,
  a bound such as the sink's, or what the source may send); `AudioLoad::from_descriptor`
  maps a Short Audio Descriptor to the heaviest load a source may send for it (IEC 61937
  rates: E-AC-3 at four times the declared rate, as High Bit Rate audio above 192 kHz;
  TrueHD and DTS-HD as High Bit Rate audio whatever the declared rate). One Bit Audio,
  DST, extended and reserved formats have no corroborated packet model and are
  `UnmodelledAudio`, as are HDMI Multi-Stream Audio and HDMI 3D Audio. `SinkCapabilities`
  gains `audio_loads: Option<Vec<AudioLoad>>` (`None` = unknown, the default and what
  deserializing older data gives; `sink_capabilities_from_display` fills it from the CTA
  Audio Data Block and the basic audio flag), `unmodelled_audio`, and `frl_audio()`
  (alloc only); `SinkBuildWarning` gains `AudioDescriptorWithoutSampleRate`. Unknown audio
  (`None`, or a bare `no_std` build) is reported as `FrlUnverified::AudioUnknown`, never
  counted as none; `frl_capacity` takes a caller's own loads directly.
- **`FrlCapacityCheck`** in the default rules: rejects with
  `Violation::FrlCapacityExceeded` only when the timing fits under no reading; accepts
  with `Warning::FrlCapacityMarginal` (naming the deciding assumptions) when it fits under
  some, and with `Warning::FrlCapacityUnverified` when the capacity cannot be checked, or
  not fully (no blanking or pixel clock, DSC, an unmodelled audio format, or no audio data
  without alloc), each with its `FrlUnverified` reason. The
  violation and the marginal warning carry the shortfall (and the deciding assumptions);
  `FrlCapacityCheck::report` gives the full report for a candidate.
- **A settled DTD border reading is not an assumption.** `FrlCapacityInput` gains
  `dtd_border_reading` (with `with_dtd_border_reading`), which `from_mode` takes from
  `VideoMode::dtd_border_reading` (display-types 0.5; piaf sets it when its caller fixes
  the reading with `DtdBorderPolicy::Fixed`). When it is `Some`, `h_blank` already follows
  that reading, so `DTD_BORDER_READING` does not apply and the borders are not counted a
  second time; the verdict rests on the one reading.
- **`NegotiationPolicy::frl_marginal` and `frl_unverified`** (`FrlCandidateHandling::Keep`
  or `Deprioritize`): the default ranker puts deprioritized FRL candidates after every
  configuration without such a warning (right after the DSC penalty) and records it in
  their trace. All presets deprioritize marginal candidates and keep unverified ones.
  There is no option to drop them: accepted candidates are never discarded silently;
  filter on the warnings instead.

### Changed

- **Part of Ashlar** — the README now opens with a block naming the
  [Ashlar](https://ashlar-display.org) stack and linking the other crates in it. Keywords
  are aligned across the stack, with `ashlar` and `hdmi` on every crate; `ashlar` is added,
  `no_std` is now spelled `no-std`, `embedded` is dropped in favour of more specific
  keywords. Categories: `parser-implementations` is replaced by `no-std` and
  `no-std::no-alloc` (concordance negotiates; it does not parse). The crate's `homepage` is
  now the Ashlar website.

- `doc/roadmap.md` drops its "Shipped" section, a stale copy of what this changelog
  records; the roadmap lists only what is planned.
- **Moved to the `ashlar-display` organization** — the repository is now
  `ashlar-display/concordance` (the old URLs redirect), and `repository` points there.
  Releases are attested and published from the new repository; 0.2.0, attested before the
  move, verifies with `--owner DracoWhitefire --signer-repo DracoWhitefire/concordance`
  (see "Verifying releases" in the README). The code of conduct is now the organization's,
  shared by every Ashlar repository; the repository's own copy is removed.

## [0.2.0] - 2026-10-18

### Added

- **SLSA Build Level 2 provenance** — release artifacts are attested via
  `actions/attest-build-provenance` and verified with
  `gh attestation verify <file> --repo DracoWhitefire/concordance`.

### Changed

- **`display-types` updated to 0.4** — tracks DisplayID 2.x support added in `piaf` 0.4.1.

### Fixed

- **TMDS clock check overflow** — `TmdsClockCheck` multiplied the pixel clock by the
  deep-color factor in `u32`. A sink-supplied pixel clock above ~537 GHz (reachable from
  EDID via the clock estimate for a large CTA Type X timing) panicked with overflow checks enabled and wrapped
  to a small value otherwise, letting an impossible TMDS mode pass. The rate is now
  computed in `u64`.

### Internal

- Fixed coverage ratchet CI: added `LC_NUMERIC=C` to the baseline `printf` to prevent
  locale-dependent decimal separators from corrupting `.coverage-baseline` on non-C locales.
- **The `negotiate` example builds again, and CI builds it** — it took piaf from a path
  outside the repository and display-types 0.3, and its lock file was stale; it now uses
  piaf 0.4.1 from crates.io and display-types 0.4, prints a mode's refresh rate as "-" when
  there is none (display-types 0.4 made it optional), and CI and the publish workflow
  build it with `--locked`.
- **Publishing uses crates.io trusted publishing** — `publish.yml` obtains a short-lived
  token through OIDC (`rust-lang/crates-io-auth-action`, revoked at the end of the job, and
  short-lived regardless) instead of the long-lived `CARGO_REGISTRY_TOKEN` secret. The
  upload job runs in the `release` environment, which only `v*` tags can deploy to and
  which requires a maintainer's approval. Quality gates moved to a separate `verify` job,
  so no dependency build scripts run and no cache is restored where publishing credentials
  exist. The attested `.crate` is kept as a separate copy; after upload the job fails if
  crates.io's checksum differs from it, and that same copy is attached to the GitHub
  release. Re-running a partially failed publish reuses an existing GitHub release.
  `publish.yml` refuses tags that do not match the `Cargo.toml` version, and gains a
  `workflow_dispatch` trigger for `release-tag` (tags pushed with `GITHUB_TOKEN` start no
  push-triggered workflows); dispatches against a non-tag ref are skipped.
- **Release summary before approval** — `publish.yml` gains a `summary` job that shows the
  crate, version and commit, changes since the previous published release (ignoring
  pre-release or malformed tags, and flagging a baseline that is not on crates.io), changes
  to workflow, toolchain and build configuration, manifest changes, the changelog section,
  and the `.crate` sha256 with a command to reproduce it, on the run page while the publish
  job waits for approval. The publish job refuses to attest a package whose sha256 differs
  from the reviewed one.
- **Release workflows** — `release-prep` picks the version bump with `cargo-semver-checks`
  (`auto`, or checks an explicit bump), stamps the changelog, refreshes `Cargo.lock` and
  `examples/negotiate/Cargo.lock` and opens a `Release vX.Y.Z` PR. `release-tag` checks that
  CI passed on `develop` (and says so when the run has not finished yet), promotes
  `develop` to `main` through a `release-promote/v*` branch and PR, fails if that merge
  fails, tags only when `main` holds exactly the promoted tree, and dispatches
  `publish.yml` for the tag; it also accepts a squash-merged release commit
  (`Release vX.Y.Z (#N)`). Both fail when started from a branch other than `develop`.
  Releases were previously tagged by hand. The version bumped on `develop` ahead of a
  release (0.2.0) is back at the published 0.1.0, so release-prep stamps it.
- **No CI run on promotion PRs** — CI no longer runs on pull requests into `main`. The only
  ones are `release-tag`'s `release-promote/*` PRs, whose content already passed CI on
  `develop`; opened by `github-actions[bot]`, their runs would be held for approval and
  listed as `action_required` or failed.
- **Supply-chain hardening for CI** — every third-party action is pinned to a commit SHA
  of a release tag, with `dependabot.yml` keeping the pins and both `Cargo.lock`s (the
  crate's and the `negotiate` example's) current (lockfile-only: version requirements stay
  a deliberate choice). The Rust toolchain itself still follows `stable`.
  `cargo-semver-checks` and `cargo-llvm-cov` are installed as checksum-verified prebuilt
  binaries by the pinned `install-action`. The security audit also runs weekly, with
  read-only repository permissions.
- **README documents release provenance** — a badge shows whether crates.io requires trusted
  publishing for this crate, and "Verifying releases" now states that 0.1.0 was uploaded
  with an API token, without provenance or a GitHub release (it claimed every release was
  attested), that later releases are attested and use trusted publishing, and how to look
  up a version's publishing details on crates.io.

## [0.1.0]

Initial release.

### Added

- Three-stage negotiation pipeline: enumerate candidates, check constraints, rank results
- `is_config_viable` — no-alloc constraint probe for firmware and embedded targets
- `NegotiatorBuilder` — ranked pipeline for `alloc`/`std` targets
- `DefaultConstraintEngine` — HDMI 2.1 constraint checks
- `DefaultEnumerator` and `SliceEnumerator` — Cartesian product candidate generation
- `DefaultRanker` with `NegotiationPolicy` presets (`BestQuality`, `BestPerformance`,
  `PowerSaving`)
- `ReasoningTrace` — per-config audit log of constraint decisions and ranking criteria
- `ConstraintRule` trait and `Layered` combinator for additive rule injection
- `sink_capabilities_from_display` — bridge from `DisplayCapabilities` (piaf) to
  `SinkCapabilities`
- `CableCapabilities::unconstrained()` — optimistic fallback for callers without cable
  information
- `serde` feature flag: `Serialize`/`Deserialize` on all public types
- `no_std` support at all three resource tiers (no-alloc, alloc, std)

### Internal

- Coverage ratchet: CI measures line coverage across the `std` and `std + serde` feature
  sets using `cargo-llvm-cov`. The baseline is stored in `.coverage-baseline`; CI fails if
  coverage drops more than 0.1% below it. On pushes to `main` or `develop`, coverage
  improvements are committed automatically via a `ci/coverage-ratchet` PR.
