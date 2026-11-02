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
