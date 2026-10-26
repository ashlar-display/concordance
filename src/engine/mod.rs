//! Constraint engine trait and default implementation.

/// Built-in [`ConstraintRule`] implementations and the [`checks::DEFAULT_CHECKS`] list.
pub mod checks;
pub mod rule;

use core::fmt;

use crate::diagnostic::Diagnostic;
use crate::output::warning::{TaggedViolation, Violation};
use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};

pub use rule::{CheckList, ConstraintRule};

pub mod result;
pub use result::{Violations, Warnings};

/// Maximum number of warnings a [`Warnings`] stores in a single constraint check on
/// no-alloc targets.
///
/// Further warnings are not stored; [`Warnings::dropped`] counts them.
pub const MAX_WARNINGS: usize = 8;

/// The result of a constraint check: the [`Warnings`] of an accepted candidate, or the
/// [`Violations`] of a rejected one. The same type in every build; see [`result`] for what
/// each holds with and without alloc.
pub type CheckResult<W, V> = Result<Warnings<W>, Violations<V>>;

/// Determines whether a given configuration is valid for the supplied capabilities.
///
/// Returns structured violations rather than a boolean, so callers can surface
/// specific rejection reasons. The default implementation enforces HDMI specification
/// rules; callers can wrap or replace it to add vendor-specific constraint rules.
///
/// See [`ConstraintRule`] and [`rule::Layered`] for rule injection without replacing
/// the entire engine.
pub trait ConstraintEngine {
    /// Non-fatal diagnostic type emitted for accepted configurations.
    type Warning: Diagnostic;

    /// Fatal constraint violation type emitted for rejected configurations.
    type Violation: Diagnostic;

    /// Evaluates a candidate configuration against the supplied capabilities.
    ///
    /// On alloc targets, returns all accumulated warnings on success and all
    /// violations on failure. On no-alloc targets, returns up to
    /// [`MAX_WARNINGS`] warnings on success and the first violation on failure.
    fn check(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> CheckResult<Self::Warning, Self::Violation>;
}

/// Default HDMI specification constraint engine.
///
/// Generic over the violation type `V`, which defaults to the built-in [`Violation`]
/// enum. Callers that need a richer violation hierarchy can define their own type
/// and use it here, as long as it implements `From<`[`Violation`]`>`:
///
/// ```
/// # use concordance::engine::{DefaultConstraintEngine, rule::{CheckList, ConstraintRule}};
/// # use concordance::output::warning::Violation;
/// # use concordance::types::{CandidateConfig, SinkCapabilities, SourceCapabilities, CableCapabilities};
/// # use core::fmt;
/// #[derive(Debug)]
/// enum MyViolation {
///     Builtin(Violation),
///     HdrCertificationFailed,
/// }
/// # impl fmt::Display for MyViolation {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "violation") }
/// # }
/// impl From<Violation> for MyViolation {
///     fn from(v: Violation) -> Self { MyViolation::Builtin(v) }
/// }
/// # struct MyHdrCheck;
/// # impl ConstraintRule<MyViolation> for MyHdrCheck {
/// #     fn display_name(&self) -> &'static str { "my_hdr_check" }
/// #     fn check(&self, _: &SinkCapabilities, _: &SourceCapabilities,
/// #              _: &CableCapabilities, _: &CandidateConfig<'_>) -> Option<MyViolation> { None }
/// #     fn warnings(&self, _: &SinkCapabilities, _: &SourceCapabilities, _: &CableCapabilities,
/// #                 _: &CandidateConfig<'_>, _: &mut dyn FnMut(concordance::Warning)) {}
/// # }
/// static MY_CHECKS: CheckList<MyViolation> = &[&MyHdrCheck];
///
/// let _engine = DefaultConstraintEngine::<MyViolation>::with_checks(MY_CHECKS);
/// ```
///
/// For the common case — built-in violations only — `DefaultConstraintEngine::default()`
/// uses [`checks::DEFAULT_CHECKS`] and no type annotation is needed.
pub struct DefaultConstraintEngine<V: 'static = Violation> {
    checks: CheckList<V>,
}

impl<V: 'static> Clone for DefaultConstraintEngine<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V: 'static> Copy for DefaultConstraintEngine<V> {}

/// Uses [`checks::DEFAULT_CHECKS`] as the rule list. Only available for `V = Violation`
/// since `DEFAULT_CHECKS` is typed for the built-in violation set.
impl Default for DefaultConstraintEngine<Violation> {
    fn default() -> Self {
        Self {
            checks: checks::DEFAULT_CHECKS,
        }
    }
}

impl<V: Diagnostic> DefaultConstraintEngine<V> {
    /// Constructs a `DefaultConstraintEngine` with a custom check list.
    ///
    /// The slice must be `'static` to support no-alloc targets. Check sets are
    /// always compile-time concerns; use a `static` binding:
    ///
    /// ```
    /// use concordance::engine::checks::{FrlCeilingCheck, TmdsClockCheck};
    /// use concordance::engine::rule::CheckList;
    /// use concordance::engine::DefaultConstraintEngine;
    /// use concordance::output::warning::Violation;
    ///
    /// static MY_CHECKS: CheckList<Violation> = &[&FrlCeilingCheck, &TmdsClockCheck];
    ///
    /// let engine = DefaultConstraintEngine::with_checks(MY_CHECKS);
    /// ```
    pub fn with_checks(checks: CheckList<V>) -> Self {
        Self { checks }
    }
}

/// Formats the engine as an ordered list of rule display names.
impl<V: Diagnostic> fmt::Debug for DefaultConstraintEngine<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for rule in self.checks {
            list.entry(&rule.display_name());
        }
        list.finish()
    }
}

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
mod tests {
    use super::*;
    use crate::output::warning::Violation;

    /// `Clone::clone` must compile and produce an engine that behaves identically
    /// to the original. Calling it exercises the hand-written `Clone` impl that
    /// delegates to the derived `Copy`.
    #[test]
    fn clone_produces_equivalent_engine() {
        let original = DefaultConstraintEngine::default();
        let cloned = original.clone();
        // Both should format identically — same check list, same display names.
        assert_eq!(alloc::format!("{original:?}"), alloc::format!("{cloned:?}"));
    }

    /// The `Debug` impl formats the engine as an ordered list of rule display names.
    /// The output must be non-empty and contain at least one known built-in rule name.
    #[test]
    fn debug_lists_rule_names() {
        let engine = DefaultConstraintEngine::<Violation>::default();
        let output = alloc::format!("{engine:?}");
        assert!(!output.is_empty());
        // Spot-check one well-known built-in rule name.
        assert!(
            output.contains("frl_ceiling"),
            "expected 'frl_ceiling' in debug output, got: {output}"
        );
    }
}

impl<V: Diagnostic> ConstraintEngine for DefaultConstraintEngine<V> {
    type Warning = crate::output::warning::Warning;
    type Violation = TaggedViolation<V>;

    fn check(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> CheckResult<Self::Warning, Self::Violation> {
        // With alloc every rule runs and every violation is kept; without it the pass
        // stops at the first violation and says so.
        let mut violations: Option<Violations<Self::Violation>> = None;
        for rule in self.checks {
            if let Some(violation) = rule.check(sink, source, cable, config) {
                let tagged = TaggedViolation {
                    rule: rule.display_name(),
                    violation,
                };
                match &mut violations {
                    Some(v) => v.push(tagged),
                    None => violations = Some(Violations::new(tagged)),
                }
                #[cfg(not(any(feature = "alloc", feature = "std")))]
                return Err(violations.take().expect("just set").stopped_early());
            }
        }
        if let Some(v) = violations {
            return Err(v);
        }
        let mut warnings = Warnings::new();
        for rule in self.checks {
            rule.warnings(sink, source, cable, config, &mut |w| warnings.push(w));
        }
        Ok(warnings)
    }
}
