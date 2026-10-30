use crate::diagnostic::Diagnostic;
use crate::engine::rule::ConstraintRule;
use crate::output::warning::Violation;
use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};

/// Checks DSC consistency: if DSC is enabled, both sink and source must declare support.
pub struct DscCheck;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for DscCheck {
    fn display_name(&self) -> &'static str {
        "dsc"
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
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        _cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V> {
        if !config.dsc_enabled {
            return None;
        }

        let source_ok = source.dsc.is_some_and(|d| d.dsc_1p2);
        let sink_ok = sink
            .hdmi_forum
            .as_ref()
            .and_then(|f| f.dsc.as_ref())
            .is_some_and(|d| d.dsc_1p2);

        if source_ok && sink_ok {
            None
        } else {
            Some(Violation::DscUnsupported.into())
        }
    }
}

#[cfg(test)]
#[path = "dsc_tests.rs"]
mod tests;
