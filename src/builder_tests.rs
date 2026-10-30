use super::*;
use crate::engine::{CheckResult, ConstraintEngine};
use crate::enumerator::SliceEnumerator;
use crate::output::warning::{Violation, Warning};
use crate::types::CandidateConfig;
use display_types::{ColorBitDepths, ColorCapabilities, VideoMode};

// ── stubs ─────────────────────────────────────────────────────────────────

/// Engine that accepts every candidate with no warnings.
struct AcceptAllEngine;

impl ConstraintEngine for AcceptAllEngine {
    type Warning = Warning;
    type Violation = Violation;

    fn check(
        &self,
        _: &SinkCapabilities,
        _: &SourceCapabilities,
        _: &CableCapabilities,
        _: &CandidateConfig<'_>,
    ) -> CheckResult<Warning, Violation> {
        Ok(crate::engine::Warnings::new())
    }
}

/// Engine that rejects every candidate with a fixed violation.
struct RejectAllEngine;

impl ConstraintEngine for RejectAllEngine {
    type Warning = Warning;
    type Violation = Violation;

    fn check(
        &self,
        _: &SinkCapabilities,
        _: &SourceCapabilities,
        _: &CableCapabilities,
        _: &CandidateConfig<'_>,
    ) -> CheckResult<Warning, Violation> {
        Err(crate::engine::Violations::new(
            Violation::ColorEncodingUnsupported,
        ))
    }
}

/// Ranker that reverses the accepted list, inverting whatever order the
/// default ranker would have produced.
struct ReverseRanker;

impl ConfigRanker for ReverseRanker {
    type Warning = Warning;

    fn rank(
        &self,
        mut configs: Vec<NegotiatedConfig<Warning>>,
        _: &NegotiationPolicy,
    ) -> Vec<NegotiatedConfig<Warning>> {
        configs.reverse();
        configs
    }
}

/// Constraint rule that always produces a violation, regardless of input.
struct AlwaysRejectRule;

impl crate::engine::rule::ConstraintRule<Violation> for AlwaysRejectRule {
    fn display_name(&self) -> &'static str {
        "always_reject"
    }

    fn warnings(
        &self,
        _: &crate::types::SinkCapabilities,
        _: &crate::types::SourceCapabilities,
        _: &crate::types::CableCapabilities,
        _: &crate::types::CandidateConfig<'_>,
        _: &mut dyn FnMut(crate::output::warning::Warning),
    ) {
    }

    fn check(
        &self,
        _: &SinkCapabilities,
        _: &SourceCapabilities,
        _: &CableCapabilities,
        _: &CandidateConfig<'_>,
    ) -> Option<Violation> {
        Some(Violation::ColorEncodingUnsupported)
    }
}

// ── helpers ───────────────────────────────────────────────────────────────

/// Sink with RGB 8 bpc and no declared modes (modes come from the enumerator).
fn rgb8_sink() -> SinkCapabilities {
    let mut caps = ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8;
    SinkCapabilities {
        color_capabilities: caps,
        ..Default::default()
    }
}

// ── tests ─────────────────────────────────────────────────────────────────

/// `with_enumerator` replaces the default enumerator; the custom enumerator's
/// mode list is used even when the sink has no declared modes.
#[test]
fn with_enumerator_overrides_sink_modes() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let sink = rgb8_sink(); // no supported_modes
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();

    let configs = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode]))
        .negotiate(&sink, &source, &cable);

    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].resolved.mode.width, 1920);
}

/// `with_engine` replaces the constraint check; an accept-all engine admits a
/// candidate that the default engine would reject (source TMDS ceiling too low).
#[test]
fn with_engine_accept_all_overrides_default_rejection() {
    // 1080p@60 RGB 8 bpc TMDS clock ≈ 136 MHz; source ceiling = 50 MHz → default
    // engine rejects it. AcceptAllEngine bypasses that check entirely.
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let sink = rgb8_sink();
    let mut source = SourceCapabilities::default();
    source.max_tmds_clock = 50_000; // 50 MHz — below 1080p@60 8 bpc
    let cable = CableCapabilities::unconstrained();

    let configs = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode]))
        .with_engine(AcceptAllEngine)
        .negotiate(&sink, &source, &cable);

    assert_eq!(
        configs.len(),
        1,
        "AcceptAllEngine must admit candidates the default engine would reject"
    );
}

/// `with_engine` replaces the constraint check; a reject-all engine empties
/// the result even for a configuration that the default engine would accept.
#[test]
fn with_engine_replaces_constraint_check() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();

    let configs = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode]))
        .with_engine(RejectAllEngine)
        .negotiate(&sink, &source, &cable);

    assert!(
        configs.is_empty(),
        "RejectAllEngine must eliminate all candidates"
    );
}

/// `with_ranker` replaces the ordering step; the output reflects the custom
/// ranker's order rather than the default policy.
#[test]
fn with_ranker_replaces_ordering() {
    // Enumerated in slice order: 4K first, then 1080p.
    // The default BEST_QUALITY ranker also puts 4K first (native resolution).
    // ReverseRanker inverts the accepted list, so 1080p appears first.
    let modes = [
        VideoMode::new(3840, 2160, 60u16, false),
        VideoMode::new(1920, 1080, 60u16, false),
    ];
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();

    let configs = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&modes))
        .with_ranker(ReverseRanker)
        .negotiate(&sink, &source, &cable);

    assert_eq!(configs.len(), 2);
    assert_eq!(
        configs[0].resolved.mode.width, 1920,
        "ReverseRanker must put 1080p first"
    );
    assert_eq!(configs[1].resolved.mode.width, 3840);
}

/// `with_extra_rule` appends a constraint on top of the default engine;
/// a rule that always rejects eliminates all candidates.
#[test]
fn with_extra_rule_applies_additional_constraint() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();

    let configs = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode]))
        .with_extra_rule(AlwaysRejectRule)
        .negotiate(&sink, &source, &cable);

    assert!(
        configs.is_empty(),
        "AlwaysRejectRule must eliminate all candidates"
    );
}

/// Rejections from `DefaultConstraintEngine` carry rule names via `TaggedViolation`.
#[test]
fn default_engine_rejections_carry_rule_name() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    // rgb8_sink has color caps so the enumerator generates candidates.
    // A cable with max_tmds_clock=1_000 (1 MHz) is below 1080p@60's ~150 MHz
    // TMDS requirement — TmdsClockCheck fires and candidates are rejected.
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities {
        max_tmds_clock: 1_000, // 1 MHz — intentionally far too low
        ..CableCapabilities::unconstrained()
    };

    let (_, rejected) = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode]))
        .negotiate_with_log(&sink, &source, &cable);

    assert!(!rejected.is_empty());
    // Every rejected entry must have at least one tagged violation with a non-empty rule name.
    for entry in &rejected {
        assert!(
            entry.violations.iter().any(|tv| !tv.rule.is_empty()),
            "rule name must be non-empty for DefaultConstraintEngine violations"
        );
    }
}

/// `negotiate_with_log` returns rejections alongside accepted configs.
#[test]
fn negotiate_with_log_captures_rejections() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let sink = rgb8_sink(); // only RGB 8 bpc
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();

    // SliceEnumerator × rgb8_sink will enumerate RGB 8 bpc (accepted) and other
    // encodings (rejected). Use RejectAllEngine to force every candidate into the log.
    let (accepted, rejected) = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode]))
        .with_engine(RejectAllEngine)
        .negotiate_with_log(&sink, &source, &cable);

    assert!(accepted.is_empty(), "RejectAllEngine must accept nothing");
    assert!(!rejected.is_empty(), "rejected log must be non-empty");
    // Every entry must carry at least one violation.
    for entry in &rejected {
        assert!(
            !entry.violations.is_empty(),
            "each rejection must carry at least one violation"
        );
        assert!(
            entry
                .violations
                .iter()
                .any(|v| matches!(v, Violation::ColorEncodingUnsupported)),
            "each rejection must carry the engine violation"
        );
    }
}

/// The FRL capacity check takes part in negotiation: 3840×2160 @ 60 Hz RGB 12-bit no
/// longer passes at FRL rates too slow to carry it; the slowest it is offered at is
/// 6 Gbps × 4 lanes.
#[test]
fn negotiation_rejects_frl_rates_too_slow_for_the_timing() {
    use crate::frl::FrlShortfall;
    use display_types::cea861::{HdmiForumFrl, HdmiForumSinkCap, vic_to_mode};
    use display_types::{ColorBitDepth, ColorFormat};

    let mode = vic_to_mode(97).unwrap();
    let mut sink = rgb8_sink();
    sink.color_capabilities.rgb444 = ColorBitDepths::BPC_8.with(ColorBitDepth::Depth12);
    sink.hdmi_forum = Some(HdmiForumSinkCap::new(
        1,
        600,
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        HdmiForumFrl::Rate12Gbps4Lanes,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        None,
        None,
        None,
    ));
    let source = SourceCapabilities::new(600_000, HdmiForumFrl::Rate12Gbps4Lanes, None);
    let cable = CableCapabilities::unconstrained();
    let (accepted, rejected) = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(core::slice::from_ref(&mode)))
        .negotiate_with_log(&sink, &source, &cable);

    let rgb12_rates: Vec<HdmiForumFrl> = accepted
        .iter()
        .filter(|c| {
            c.resolved.color_encoding == ColorFormat::Rgb444
                && c.resolved.bit_depth == ColorBitDepth::Depth12
        })
        .map(|c| c.resolved.frl_rate)
        .collect();
    assert!(!rgb12_rates.is_empty());
    assert!(!rgb12_rates.contains(&HdmiForumFrl::Rate3Gbps3Lanes));
    assert!(!rgb12_rates.contains(&HdmiForumFrl::Rate6Gbps3Lanes));
    assert!(rgb12_rates.contains(&HdmiForumFrl::Rate6Gbps4Lanes));

    // The rejected candidates say why.
    let capacity_rejections = rejected
        .iter()
        .flat_map(|r| r.violations.iter())
        .filter(|v| {
            v.rule == "frl_capacity"
                && matches!(
                    v.violation,
                    Violation::FrlCapacityExceeded {
                        shortfall: FrlShortfall::Borrow | FrlShortfall::BorrowLimit { .. },
                        ..
                    }
                )
        })
        .count();
    assert!(capacity_rejections >= 2);
}

/// `negotiate` (without log) produces the same accepted set as `negotiate_with_log`.
#[test]
fn negotiate_and_negotiate_with_log_agree_on_accepted() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();

    let accepted_plain = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode.clone()]))
        .negotiate(&sink, &source, &cable);

    let (accepted_log, _) = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode]))
        .negotiate_with_log(&sink, &source, &cable);

    assert_eq!(
        accepted_plain.len(),
        accepted_log.len(),
        "both methods must agree on the accepted count"
    );
}

/// `negotiate` does not allocate a rejection log (smoke: just checks it compiles
/// and runs without panicking).
#[test]
fn negotiate_without_log_does_not_panic() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();
    let _ = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode]))
        .with_engine(RejectAllEngine)
        .negotiate(&sink, &source, &cable);
}

/// The pipeline deduplicates candidates that are identical across all five
/// key fields; supplying the same mode twice yields only one accepted config.
/// `with_extra_rule` serves an engine with its own warning type: the extra rule is
/// written for that type, and the result is still an engine.
#[test]
fn with_extra_rule_accepts_a_rule_for_a_custom_warning_type() {
    use crate::engine::rule::ConstraintRule;
    use crate::engine::{Violations, Warnings};

    #[derive(Debug)]
    struct MyWarning;
    impl core::fmt::Display for MyWarning {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("mine")
        }
    }
    struct MyEngine;
    impl ConstraintEngine for MyEngine {
        type Warning = MyWarning;
        type Violation = TaggedViolation<Violation>;
        fn check(
            &self,
            _: &SinkCapabilities,
            _: &SourceCapabilities,
            _: &CableCapabilities,
            _: &CandidateConfig<'_>,
        ) -> CheckResult<MyWarning, TaggedViolation<Violation>> {
            Ok(Warnings::new())
        }
    }
    struct MyRule;
    impl ConstraintRule<Violation, MyWarning> for MyRule {
        fn display_name(&self) -> &'static str {
            "my_rule"
        }
        fn check(
            &self,
            _: &SinkCapabilities,
            _: &SourceCapabilities,
            _: &CableCapabilities,
            _: &CandidateConfig<'_>,
        ) -> Option<Violation> {
            None
        }
        fn warnings(
            &self,
            _: &SinkCapabilities,
            _: &SourceCapabilities,
            _: &CableCapabilities,
            _: &CandidateConfig<'_>,
            warn: &mut dyn FnMut(MyWarning),
        ) {
            warn(MyWarning);
        }
    }
    let _ = Violations::new(0u8); // the result types are public API
    let builder = NegotiatorBuilder::default()
        .with_engine(MyEngine)
        .with_extra_rule(MyRule);
    let mode = VideoMode::new(1920, 1080, 60u32, false);
    let config = CandidateConfig::new(
        &mode,
        display_types::ColorFormat::Rgb444,
        display_types::ColorBitDepth::Depth8,
        display_types::cea861::HdmiForumFrl::NotSupported,
        false,
    );
    let warnings = builder
        .engine
        .check(
            &rgb8_sink(),
            &SourceCapabilities::default(),
            &CableCapabilities::unconstrained(),
            &config,
        )
        .unwrap();
    assert_eq!(warnings.len(), 1);
}

#[test]
fn negotiate_dedup_removes_identical_candidates() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();

    let configs = NegotiatorBuilder::default()
        .with_enumerator(SliceEnumerator::new(&[mode.clone(), mode]))
        .negotiate(&sink, &source, &cable);

    assert_eq!(
        configs.len(),
        1,
        "identical candidates must be deduplicated"
    );
}
