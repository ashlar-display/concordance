# Architecture

## Role

Given sink, source, and cable capabilities (all caller-supplied structs), produce a ranked
list of viable configurations. Answers: "what modes can I drive on this display, in what
priority order, using what color format and bit depth?"

Concordance is the policy layer of the stack. Parsing layers below it make no judgements.
Hardware layers above it implement specification. Concordance is explicitly opinionated, but
its opinions are configurable and its reasoning is always visible.

## Scope

Concordance covers:

- validation of candidate configurations against HDMI 2.1 specification constraints,
- enumeration of all candidate configurations from the intersection of sink, source, and cable
  capabilities,
- ranking of accepted candidates according to a configurable policy,
- a `no_std`-compatible single-config probe (`is_config_viable`) for firmware and embedded targets,
- structured diagnostics: violations for rejected configurations; warnings for accepted
  configurations with caveats.

The following are out of scope:

- **Sink capability discovery** — parsing EDID and HF-VSDB into `SinkCapabilities` belongs in
  the parsing layer (e.g. `piaf`). The integration layer converts parsed output into this
  library's `SinkCapabilities` struct.
- **Source capability discovery** — querying DRM/KMS or VBIOS for actual GPU limits belongs in
  the integration layer.
- **Cable capability discovery** — reading the HDMI cable type marker or accepting a user-supplied
  override belongs in the integration layer.
- **Link training** — determining whether a negotiated FRL tier is achievable on real hardware.
- **InfoFrame encoding** — signaling the negotiated configuration to the sink.
- **HDCP** — out of scope for the entire stack.

## Inputs and Output

**Inputs:**
- `SinkCapabilities` — a struct defined in this library, filled in by the caller
- `SourceCapabilities` — a struct defined in this library, filled in by the caller
- `CableCapabilities` — a struct defined in this library, filled in by the caller

**Output:** A ranked iterator of `NegotiatedConfig<W>`, each entry containing:
- `resolved: ResolvedDisplayConfig` — the hardware-ready output: `VideoMode`, color format
  and bit depth, FRL tier (or TMDS), DSC required flag, VRR applicability
- `Vec<W>` — non-fatal warnings about the accepted configuration
- `ReasoningTrace`

## SinkCapabilities

`SinkCapabilities` is a plain struct the caller fills in manually. Populating it from a
parsed `DisplayCapabilities` (from `display-types`) is the concern of the integration layer,
not this library.

```text
#[non_exhaustive]
pub struct SinkCapabilities {
    // Video modes declared by the display
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub supported_modes: Vec<VideoMode>,

    // Timing range limits (from EDID range limits descriptor)
    pub max_pixel_clock_mhz: Option<u16>,
    pub min_v_rate: Option<u16>,
    pub max_v_rate: Option<u16>,

    // Color encoding (from EDID base block)
    pub digital_color_encoding: Option<DigitalColorEncoding>,
    pub color_bit_depth: Option<ColorBitDepth>,

    // HDMI 1.x capabilities (from HDMI VSDB; None if not present)
    pub hdmi_vsdb: Option<HdmiVsdb>,

    // HDMI 2.1 capabilities (from HF-SCDB; None for pre-HDMI-2.1 sinks)
    pub hdmi_forum: Option<HdmiForumSinkCap>,

    // HDR and colorimetry
    pub hdr_static: Option<HdrStaticMetadata>,
    pub colorimetry: Option<ColorimetryBlock>,
}
```

`VideoMode`, `DigitalColorEncoding`, `ColorBitDepth`, `HdmiVsdb`, `HdmiForumSinkCap`,
`HdrStaticMetadata`, and `ColorimetryBlock` are all from `display-types`. `supported_modes`
is absent in bare `no_std` builds; `is_config_viable` does not need the mode list since
it validates a caller-supplied candidate rather than enumerating one.

## SourceCapabilities

`SourceCapabilities` is a plain struct the caller fills in manually. Populating it from
actual GPU hardware is the concern of the source capability discovery library in the
integration layer, not this library.

```text
#[non_exhaustive]
pub struct SourceCapabilities {
    pub max_tmds_clock: u32,
    pub max_frl_rate: HdmiForumFrl,
    pub dsc: Option<DscCapabilities>,
    pub quirks: QuirkFlags,
    // ...
}
```

`HdmiForumFrl` is from `display-types`. FRL rates are cumulative — declaring a maximum
implies support for all lower tiers — so a single `max_frl_rate` is the right
representation. `HdmiForumFrl::NotSupported` indicates a TMDS-only source.
`#[non_exhaustive]` is used for forward compatibility. This struct represents real hardware
limits and may include vendor quirks.

## CableCapabilities

`CableCapabilities` is a plain struct the caller fills in manually. Populating it from
actual cable identification (e.g. HDMI cable type marker read from the sink EDID, or
user-supplied override) is the concern of the integration layer, not this library.

```text
#[non_exhaustive]
pub struct CableCapabilities {
    pub hdmi_spec: HdmiSpec,         // e.g. Hdmi14, Hdmi20, Hdmi21
    pub max_frl_rate: HdmiForumFrl,  // NotSupported = TMDS-only cable
    pub max_tmds_clock: u32,
    // ...
}
```

`HdmiSpec` is a concordance-defined enum encoding the cable's declared HDMI version.
`HdmiForumFrl` is from `display-types`. A cable may be the binding constraint even when
both source and sink are HDMI 2.1 capable.

`CableCapabilities::unconstrained()` is provided as a convenience for callers that have
no cable information and wish to fall back to the optimistic assumption (source + sink
limits only).

## Internal Architecture

The negotiation layer is structured into three components, each defined as a trait with a
default implementation. Callers can substitute any component without forking the crate.
The constraint engine additionally supports rule injection — adding checks on top of the
default implementation — via a `ConstraintRule` trait and a `Layered` combinator.

```text
pub trait ConstraintEngine { ... }
pub trait CandidateEnumerator { ... }
pub trait ConfigRanker { ... }
```

The components are wired together via `NegotiatorBuilder`, which accepts concrete
implementations for each slot and falls back to the defaults when none is supplied.

### 1. Constraint Engine

Determines whether a given configuration is valid for the supplied sink, source, and cable.
Returns structured violations, not just a boolean.

The default implementation enforces HDMI specification rules, in this order
(`engine::checks::DEFAULT_CHECKS`): FRL ceiling, FRL capacity, refresh rate range, pixel
clock ceiling, TMDS clock ceiling, colour encoding, bit depth, DSC. Callers can wrap or
replace it to add vendor-specific constraint rules (e.g. platform bandwidth caps, quirk
overrides) without touching the rest of the pipeline.

This is also exposed directly as the `no_std`-compatible binary probe:

```text
pub fn is_config_viable(
    sink: &SinkCapabilities,
    source: &SourceCapabilities,
    cable: &CableCapabilities,
    config: &CandidateConfig,
) -> Result<(), Vec<Violation>>
```

The ranked iterator is built on top of this primitive. Firmware and embedded consumers that
cannot afford allocation or iteration use this function directly.

#### Constructing a `VideoMode` on firmware targets

`is_config_viable` requires a `CandidateConfig` holding a `&VideoMode`. Firmware that does
not go through EDID parsing has two construction paths:

**Standard CTA modes (recommended)** — use `display_types::cea861::vic_to_mode`. Every
standard HDMI mode has a Video Identification Code; `vic_to_mode` returns a `VideoMode` with
the exact pixel clock from the CEA-861 timing table, so the pixel clock constraint checks are
precise:

```rust,no_run
# use display_types::cea861::vic_to_mode;
// VIC 97 = 3840×2160 @ 60 Hz, 594 000 kHz
let mode = vic_to_mode(97).expect("VIC 97 is in the table");
```

VIC numbers for common modes: 16 = 1080p@60, 31 = 1080p@50, 93 = 4K@24, 97 = 4K@60,
107 = 4K@120 (via FRL). The full table is in `display-types/src/cea861/vic_table.rs`.

**Non-CTA / custom timings** — use `VideoMode::new` followed by `.with_pixel_clock` if the
exact clock is known:

```rust,no_run
# use display_types::VideoMode;
// Custom panel: supply the exact pixel clock from the PLL or hardware register.
let mode = VideoMode::new(1920, 1200, 60u16, false).with_pixel_clock(154_000);
```

Without `.with_pixel_clock`, the pixel clock is derived via CVT-RB estimation, which
under-estimates for HDMI Forum CTA modes by roughly 10–15% and can produce false accepts in
bandwidth ceiling checks. For custom timings where the exact clock is unavailable, the estimate
is the best option — just note the caveat for modes near a bandwidth ceiling.

#### FRL capacity

`FrlCeilingCheck` only compares an FRL rate with the sink's, source's and cable's ceilings.
Whether that rate can actually carry the mode is `FrlCapacityCheck`'s question, answered by
the `frl` module's `frl_capacity`: a line fits only if its audio packets fit in the
blanking, the active video can borrow what it needs from the blanking (at most 400
tribytes), and the line's characters plus the link overhead fit in the characters the rate
delivers per line.

**No ground truth.** The HDMI 2.1 specification is not public, and this computation does
not come from it. It follows the public implementations of the same procedure, amdgpu's
`dml1_frl_cap_chk` and i915's FRL DFM, which share the procedure and its constants and
differ in details. Where one contradicts its own comments or other formulas, the
consistent reading is used (the blanking time counts one character per tribyte; the
Audio Clock Regeneration packet is not compressible; the borrow limit applies). Where
neither reading is visibly wrong, the point is an `Assumptions` flag (characters per line
floored or rounded up, the margin's tolerance, the overhead's truncation, two audio
layout and rate questions, the audio load, and EDID 1.3 versus 1.4 for DTD borders).

**Evaluated at every reading.** `frl_capacity` evaluates every combination of the
assumptions that apply and reports the worst (`strict`) and the best (`permissive`)
evaluation, with every intermediate value, and the assumptions that alone decide. That
band is the honest extent of what is known; when a point is settled (specification
access, an erratum, hardware evidence), its flag stops applying and the band narrows.
All arithmetic is exact integer arithmetic, `no_std`, no allocation.

| Strict | Permissive | Outcome |
|---|---|---|
| fails | fails | rejected: `Violation::FrlCapacityExceeded { rate, shortfall }` |
| fails | passes | accepted with `Warning::FrlCapacityMarginal { rate, deciding, shortfall }` |
| passes | passes | accepted |

A candidate whose capacity cannot be checked (a mode without `h_blank` or an exact pixel
clock, a DSC candidate) or not fully (an audio format no implementation models; no audio
data without alloc) is accepted with `Warning::FrlCapacityUnverified { rate, reason }`,
never silently. The violation and warnings carry what explains the outcome;
`FrlCapacityCheck::report(sink, config)` returns the full report on demand, deterministic,
so a driver can compare it with its own vendor check, or replace the rule.

**Audio.** Audio travels in the blanking. The sink bounds it: a source may send only the
formats the sink declares, and `sink_capabilities_from_display` maps each Short Audio
Descriptor (and basic audio) to the heaviest load a source may send for it
(`AudioLoad::from_descriptor`, at IEC 61937 rates). `frl_capacity` borrows the loads through
`FrlAudio`: `AtMost` for a bound like the sink's (the source may also send none, so the
audio load is one of the assumptions), `OneOf` for what the source will send. In bare
`no_std` builds `SinkCapabilities` carries no audio loads, so the engine reports each FRL
candidate unverified for audio; firmware that knows its audio calls `frl_capacity` directly
with a `static` or stack slice.

Which of the rates that fit to prefer, and what to do with marginal or unverified
candidates, is the ranker's and `NegotiationPolicy`'s (see criterion 1b below).

#### Rule injection

`ConstraintEngine` enables full replacement of the constraint policy, but replacement
requires reimplementing all HDMI specification rules — forking in disguise. For the common
case of adding rules on top of the default checks, a finer-grained unit of extensibility
is provided:

```text
pub trait ConstraintRule<V: Diagnostic, W: Diagnostic = Warning> {
    fn display_name(&self) -> &'static str;
    fn check(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V>;
    // Provided; reports nothing by default.
    fn warnings(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
        warn: &mut dyn FnMut(W),
    ) {}
}
```

`ConstraintRule<V, W>` is the unit of a single check — it either finds a violation of type
`V` or it doesn't, and for a candidate no rule rejected it may report warnings of type `W`
(the built-in `Warning` unless a custom engine needs its own). `ConstraintEngine`
coordinates a full pass (collecting all violations, or short-circuiting on the first in
no-alloc mode), and collects the rules' warnings for accepted candidates; `TaggingAdapter`
and both `Layered` forms forward them.

A `Layered<E, R>` combinator extends a base engine with an additional rule, running both
in sequence. The extra rule must produce the same violation type as the engine:

```text
impl<E, R> ConstraintEngine for Layered<E, R>
where
    E: ConstraintEngine,
    R: ConstraintRule<E::Violation, E::Warning>,
{
    type Warning = E::Warning;
    type Violation = E::Violation;
    // ...
}
```

`NegotiatorBuilder` exposes a composing entry point so a caller never needs to construct
`Layered` directly. The builder wraps the supplied rule in a `TaggingAdapter` that
automatically prefixes each violation with the rule's `display_name`, matching the tagging
format of the built-in violations:

```text
impl NegotiatorBuilder<E, En, R> {
    pub fn with_extra_rule<X, V>(self, rule: X) -> NegotiatorBuilder<Layered<E, TaggingAdapter<X>>, En, R>
    where
        E: ConstraintEngine<Violation = TaggedViolation<V>>,
        X: ConstraintRule<V>,
        V: Diagnostic + 'static,
    { ... }
}
```

A platform-specific caller writes only their rule and passes it in:

```rust,no_run
# use concordance::{NegotiatorBuilder, CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities, Violation};
# use concordance::engine::rule::ConstraintRule;
# struct PlatformBandwidthRule { max_khz: u32 }
# impl PlatformBandwidthRule { fn new(max_khz: u32) -> Self { Self { max_khz } } }
# impl ConstraintRule<Violation> for PlatformBandwidthRule {
#     fn display_name(&self) -> &'static str { "platform_bandwidth" }
#     fn check(&self, _: &SinkCapabilities, _: &SourceCapabilities,
#              _: &CableCapabilities, _: &CandidateConfig<'_>) -> Option<Violation> { None }
# }
# let (sink, source, cable) = (SinkCapabilities::default(), SourceCapabilities::default(), CableCapabilities::default());
# let limits = 594_000u32;
let configs = NegotiatorBuilder::default()
    .with_extra_rule(PlatformBandwidthRule::new(limits))
    .negotiate(&sink, &source, &cable);
```

`DefaultConstraintEngine` is itself decomposed into `ConstraintRule` implementations
internally, so advanced callers who need selective control — including or excluding
specific built-in checks — can compose their own engine from individual rules without
reimplementing any specification logic.

#### Hard filters for compositor and driver callers

`with_extra_rule` is the right mechanism for any go/no-go constraint that does not belong
in the HDMI specification rule set — resolution floors, refresh rate ceilings, aspect ratio
enforcement, or platform-specific bandwidth caps. A rule that returns `Some(violation)` for
every candidate outside an acceptable range will exclude those candidates from the ranked
output cleanly, without touching the ranker or the policy.

Examples:

```text
// Exclude all modes below 1080p total pixels.
struct MinResolutionRule { min_pixels: u32 }

impl ConstraintRule<MyViolation> for MinResolutionRule {
    fn display_name(&self) -> &'static str { "min_resolution" }
    fn check(&self, ..., config: &CandidateConfig<'_>) -> Option<MyViolation> {
        let pixels = config.mode.width as u32 * config.mode.height as u32;
        if pixels < self.min_pixels { Some(MyViolation::ResolutionBelowMinimum) } else { None }
    }
}

// Exclude interlaced modes entirely.
struct NoInterlacedRule;

impl ConstraintRule<MyViolation> for NoInterlacedRule {
    fn display_name(&self) -> &'static str { "no_interlaced" }
    fn check(&self, ..., config: &CandidateConfig<'_>) -> Option<MyViolation> {
        if config.mode.interlaced { Some(MyViolation::InterlacedNotPermitted) } else { None }
    }
}

let configs = NegotiatorBuilder::default()
    .with_extra_rule(MinResolutionRule { min_pixels: 1920 * 1080 })
    .with_extra_rule(NoInterlacedRule)
    .negotiate(&sink, &source, &cable);
```

Soft preferences — preferring a specific refresh rate, or ranking RGB above YCbCr within the
same resolution — belong in the `NegotiationPolicy` or a custom `ConfigRanker`, not in
constraint rules. The distinction: a constraint rule eliminates candidates; a ranker orders
those that remain.

### 2. Enumerator

Generates all candidate configurations from the intersection of sink, source, and cable
capabilities. Completely policy-free: the enumerator produces candidates; it never
pre-filters based on perceived usefulness. No candidate is dropped at enumeration time —
rejection happens only in the constraint engine.

Equivalent candidates (same mode, format, and tier reached by different paths) are
deduplicated by the pipeline before ranking.

Custom enumerators can restrict or expand the candidate set (e.g. to limit enumeration to a
specific resolution list on embedded targets) without altering constraint or ranking logic.

See [`doc/enumerator.md`](enumerator.md) for a detailed description of the Cartesian product
dimensions, pre-filtering optimisation, and iterator implementation.

### 3. Ranker

Orders the validated candidates according to a `NegotiationPolicy`. The default policy
encodes a sensible preference (native resolution, max color fidelity, then refresh rate,
then fallback formats), but the caller can supply an override.

Named policy presets (`BestQuality`, `BestPerformance`, `PowerSaving`) are a thin layer on
top of the same ranked iterator. Custom `NegotiationPolicy` implementations can encode
platform-specific priorities (e.g. always prefer a specific refresh rate, or penalize DSC).

#### Default ranking algorithm

`DefaultRanker` implements a stable multi-criterion sort. The comparison function applies
criteria in priority order; the first non-equal result determines the relative order of two
configs. Higher-ranked configs appear earlier in the output.

**Native resolution detection.** The `rank` signature does not receive capabilities, so
native resolution is inferred from the accepted set: the mode with the greatest pixel area
(`width × height`) is treated as the native resolution. This is the correct heuristic —
the display's native resolution is its highest declared mode, and any such mode in the
accepted set has already passed the constraint engine.

**Sort criteria, in order:**

| # | Criterion                     | Direction         | Controlled by                                  |
|---|-------------------------------|-------------------|------------------------------------------------|
| 1 | DSC required                  | `false` first     | `penalize_dsc`                                 |
| 1b | FRL capacity warning         | without first     | `frl_marginal`, `frl_unverified`               |
| 2 | Native resolution             | native first      | `prefer_native_resolution`                     |
| 3 | Quality/performance dimension | see below         | `prefer_color_fidelity`, `prefer_high_refresh` |
| 4 | Interlaced                    | progressive first | always                                         |
| 5 | FRL rate                      | lower first       | always                                         |
| 6 | Resolution area               | larger first      | always (tiebreaker)                            |

**Quality/performance dimension (criterion 3).** The two policy flags jointly determine
which sub-criteria are applied and in what order:

- `prefer_color_fidelity = true` — bit depth (desc), color format quality (desc), refresh
  rate (desc). Color fidelity is the primary driver; refresh rate breaks ties within the
  same quality level.
- `prefer_high_refresh = true`, `prefer_color_fidelity = false` — refresh rate (desc), bit
  depth (desc), color format quality (desc). Refresh rate is the primary driver.
- Both false (power saving) — refresh rate (asc), bit depth (asc), color format quality
  (asc, simpler first). Lower bandwidth and lower power draw are preferred; the direction
  of all three sub-criteria is reversed.

**Color format quality.** Ranked 3 → 0: `Rgb444` (3), `YCbCr444` (2), `YCbCr422` (1),
`YCbCr420` (0). RGB ranks above YCbCr444 at the same chroma resolution because it requires
no color-space conversion at the sink. In power-saving mode the order is reversed:
`YCbCr420` is preferred because it carries the least chroma data.

**DSC penalty.** DSC is "visually lossless" compression but is still lossy: the sink
reconstructs rather than preserves original pixel data. An uncompressed transport at the
same resolution, format, and depth is strictly preferable. The penalty pushes DSC configs
behind their uncompressed equivalents so they act as fallbacks, not first choices.
`BEST_PERFORMANCE` disables the penalty: a high-refresh DSC mode may legitimately rank
above a lower-refresh uncompressed one when performance is the goal.

**FRL capacity warnings (criterion 1b).** `frl_marginal` and `frl_unverified` each say
`Keep` (rank as usual) or `Deprioritize` (rank after every config without that warning).
All presets deprioritize marginal candidates — there is evidence of risk, so a caller
taking the first entry gets a sure configuration whenever one exists — and keep
unverified ones, which mostly mean "could not check": one unmodelled audio format would
otherwise push every FRL candidate below TMDS. There is no option to drop them: the ranker
cannot report what it removes, and accepted candidates are never discarded silently. A
caller that never wants them filters on the warnings each configuration carries.

**FRL rate tiebreaker.** When two configs are otherwise equal, the one using the lower FRL
rate is ranked first. A lower FRL rate achieves the same result at reduced link complexity
and power; there is no reason to prefer a higher tier when a lower one suffices.

**ReasoningTrace.** After sorting, `DefaultRanker` appends a `PreferenceApplied` step to
each config describing the criteria that apply to that specific config (e.g.
`"DSC penalized"`, `"native resolution preferred"`, `"progressive mode preferred"`). These
are per-config facts, not relative comparisons — they give a diagnostic tool enough context
to explain why a config has the characteristics it does without requiring knowledge of the
full ranked list.

## NegotiatedConfig and ReasoningTrace

`NegotiatedConfig` is a pure data struct — it holds resolved values. Helpers that compute
derived results (compatibility checks, ranking utilities, mode filters) are free functions in
separate modules, not methods on the struct. This keeps the output type stable even as
higher-level policy evolves.

`NegotiatedConfig` is generic over the warning type, defaulting to the built-in `Warning`:

```text
pub struct NegotiatedConfig<W = Warning> {
    pub resolved: ResolvedDisplayConfig,
    pub warnings: Vec<W>,
    pub trace: ReasoningTrace,
}

/// Bound required on all warning types, built-in or custom.
pub trait Diagnostic: fmt::Display + fmt::Debug {}

#[derive(Debug, thiserror::Error)]
pub enum Warning {
    #[error("DSC required; lossy compression active")]
    DscActive,
    #[error("cable bandwidth marginal for selected mode")]
    CableBandwidthMarginal,
    FrlCapacityMarginal { rate, deciding, shortfall },
    FrlCapacityUnverified { rate, reason },
    // ...
}
impl Diagnostic for Warning {}

pub enum DecisionStep {
    Accepted { adjustments: Vec<Adjustment> },
    Rejected { reason: RejectionReason, details: String },
    PreferenceApplied { rule: PreferenceRule },
}

pub struct ReasoningTrace {
    pub steps: Vec<DecisionStep>,
}
```

`ConstraintEngine` and `ConfigRanker` each declare an associated `Warning` type bounded by
`Diagnostic`, and `ConstraintRule` takes one as a parameter, so a custom component can emit
its own warning variants without wrapping or losing type information. The default
implementations use the built-in `Warning`; the default engine's warnings come from its
rules (today the FRL capacity check).

Warnings are attached to accepted configurations — they do not prevent a mode from being
offered, but give the caller enough information to surface concerns to the user or log them.
Diagnostics are first-class and machine-readable. A compositor can ignore both; a driver or
diagnostic tool needs them.

## Error Handling

Fatal errors (invalid inputs, internal invariant violations) are represented as a
`thiserror`-derived `Error` type returned from fallible API entry points.

`Violation`, used in `is_config_viable`, is a `thiserror` error type with an associated
type on `ConstraintEngine`:

```text
pub trait ConstraintEngine {
    type Warning: Diagnostic;
    type Violation: Diagnostic;
    // ...
}
```

A custom `ConstraintEngine` implementation can define its own `Violation` type — adding
platform-specific rejection reasons — without wrapping the built-in enum or losing
structured information. The built-in `Violation` type remains the default.

Real hardware often declares inconsistent or conflicting capabilities. Where possible,
concordance produces the best available output and surfaces the inconsistency as a warning
rather than refusing to negotiate. Callers decide how strict to be.

`thiserror` is a build-time dependency only; it generates no runtime overhead.

## Consumer Perspectives

| Consumer             | What they want                                        |
|----------------------|-------------------------------------------------------|
| Compositor           | First valid entry from the ranked list; sane defaults |
| Driver / KMS bridge  | Full ranked list with reasoning trace; deterministic  |
| Firmware / embedded  | `is_config_viable` — no allocation, no iteration      |
| Test / validation    | Full enumeration including edge cases                 |
| End-user config tool | Named presets wrapping the ranked iterator            |

## Cable Consideration

HDMI link capability is determined by:

```text
Source + Sink + Cable → Link Training → Actual Limits
```

Concordance takes an explicit `CableCapabilities` input and treats the cable as a first-class
constraint alongside source and sink. A cable that cannot carry FRL, or whose TMDS clock
ceiling is below the required pixel rate, produces a `Violation` like any other constraint
failure.

Link training (in the SCDC/link training layer above) determines the real-world ceiling.
A `NegotiatedConfig` may still need to be revised downward after training, but the cable's
declared capabilities are enforced at negotiation time, not deferred.

Callers without cable information may pass `CableCapabilities::unconstrained()` to recover
the previous optimistic behavior.

## Design Principles

- **Ranked iterator, not a verdict** — there is no single right answer. The library
  enumerates all valid configurations in a defined, documented priority order and lets the
  caller pick. No mode is silently discarded — rejections appear in the trace.
- **No black box** — every output entry carries enough context for a driver or diagnostic
  tool to explain the choice.
- **Configurable behavior** — ranking priorities are governed by a `NegotiationPolicy` the
  caller supplies; named presets are provided for common cases. Constraint checking can be
  tuned via `QuirkFlags`. No behavioral choices are buried in the implementation.
- **Extensible without forking** — the three pipeline components (`ConstraintEngine`,
  `CandidateEnumerator`, `ConfigRanker`) are traits with default implementations. Any
  component can be replaced or wrapped via `NegotiatorBuilder` to accommodate
  platform-specific rules, restricted enumeration, or custom ranking, without touching the
  crate source. Warning and violation types are associated types on these traits, so custom
  components can emit their own diagnostic variants with full type fidelity.
- **`NegotiatedConfig` is a data struct, not a decision layer** — it holds resolved values.
  Derived operations live as free functions in separate modules, not as methods on the struct.
- **Tiered resource model** — three audiences are explicitly supported, each with its own
  build profile:
  - `no_std`, no alloc, no copy — `is_config_viable` borrows all inputs and returns
    structured violations with no heap use. Targets firmware and embedded consumers.
  - `no_std + alloc` — the ranked iterator and `ReasoningTrace` require allocation but
    avoid unnecessary copies; inputs are still borrowed throughout.
  - `std` — full feature set; additive on top of `alloc`.
  Borrowing is the default throughout the API. Owned types appear only where the output
  genuinely needs to outlive its inputs.
- **Stable output types** — `NegotiatedConfig` and the three input structs are
  `#[non_exhaustive]` and versioned. Consumers are insulated from internal changes.
- **No unsafe code** — `#![forbid(unsafe_code)]` is a hard constraint, not a guideline.
- **Serde on all public types** — every public type derives `Serialize`/`Deserialize` behind
  a `serde` feature flag, covering inputs, outputs, and policy types. Enables diagnostic
  tooling, config persistence, and test fixtures without making serde a required dependency.
- **Attested releases.** From 0.2.0, every release is published through a GitHub Actions
  workflow, with crates.io trusted publishing, that signs the `.crate` package with
  [SLSA Build Level 2](https://slsa.dev) provenance. Verify with
  `gh attestation verify <file> --repo ashlar-display/concordance` (0.2.0, from before the
  move to the `ashlar-display` organization: see "Verifying releases" in the README).
  0.1.0 carries no provenance.
