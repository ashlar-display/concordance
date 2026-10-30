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
#[path = "rule_tests.rs"]
mod tests;
