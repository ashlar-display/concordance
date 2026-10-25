# concordance

[![CI](https://github.com/ashlar-display/concordance/actions/workflows/ci.yml/badge.svg)](https://github.com/ashlar-display/concordance/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/concordance.svg)](https://crates.io/crates/concordance)
[![docs.rs](https://docs.rs/concordance/badge.svg)](https://docs.rs/concordance)
[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](https://github.com/ashlar-display/concordance/blob/main/LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/rustc-1.85+-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)
[![SLSA Level 2](https://slsa.dev/images/gh-badge-level2.svg)](https://slsa.dev)
[![trusted publishing only](https://img.shields.io/badge/dynamic/json?url=https%3A%2F%2Fcrates.io%2Fapi%2Fv1%2Fcrates%2Fconcordance&query=%24.crate.trustpub_only&label=trusted%20publishing%20only)](#verifying-releases)

> **Part of [Ashlar](https://ashlar-display.org)** — a display connection stack in Rust,
> HDMI 2.1 first: from EDID parsing through mode negotiation to FRL link training.
>
> [display-types](https://github.com/ashlar-display/display-types) ·
> [piaf](https://github.com/ashlar-display/piaf) ·
> **concordance** ·
> [cartouche](https://github.com/ashlar-display/cartouche) ·
> [culvert](https://github.com/ashlar-display/culvert) ·
> [plumbob](https://github.com/ashlar-display/plumbob) ·
> [hdmi-hal](https://github.com/ashlar-display/hdmi-hal) ·
> [hdmi-hal-i2c-dev](https://github.com/ashlar-display/hdmi-hal-i2c-dev)

HDMI 2.1 mode negotiation — policy layer of the display connection stack.

Given sink, source, and cable capabilities, concordance answers: "what modes can I drive
on this display, in what priority order, using what color format and bit depth?"

The library takes three caller-supplied structs — `SinkCapabilities`, `SourceCapabilities`,
and `CableCapabilities` — and runs them through a three-stage pipeline: enumerate all
candidate configurations, check each against HDMI specification constraints, rank the
accepted candidates according to a policy. The output is a ranked `Vec<NegotiatedConfig>`,
each entry carrying the resolved mode, color format, FRL tier, and any non-fatal warnings.
A `ReasoningTrace` on each entry records why a configuration was accepted or rejected, so
every output is fully explainable.

For embedded and firmware targets, the constraint engine is also exposed as a standalone
`no_std`-compatible probe:

```rust
use concordance::{is_config_viable, SinkCapabilities, SourceCapabilities, CableCapabilities, CandidateConfig};
use display_types::{ColorBitDepth, ColorFormat};
use display_types::cea861::{HdmiForumFrl, vic_to_mode};

// For standard CTA modes, use vic_to_mode — it carries the exact pixel clock from
// the CEA-861 timing table, so bandwidth ceiling checks are precise.
// VIC 97 = 3840×2160 @ 60 Hz (594 MHz). Other common VICs: 16 = 1080p@60, 97 = 4K@60.
let mode = vic_to_mode(97).unwrap();
let config = CandidateConfig::new(
    &mode,
    ColorFormat::Rgb444,
    ColorBitDepth::Depth8,
    HdmiForumFrl::NotSupported,
    false,
);

match is_config_viable(&sink, &source, &cable, &config) {
    Ok(_warnings) => println!("viable"),
    Err(violations) => {
        // Each violation is a TaggedViolation<Violation>: Display shows "[rule] message".
        for v in &violations { eprintln!("rejected: {v}"); }
    }
}
```

For the full ranked pipeline (`alloc`/`std`):

```rust
use concordance::NegotiatorBuilder;

let configs = NegotiatorBuilder::default()
    .negotiate(&sink, &source, &cable);

for cfg in &configs {
    println!("{}×{}@{} {:?} {:?}",
        cfg.resolved.mode.width, cfg.resolved.mode.height, cfg.resolved.mode.refresh_rate,
        cfg.resolved.color_encoding, cfg.resolved.bit_depth);
}
```

Custom constraint rules can be injected without replacing the default engine:

```rust
use concordance::NegotiatorBuilder;

let configs = NegotiatorBuilder::default()
    .with_extra_rule(PlatformBandwidthRule::new(limits))
    .negotiate(&sink, &source, &cable);
```

## Why concordance

**Ranked iterator, not a verdict.** There is no single right answer. The library enumerates
all valid configurations in a defined, documented priority order and lets the caller pick.
No mode is silently discarded — rejections appear in the reasoning trace.

**Every output is explainable.** Each `NegotiatedConfig` carries a `ReasoningTrace`
recording the decisions made during negotiation. A driver or diagnostic tool can explain
exactly why a given mode was selected or excluded.

**Extensible without forking.** The three pipeline components — `ConstraintEngine`,
`CandidateEnumerator`, `ConfigRanker` — are traits with default implementations. Any
component can be replaced or wrapped via `NegotiatorBuilder` without touching crate source.
For the common case of adding a rule on top of the default checks, `with_extra_rule` injects
a single `ConstraintRule` without reimplementing everything else.

**Custom violation and warning types.** `ConstraintEngine` and `ConfigRanker` declare
associated `Warning` and `Violation` types bounded by `Diagnostic`. A custom component emits
its own variants with full type fidelity — no wrapping, no loss of structured information.

**Cable is a first-class constraint.** Link capability is determined by source, sink, *and*
cable. A cable that cannot carry FRL or whose TMDS clock ceiling is too low produces a
`Violation` like any other constraint failure. `CableCapabilities::unconstrained()` is
provided for callers without cable information.

**`no_std` at all resource levels.** Three build tiers are explicitly supported:
- Bare `no_std` (no allocator): `is_config_viable` borrows all inputs, no heap use.
- `no_std + alloc`: the ranked pipeline and `ReasoningTrace` are available.
- `std`: full feature set, additive on top of `alloc`.

**Serde on all public types.** Every public type derives `Serialize`/`Deserialize` behind
the `serde` feature flag, covering inputs, outputs, and policy types.

## Feature flags

| Flag    | Default | Description                                              |
|---------|---------|----------------------------------------------------------|
| `std`   | yes     | Enables the ranked pipeline and `ReasoningTrace`; implies `alloc` |
| `alloc` | no      | Enables the ranked pipeline without `std`                |
| `serde` | no      | Derives `Serialize`/`Deserialize` on all public types    |

Without `alloc` or `std`, only `is_config_viable` is available. The three input structs and
`CandidateConfig` are available at all tiers.

## Stack position

concordance is the policy layer. It consumes the typed model; it does not produce it.

```mermaid
flowchart LR
    piaf["piaf\n(EDID parser)"]
    dc["DisplayCapabilities"]
    sink["SinkCapabilities"]
    nb["NegotiatorBuilder"]
    out["Vec&lt;NegotiatedConfig&gt;"]
    integ["integration layer\n(GPU / cable discovery)"]

    piaf --> dc
    dc -->|"sink_capabilities_from_display"| sink
    sink --> nb
    integ -->|"SourceCapabilities\nCableCapabilities"| nb
    nb --> out
```

All types in `SinkCapabilities` — `VideoMode`, `HdmiForumSinkCap`, `HdmiVsdb`,
`ColorCapabilities`, etc. — are from
[`display-types`](https://crates.io/crates/display-types), the shared vocabulary crate.
`sink_capabilities_from_display` in concordance bridges a parsed `DisplayCapabilities`
directly into `SinkCapabilities`; the integration layer only needs to supply source and
cable capabilities.

## Out of scope

- **EDID parsing** — that's [piaf](https://github.com/ashlar-display/piaf).
- **Source capability discovery** — querying DRM/KMS or VBIOS for GPU limits.
- **Cable capability discovery** — reading the HDMI cable type marker or accepting a
  user override.
- **Link training** — determining whether a negotiated FRL tier is achievable on real hardware.
- **InfoFrame encoding** — signaling the negotiated configuration to the sink.
- **HDCP** — out of scope for the entire stack.

## Documentation

Extended documentation lives under [`doc/`](doc/).

**Understanding the library**

- [`doc/architecture.md`](doc/architecture.md) — pipeline structure, constraint rules, ranking algorithm, and design principles
- [`doc/enumerator.md`](doc/enumerator.md) — candidate enumeration, Cartesian product dimensions, and pre-filtering

**Contributing**

- [`doc/setup.md`](doc/setup.md) — build, test, and coverage commands
- [`doc/testing.md`](doc/testing.md) — testing strategy, fixture corpus, and CI expectations
- [`doc/roadmap.md`](doc/roadmap.md) — planned features and future work

## Verifying releases

Releases are built on GitHub Actions and attested with
[SLSA Build Level 2](https://slsa.dev) provenance (from 0.2.0). To verify a release
`.crate` against its signed provenance, install the
[GitHub CLI](https://cli.github.com/) and run:

```sh
gh attestation verify concordance-X.Y.Z.crate --repo ashlar-display/concordance
```

The release from before the repository moved to the `ashlar-display` organization (0.2.0)
was attested under its previous owner, and verifies against it:

```sh
gh attestation verify concordance-0.2.0.crate --owner DracoWhitefire \
  --signer-repo DracoWhitefire/concordance
```

The attested `.crate` is attached to each
[GitHub release](https://github.com/ashlar-display/concordance/releases).

Those releases are also uploaded through crates.io
[trusted publishing](https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html): the publish
workflow obtains a short-lived token via OpenID Connect instead of using a stored
API token. crates.io can also reject uploads made with an API token for this crate;
the badge above shows whether that setting is on. Each version's publishing details,
including the workflow run and commit, are available from the crates.io API:

```sh
curl -A "your-tool (contact)" https://crates.io/api/v1/crates/concordance/X.Y.Z
# see .version.trustpub_data
```

0.1.0 was uploaded with an API token; it carries no provenance and has no GitHub release.

## License

Licensed under the [Mozilla Public License 2.0](https://github.com/ashlar-display/concordance/blob/main/LICENSE).
