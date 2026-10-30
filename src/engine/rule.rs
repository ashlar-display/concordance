//! The `ConstraintRule` trait and `Layered` combinator.

use crate::diagnostic::Diagnostic;
use crate::engine::CheckResult;
use crate::output::warning::{TaggedViolation, Warning};
use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};

/// A static slice of constraint rules, used as the check list for [`DefaultConstraintEngine`][crate::engine::DefaultConstraintEngine].
///
/// The `'static` bound is required to support no-alloc targets, where the slice
/// must live for the duration of the program. Check sets are always compile-time
/// concerns; declare yours as a `static` item.
pub type CheckList<V> = &'static [&'static (dyn ConstraintRule<V> + Sync)];

/// A single constraint check — the unit of extensibility for the constraint engine.
///
/// Unlike [`ConstraintEngine`][crate::engine::ConstraintEngine], which coordinates a
/// full check pass and may produce both warnings and violations, a `ConstraintRule`
/// evaluates a single constraint and either finds a violation or does not.
///
/// The return type is always `Option<V>` — no alloc/no-alloc split. The split lives
/// at the `ConstraintEngine` level, which decides whether to collect all violations
/// or short-circuit on the first.
///
/// Custom rules sharing the built-in violation type can be injected via
/// [`NegotiatorBuilder::with_extra_rule`][crate::NegotiatorBuilder::with_extra_rule]
/// without replacing or reimplementing the default engine.
///
/// A rule may also report non-fatal findings on candidates it accepts, through
/// [`warnings`][Self::warnings]. Their type `W` defaults to the built-in [`Warning`];
/// a rule for a custom engine can use that engine's warning type instead.
pub trait ConstraintRule<V: Diagnostic, W: Diagnostic = Warning> {
    /// A short, stable identifier for this rule.
    ///
    /// Used in [`ReasoningTrace`][crate::output::trace::ReasoningTrace] entries to
    /// identify which rule rejected a candidate. Should be a lowercase snake_case
    /// string (e.g. `"frl_ceiling"`).
    fn display_name(&self) -> &'static str;

    /// Evaluates this rule against the supplied capabilities.
    ///
    /// Returns `Some(violation)` if the candidate fails this constraint, `None` if
    /// it passes.
    fn check(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V>;

    /// Reports non-fatal findings for a candidate, through `warn`.
    ///
    /// The engine calls it only for candidates no rule rejected. Required, so that a rule
    /// states it has nothing to report (an empty body) and a wrapper cannot forget to
    /// forward the rule it wraps.
    fn warnings(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
        warn: &mut dyn FnMut(W),
    );
}

/// Wraps a [`ConstraintRule<V>`] so it produces a [`ConstraintRule<TaggedViolation<V>>`],
/// tagging each emitted violation with the rule's [`display_name`][ConstraintRule::display_name].
///
/// [`NegotiatorBuilder::with_extra_rule`][crate::NegotiatorBuilder::with_extra_rule]
/// applies this adapter automatically. Use it directly only if you are constructing
/// a [`Layered`] engine by hand.
pub struct TaggingAdapter<R>(pub R);

impl<R, V, W> ConstraintRule<TaggedViolation<V>, W> for TaggingAdapter<R>
where
    R: ConstraintRule<V, W>,
    V: Diagnostic,
    W: Diagnostic,
{
    fn display_name(&self) -> &'static str {
        self.0.display_name()
    }

    fn check(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<TaggedViolation<V>> {
        self.0
            .check(sink, source, cable, config)
            .map(|violation| TaggedViolation {
                rule: self.0.display_name(),
                violation,
            })
    }

    fn warnings(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
        warn: &mut dyn FnMut(W),
    ) {
        self.0.warnings(sink, source, cable, config, warn);
    }
}

/// Chains two rules (or a base engine and an extra rule) in sequence.
///
/// Two usage patterns:
///
/// - **`Layered<R1, R2>` where both implement `ConstraintRule<V>`** — composes two
///   rules. Both are evaluated; the first violation found is returned.
///
/// - **`Layered<E, R>` where `E: ConstraintEngine` and `R: ConstraintRule<E::Violation>`**
///   — extends an engine with an additional rule. Implements `ConstraintEngine`,
///   running the base engine then the extra rule.
///
/// Construct via [`NegotiatorBuilder::with_extra_rule`][crate::NegotiatorBuilder::with_extra_rule]
/// rather than directly.
pub struct Layered<Base, Extra> {
    /// The base engine or rule.
    pub base: Base,
    /// The additional rule applied after the base.
    pub extra: Extra,
}

impl<Base, Extra> Layered<Base, Extra> {
    /// Constructs a `Layered` combinator from a base and an extra component.
    pub fn new(base: Base, extra: Extra) -> Self {
        Self { base, extra }
    }
}

/// `Layered<R1, R2>` is itself a `ConstraintRule<V, W>` when both components are.
///
/// Evaluates the base rule first; if it passes, evaluates the extra rule. Returns
/// the first violation found, or `None` if both pass. Warnings come from both.
impl<R1, R2, V, W> ConstraintRule<V, W> for Layered<R1, R2>
where
    R1: ConstraintRule<V, W>,
    R2: ConstraintRule<V, W>,
    V: Diagnostic,
    W: Diagnostic,
{
    fn display_name(&self) -> &'static str {
        "layered"
    }

    fn check(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V> {
        self.base
            .check(sink, source, cable, config)
            .or_else(|| self.extra.check(sink, source, cable, config))
    }

    fn warnings(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
        warn: &mut dyn FnMut(W),
    ) {
        self.base.warnings(sink, source, cable, config, warn);
        self.extra.warnings(sink, source, cable, config, warn);
    }
}

/// `Layered<E, R>` is a `ConstraintEngine` when `E` is an engine and `R` is a rule
/// that produces the same violation type.
///
/// Runs the base engine first, then the extra rule. With alloc, violations from
/// both are collected. Without alloc, the base engine short-circuits on its first
/// violation before the extra rule is reached. When both accept, the base engine's
/// warnings come first, then the extra rule's; without alloc, up to
/// [`MAX_WARNINGS`][super::MAX_WARNINGS] in all.
impl<E, R> super::ConstraintEngine for Layered<E, R>
where
    E: super::ConstraintEngine,
    R: ConstraintRule<E::Violation, E::Warning>,
{
    type Warning = E::Warning;
    type Violation = E::Violation;

    fn check(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> CheckResult<Self::Warning, Self::Violation> {
        match self.base.check(sink, source, cable, config) {
            // A complete base pass collects the extra rule's violation too; one that
            // stopped early (no alloc) is passed on as it is.
            Err(mut violations) => {
                if violations.is_complete() {
                    if let Some(v) = self.extra.check(sink, source, cable, config) {
                        violations.push(v);
                    }
                }
                Err(violations)
            }
            Ok(mut warnings) => {
                if let Some(v) = self.extra.check(sink, source, cable, config) {
                    Err(super::Violations::new(v))
                } else {
                    self.extra
                        .warnings(sink, source, cable, config, &mut |w| warnings.push(w));
                    Ok(warnings)
                }
            }
        }
    }
}
#[cfg(all(test, any(feature = "alloc", feature = "std")))]
mod tests {
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
}
