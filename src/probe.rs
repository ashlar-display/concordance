//! The `is_config_viable` binary probe function.

use crate::engine::ConstraintEngine;
use crate::engine::DefaultConstraintEngine;
use crate::output::warning::{TaggedViolation, Violation, Warning};
use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};

/// Determines whether a specific configuration is viable for the given capabilities.
///
/// Returns structured violations rather than a boolean, giving the caller enough
/// information to surface specific rejection reasons.
///
/// This is the `no_std`-compatible binary probe. Firmware and embedded consumers
/// that cannot afford allocation or iteration use this function directly. The ranked
/// iterator is built on top of this primitive.
///
/// Returns the [`Warnings`][crate::engine::Warnings] of an accepted configuration or the
/// [`Violations`][crate::engine::Violations] of a rejected one. With alloc they hold every
/// warning and violation; without it, up to [`crate::engine::MAX_WARNINGS`] warnings (the
/// rest counted in `dropped()`) and the first violation, the pass stopping there
/// (`is_complete()` is `false`).
///
/// Callers without cable information may pass [`CableCapabilities::unconstrained()`]
/// to recover the previous optimistic behavior.
pub fn is_config_viable(
    sink: &SinkCapabilities,
    source: &SourceCapabilities,
    cable: &CableCapabilities,
    config: &CandidateConfig<'_>,
) -> crate::engine::CheckResult<Warning, TaggedViolation<Violation>> {
    DefaultConstraintEngine::default().check(sink, source, cable, config)
}

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
#[path = "probe_tests.rs"]
mod tests;
