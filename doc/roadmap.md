# Roadmap

## Planned

### Fixture corpus

A `testdata/` corpus of real capability triples and known-bad inputs, providing a regression
suite and a confidence base for refactoring constraint logic.

### Fuzzing

Fuzz targets for the constraint engine and enumerator covering panic-safety, memory bounds,
and pipeline invariants under adversarial input.

### Broader constraint coverage

Additional built-in `ConstraintRule` implementations covering edge cases currently left to
callers: VRR range validation, Deep Color bandwidth margins, ALLM and QMS interaction checks.

### Compositor policy knobs

`NegotiationPolicy` currently exposes coarse flags (`prefer_high_refresh`,
`prefer_color_fidelity`, etc.) that rank all valid configs globally. Compositors managing
per-output policy need finer control without replacing the full ranker. Planned additions:

- `preferred_refresh_hz: Option<u16>` — soft preference for a specific refresh rate; modes
  closer to the target rank higher within the same resolution. Supports content-rate matching
  (e.g. prefer integer multiples of 24 Hz for film playback).
- `max_resolution_pixels: Option<u32>` — soft ceiling on total pixel count; modes above the
  ceiling rank lower without being hard-rejected. Complements `with_extra_rule` for callers
  who want a preference rather than a hard filter.
- `exclude_interlaced: bool` — convenience flag to penalise or hard-reject interlaced modes,
  which compositors universally avoid. Today this requires a custom `with_extra_rule`.
- Soft color format preference — within a given resolution and refresh, prefer RGB over YCbCr
  for compositing efficiency. Currently `allow_ycbcr` is a hard on/off; a soft signal is
  more flexible when YCbCr is acceptable but not preferred.
- `prefer_vrr: bool` — prefer VRR-applicable modes once VRR constraint checking lands (I1).

Hard go/no-go filters (resolution floor, aspect ratio, refresh ceiling) are already
expressible today via `with_extra_rule`; that pattern is documented in
`doc/architecture.md`. The additions above cover the soft-preference cases where a rule
would be too blunt.

### Multi-output model gaps

The current API is per-output: one `SinkCapabilities`, one negotiation result. Three
compositor use cases require cross-output reasoning that the current model cannot express:

- **Clone / mirror mode** — the compositor needs the intersection of two sinks' supported
  modes to find configurations valid on both outputs simultaneously. There is no
  multi-sink entry point.
- **Cross-output bandwidth budget** — GPU memory bandwidth is shared across all active
  outputs. A per-output negotiation cannot know what headroom the other outputs are
  consuming.
- **Portrait / rotation** — `VideoMode` and `NegotiationPolicy` carry no rotation
  information. A rotated output's effective resolution (swapped width/height) is invisible
  to the constraint engine.

These are scope expansions rather than policy knob additions and are tracked separately.
The right solution likely involves a multi-output negotiation entry point that receives all
active sink/source/cable triples and returns a jointly valid configuration set.

### VRR constraint implementation

`NegotiatedConfig.resolved.vrr_applicable` is always `false` today. Completing this requires
implementing VRR range validation (min/max refresh from the sink's VRR range descriptor) as a
`ConstraintRule` and updating `vrr_applicable` in `DefaultRanker` to reflect the result. Until
then the field's doc comment documents it as always-false.

### DSC parameter resolution

`DscCapabilities` captures `max_slices` and `max_bpp_x16` from the source, but neither is
validated in `DscCheck` nor surfaced in output. Completing this requires validating slice count
and BPP against source and sink limits and adding a resolved `DscConfig` struct (or equivalent
fields) to `NegotiatedConfig` so kernel drivers and firmware have actionable compression
parameters to program the encoder. The FRL capacity check comes with it: DSC candidates are
reported unverified until then (amdgpu's `dml1_frl_cap_chk_compressed` is the public
reference).

### Dropping candidates by policy

`NegotiationPolicy` can deprioritize FRL candidates with a capacity warning, not drop them:
the ranker has no channel to report a candidate it removes, and accepted candidates are
never discarded silently. A "rejected by policy" entry in the negotiation log would allow a
`Drop` option (and a "safe" preset) without breaking that rule.
