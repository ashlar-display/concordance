//! Pipeline builder for wiring together the three negotiation components.

use alloc::vec::Vec;

use display_types::ResolvedDisplayConfig;

use crate::engine::rule::{ConstraintRule, Layered, TaggingAdapter};
use crate::engine::{ConstraintEngine, DefaultConstraintEngine};
use crate::enumerator::{CandidateEnumerator, DefaultEnumerator};
use crate::output::config::NegotiatedConfig;
use crate::output::rejection::RejectedConfig;
use crate::output::trace::ReasoningTrace;
use crate::output::warning::TaggedViolation;

/// Return type of [`NegotiatorBuilder::negotiate_with_log`].
pub type NegotiationLog<W, V> = (Vec<NegotiatedConfig<W>>, Vec<RejectedConfig<V>>);
use crate::ranker::policy::NegotiationPolicy;
use crate::ranker::{ConfigRanker, DefaultRanker};
use crate::types::{CableCapabilities, SinkCapabilities, SourceCapabilities};

/// Wires the three pipeline components together and drives the negotiation run.
///
/// Default implementations are used for any slot not explicitly configured.
/// Callers can substitute any component without forking the crate.
///
/// # Example
///
/// ```
/// # use concordance::{NegotiatorBuilder, SinkCapabilities, SourceCapabilities, CableCapabilities};
/// # let sink = SinkCapabilities::default();
/// # let source = SourceCapabilities::default();
/// # let cable = CableCapabilities::default();
/// let configs = NegotiatorBuilder::default()
///     .negotiate(&sink, &source, &cable);
/// ```
pub struct NegotiatorBuilder<E = DefaultConstraintEngine, En = DefaultEnumerator, R = DefaultRanker>
{
    engine: E,
    enumerator: En,
    ranker: R,
    policy: NegotiationPolicy,
}

impl Default for NegotiatorBuilder {
    fn default() -> Self {
        Self {
            engine: DefaultConstraintEngine::default(),
            enumerator: DefaultEnumerator,
            ranker: DefaultRanker,
            policy: NegotiationPolicy::default(),
        }
    }
}

impl<E, En, R> NegotiatorBuilder<E, En, R> {
    /// Overrides the constraint engine.
    pub fn with_engine<E2: ConstraintEngine>(self, engine: E2) -> NegotiatorBuilder<E2, En, R> {
        NegotiatorBuilder {
            engine,
            enumerator: self.enumerator,
            ranker: self.ranker,
            policy: self.policy,
        }
    }

    /// Overrides the candidate enumerator.
    pub fn with_enumerator<En2: CandidateEnumerator>(
        self,
        enumerator: En2,
    ) -> NegotiatorBuilder<E, En2, R> {
        NegotiatorBuilder {
            engine: self.engine,
            enumerator,
            ranker: self.ranker,
            policy: self.policy,
        }
    }

    /// Overrides the configuration ranker.
    pub fn with_ranker<R2: ConfigRanker>(self, ranker: R2) -> NegotiatorBuilder<E, En, R2> {
        NegotiatorBuilder {
            engine: self.engine,
            enumerator: self.enumerator,
            ranker,
            policy: self.policy,
        }
    }

    /// Appends an extra constraint rule to the engine without replacing it.
    ///
    /// The rule returns `Option<V>` (the inner violation type); it is automatically
    /// wrapped in [`TaggedViolation`] with the rule's
    /// [`display_name`][crate::engine::rule::ConstraintRule::display_name] when a
    /// violation is emitted.
    ///
    /// The rule is evaluated after all built-in checks. In alloc mode,
    /// violations from both the base engine and the extra rule are collected;
    /// in no-alloc mode the engine short-circuits on the first failure, so
    /// the extra rule is only reached if all built-in checks pass.
    pub fn with_extra_rule<X, InnerV>(
        self,
        rule: X,
    ) -> NegotiatorBuilder<Layered<E, TaggingAdapter<X>>, En, R>
    where
        E: ConstraintEngine<Violation = TaggedViolation<InnerV>>,
        X: ConstraintRule<InnerV, E::Warning>,
        InnerV: crate::diagnostic::Diagnostic + 'static,
    {
        NegotiatorBuilder {
            engine: Layered::new(self.engine, TaggingAdapter(rule)),
            enumerator: self.enumerator,
            ranker: self.ranker,
            policy: self.policy,
        }
    }

    /// Overrides the negotiation policy.
    pub fn with_policy(mut self, policy: NegotiationPolicy) -> Self {
        self.policy = policy;
        self
    }
}

impl<E, En, R> NegotiatorBuilder<E, En, R>
where
    E: ConstraintEngine,
    En: CandidateEnumerator,
    R: ConfigRanker<Warning = E::Warning>,
{
    /// Runs the negotiation pipeline and returns a ranked list of viable configurations.
    ///
    /// Candidates are enumerated, validated by the constraint engine, deduplicated,
    /// and ranked according to the policy.
    ///
    /// To also capture why each rejected candidate failed, use
    /// [`negotiate_with_log`][Self::negotiate_with_log] instead.
    pub fn negotiate(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
    ) -> Vec<NegotiatedConfig<E::Warning>> {
        let (accepted, _) = self.negotiate_inner(sink, source, cable, false);
        accepted
    }

    /// Runs the negotiation pipeline and returns both the accepted configurations and a
    /// per-candidate rejection log.
    ///
    /// The rejection log contains one [`RejectedConfig`] entry for every candidate that
    /// failed the constraint engine, in enumeration order. Each entry records the same
    /// five fields as [`CandidateConfig`][crate::CandidateConfig] plus the violations that
    /// caused the rejection.
    ///
    /// Use this in diagnostic tools or test harnesses where you need to explain why a
    /// specific mode was excluded. The allocation cost is non-trivial on large mode lists;
    /// prefer [`negotiate`][Self::negotiate] when the rejection detail is not needed.
    pub fn negotiate_with_log(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
    ) -> NegotiationLog<E::Warning, E::Violation> {
        self.negotiate_inner(sink, source, cable, true)
    }

    fn negotiate_inner(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        collect_rejections: bool,
    ) -> NegotiationLog<E::Warning, E::Violation> {
        let mut accepted: Vec<NegotiatedConfig<E::Warning>> = Vec::new();
        let mut rejected: Vec<RejectedConfig<E::Violation>> = Vec::new();

        for config in self.enumerator.enumerate(sink, source, cable) {
            match self.engine.check(sink, source, cable, &config) {
                Ok(warnings) => {
                    let negotiated = NegotiatedConfig {
                        resolved: ResolvedDisplayConfig::new(
                            config.mode.clone(),
                            config.color_encoding,
                            config.bit_depth,
                            config.frl_rate,
                            config.dsc_enabled,
                            false,
                        ),
                        warnings: warnings.into_vec(),
                        trace: ReasoningTrace::new(),
                    };

                    // O(n²) dedup — candidate lists are small enough that this is acceptable.
                    let is_dup = accepted.iter().any(|c| c.resolved == negotiated.resolved);
                    if !is_dup {
                        accepted.push(negotiated);
                    }
                }
                Err(violations) => {
                    if collect_rejections {
                        rejected.push(RejectedConfig {
                            mode: config.mode.clone(),
                            color_encoding: config.color_encoding,
                            bit_depth: config.bit_depth,
                            frl_rate: config.frl_rate,
                            dsc_enabled: config.dsc_enabled,
                            violations: violations.into_vec(),
                        });
                    }
                }
            }
        }

        (self.ranker.rank(accepted, &self.policy), rejected)
    }
}

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
#[path = "builder_tests.rs"]
mod tests;
