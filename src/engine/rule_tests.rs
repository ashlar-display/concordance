use super::*;
use crate::diagnostic::Diagnostic;
use crate::engine::{ConstraintEngine, DefaultConstraintEngine, Violations};
use crate::output::warning::Violation;
use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};
use alloc::vec::Vec;
use display_types::cea861::HdmiForumFrl;
use display_types::{ColorBitDepth, ColorFormat, VideoMode};

fn mode() -> VideoMode {
    VideoMode::new(1920, 1080, 60u16, false)
}

fn config(mode: &VideoMode) -> CandidateConfig<'_> {
    CandidateConfig {
        mode,
        color_encoding: ColorFormat::Rgb444,
        bit_depth: ColorBitDepth::Depth8,
        frl_rate: HdmiForumFrl::NotSupported,
        dsc_enabled: false,
    }
}

fn sink() -> SinkCapabilities {
    SinkCapabilities::default()
}
fn source() -> SourceCapabilities {
    SourceCapabilities::default()
}
fn cable() -> CableCapabilities {
    CableCapabilities::default()
}

struct AlwaysPass;
struct FailEncoding;
struct FailDepth;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for AlwaysPass {
    fn display_name(&self) -> &'static str {
        "always_pass"
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
    ) -> Option<V> {
        None
    }
}

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for FailEncoding {
    fn display_name(&self) -> &'static str {
        "fail_encoding"
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
    ) -> Option<V> {
        Some(Violation::ColorEncodingUnsupported.into())
    }
}

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for FailDepth {
    fn display_name(&self) -> &'static str {
        "fail_depth"
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
    ) -> Option<V> {
        Some(Violation::BitDepthUnsupported.into())
    }
}

// --- TaggingAdapter ---

#[test]
fn tagging_adapter_display_name_delegates_to_inner() {
    let adapter = TaggingAdapter(AlwaysPass);
    assert_eq!(
        ConstraintRule::<TaggedViolation<Violation>>::display_name(&adapter),
        "always_pass"
    );
    let adapter = TaggingAdapter(FailEncoding);
    assert_eq!(
        ConstraintRule::<TaggedViolation<Violation>>::display_name(&adapter),
        "fail_encoding"
    );
}

// --- Layered<R1, R2> as ConstraintRule ---

#[test]
fn layered_rule_display_name_is_layered() {
    let rule: Layered<AlwaysPass, AlwaysPass> = Layered::new(AlwaysPass, AlwaysPass);
    assert_eq!(ConstraintRule::<Violation>::display_name(&rule), "layered");
}

#[test]
fn layered_rule_both_pass() {
    let m = mode();
    let rule = Layered::new(AlwaysPass, AlwaysPass);
    let result =
        ConstraintRule::<Violation>::check(&rule, &sink(), &source(), &cable(), &config(&m));
    assert!(result.is_none());
}

#[test]
fn layered_rule_base_fails_short_circuits() {
    let m = mode();
    // Base returns ColorEncodingUnsupported; extra would return BitDepthUnsupported.
    // Only the base violation should be present since or_else short-circuits.
    let rule = Layered::new(FailEncoding, FailDepth);
    let result =
        ConstraintRule::<Violation>::check(&rule, &sink(), &source(), &cable(), &config(&m));
    assert!(matches!(result, Some(Violation::ColorEncodingUnsupported)));
}

#[test]
fn layered_rule_base_passes_extra_fails() {
    let m = mode();
    let rule = Layered::new(AlwaysPass, FailDepth);
    let result =
        ConstraintRule::<Violation>::check(&rule, &sink(), &source(), &cable(), &config(&m));
    assert!(matches!(result, Some(Violation::BitDepthUnsupported)));
}

// --- Layered<E, R> as ConstraintEngine ---

fn empty_engine() -> DefaultConstraintEngine<Violation> {
    DefaultConstraintEngine::with_checks(&[])
}

#[test]
fn layered_engine_both_pass() {
    let m = mode();
    let engine = Layered::new(empty_engine(), TaggingAdapter(AlwaysPass));
    assert!(
        engine
            .check(&sink(), &source(), &cable(), &config(&m))
            .is_ok()
    );
}

#[test]
fn layered_engine_base_passes_extra_fails() {
    let m = mode();
    let engine = Layered::new(empty_engine(), TaggingAdapter(FailDepth));
    let violations = engine
        .check(&sink(), &source(), &cable(), &config(&m))
        .unwrap_err();
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.violation, Violation::BitDepthUnsupported))
    );
}

#[test]
fn layered_engine_base_fails_extra_violation_also_collected() {
    let m = mode();
    // Base engine with ColorEncodingCheck; default sink → no color caps → fires.
    static BASE_RULES: &[&(dyn ConstraintRule<Violation> + Sync)] =
        &[&crate::engine::checks::ColorEncodingCheck];
    let engine = Layered::new(
        DefaultConstraintEngine::with_checks(BASE_RULES),
        TaggingAdapter(FailDepth),
    );
    let violations = engine
        .check(&sink(), &source(), &cable(), &config(&m))
        .unwrap_err();
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.violation, Violation::ColorEncodingUnsupported))
    );
    assert!(
        violations
            .iter()
            .any(|v| matches!(v.violation, Violation::BitDepthUnsupported))
    );
}

#[test]
fn layered_engine_base_fails_extra_passes_only_base_violations() {
    let m = mode();
    static BASE_RULES: &[&(dyn ConstraintRule<Violation> + Sync)] =
        &[&crate::engine::checks::ColorEncodingCheck];
    let engine = Layered::new(
        DefaultConstraintEngine::with_checks(BASE_RULES),
        TaggingAdapter(AlwaysPass),
    );
    let violations = engine
        .check(&sink(), &source(), &cable(), &config(&m))
        .unwrap_err();
    assert_eq!(violations.len(), 1);
    assert!(matches!(
        violations.first().violation,
        Violation::ColorEncodingUnsupported
    ));
}

#[test]
fn layered_engine_base_stopped_early_skips_the_extra_rule() {
    // A base that stopped at its first violation (as the default engine does without
    // alloc): the extra rule is not run, so the result stays marked incomplete.
    struct StopsEarly;
    impl ConstraintEngine for StopsEarly {
        type Warning = Warning;
        type Violation = TaggedViolation<Violation>;
        fn check(
            &self,
            _: &SinkCapabilities,
            _: &SourceCapabilities,
            _: &CableCapabilities,
            _: &CandidateConfig<'_>,
        ) -> CheckResult<Self::Warning, Self::Violation> {
            Err(Violations::new(TaggedViolation {
                rule: "stops_early",
                violation: Violation::ColorEncodingUnsupported,
            })
            .stopped_early())
        }
    }
    let m = mode();
    let engine = Layered::new(StopsEarly, TaggingAdapter(FailDepth));
    let violations = engine
        .check(&sink(), &source(), &cable(), &config(&m))
        .unwrap_err();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations.first().rule, "stops_early");
    assert!(!violations.is_complete());
}

// --- warnings ---

/// Passes, and warns once with the built-in type.
struct WarnOnce;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for WarnOnce {
    fn display_name(&self) -> &'static str {
        "warn_once"
    }
    fn check(
        &self,
        _: &SinkCapabilities,
        _: &SourceCapabilities,
        _: &CableCapabilities,
        _: &CandidateConfig<'_>,
    ) -> Option<V> {
        None
    }
    fn warnings(
        &self,
        _: &SinkCapabilities,
        _: &SourceCapabilities,
        _: &CableCapabilities,
        _: &CandidateConfig<'_>,
        warn: &mut dyn FnMut(Warning),
    ) {
        warn(Warning::CableBandwidthMarginal);
    }
}

fn collect<R: ConstraintRule<V, W>, V: Diagnostic, W: Diagnostic>(rule: &R) -> Vec<W> {
    let m = mode();
    let mut out = Vec::new();
    rule.warnings(&sink(), &source(), &cable(), &config(&m), &mut |w| {
        out.push(w)
    });
    out
}

#[test]
fn rules_warn_nothing_by_default() {
    assert!(collect::<_, Violation, Warning>(&AlwaysPass).is_empty());
}

#[test]
fn tagging_adapter_forwards_warnings() {
    let w = collect::<_, TaggedViolation<Violation>, Warning>(&TaggingAdapter(WarnOnce));
    assert!(matches!(w[..], [Warning::CableBandwidthMarginal]));
}

#[test]
fn layered_rule_forwards_both_warnings() {
    let w = collect::<_, Violation, Warning>(&Layered::new(WarnOnce, WarnOnce));
    assert_eq!(w.len(), 2);
}

#[test]
fn layered_engine_adds_the_extra_rules_warnings() {
    // What `NegotiatorBuilder::with_extra_rule` builds: the default engine plus a
    // tagged extra rule. The engine accepts, so the extra rule's warning comes through.
    static NONE: CheckList<Violation> = &[];
    let engine = Layered::new(
        DefaultConstraintEngine::with_checks(NONE),
        TaggingAdapter(WarnOnce),
    );
    let m = mode();
    let w = engine
        .check(&sink(), &source(), &cable(), &config(&m))
        .unwrap()
        .into_vec();
    assert!(matches!(w[..], [Warning::CableBandwidthMarginal]));
}

/// A custom warning type: rules for a custom engine keep full type fidelity.
#[derive(Debug, PartialEq)]
struct MyWarning;

impl core::fmt::Display for MyWarning {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("my warning")
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

#[test]
fn custom_warning_types_compose() {
    let w = collect::<_, Violation, MyWarning>(&Layered::new(MyRule, MyRule));
    assert_eq!(w, [MyWarning, MyWarning]);
}
