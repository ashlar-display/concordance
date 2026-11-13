# Testing strategy

Negotiation logic benefits from a testing approach that combines small deterministic tests
with larger corpus-based validation.

## Test categories

### Unit tests

Unit tests cover narrow pieces of logic and live next to the code they test. Constraint
engine tests call `check` directly on handcrafted capability structs without going through
the full pipeline. Enumerator tests assert on the candidate set produced from a given
capability triple. Ranker tests assert on sort order given a fixed accepted set.

This keeps failures localized: a failing test in the engine can only mean the engine is
broken.

### Integration tests

A single integration test verifies that `NegotiatorBuilder::default()` wires the pipeline
correctly and that `negotiate` invokes all three components. It does not duplicate the
field-level assertions that belong in component unit tests.

### Fixture tests

Concordance should maintain a fixture corpus containing:

- valid capability triples from real hardware,
- capability declarations with known inconsistencies,
- edge cases (TMDS-only cable, DSC required, VRR boundary conditions),
- pathological inputs.

A suggested layout:
```text
testdata/
 ├── valid/
 ├── invalid/
 └── edge/
```

Fixtures serve as a regression suite and a confidence base for refactoring negotiation logic
without unintentionally changing behaviour.

### Fuzzing

Fuzzing is strongly recommended for the constraint engine and enumerator.

Important expectations:

- no panics,
- no uncontrolled memory growth,
- any input produces controlled output (violations, warnings) rather than undefined behaviour,
- unknown or conflicting capability values do not break pipeline invariants.

## Feature sets

CI runs the tests with the default features, with `serde`, with `alloc` only and with no
default features, on stable and on the MSRV. Without `alloc` the builder and the ranker do
not exist, so their tests are gated on `alloc` like the code they test, as are the
enumerator's. The tests of the engine, `is_config_viable`, the rules and the FRL capacity
model need `alloc` too; in every build run the tests of the individual constraint checks,
`CheckResult`, the FRL model and the input types, and the README's probe example, which
runs `is_config_viable` without an allocator. The bare `no_std` and `alloc`-only builds are
compiled for `thumbv7em-none-eabi`, which has no `std`.

The README's Rust examples run as doctests (`ReadmeDoctests` in `src/lib.rs`), against a
hidden sink that takes 4K60 RGB at 8 bpc, with hidden asserts on the outcomes; the probe
example also runs against a sink that declares nothing, so both of its arms run. The
examples in the crate documentation (`doc/architecture.md`) run as doctests too; those
that need `alloc` are wrapped in a hidden `#[cfg(feature = "alloc")]`.

## Test philosophy

Concordance should be strict about HDMI compliance, but practical about
real-world hardware inconsistencies.

The test suite should reflect that balance by checking both:

- outright rejection of structurally invalid configurations,
- graceful handling of conflicting or incomplete capability declarations.

## Long-term goal

As the fixture corpus grows, it should become a source of confidence for refactoring the
constraint engine, adjusting ranking policy, and extending the pipeline without
unintentionally changing behaviour.
