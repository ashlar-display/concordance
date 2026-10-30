use crate::diagnostic::Diagnostic;
use crate::engine::rule::ConstraintRule;
use crate::output::warning::Violation;
use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};

/// Checks that the requested color encoding is supported by the sink at any bit depth.
pub struct ColorEncodingCheck;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for ColorEncodingCheck {
    fn display_name(&self) -> &'static str {
        "color_encoding"
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
        _source: &SourceCapabilities,
        _cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V> {
        #[cfg(any(feature = "alloc", feature = "std"))]
        {
            use display_types::ColorFormat;
            // Reject non-YCbCr 4:2:0 encodings on exclusive-4:2:0 modes (Y420 VDB).
            if config.color_encoding != ColorFormat::YCbCr420
                && sink
                    .ycbcr420_exclusive_modes
                    .as_slice()
                    .contains(config.mode)
            {
                return Some(Violation::EncodingRestrictedToYCbCr420.into());
            }

            // Per-mode YCbCr 4:2:0 eligibility (Y420 VDB + CMB).
            if config.color_encoding == ColorFormat::YCbCr420 {
                let in_exclusive = sink
                    .ycbcr420_exclusive_modes
                    .as_slice()
                    .contains(config.mode);
                let in_capable = sink.ycbcr420_capable_modes.as_slice().contains(config.mode);

                if in_exclusive || in_capable {
                    return None;
                }

                // Per-mode lists are populated but mode is absent — reject.
                // If both lists are empty, fall through to the display-level check below.
                let lists_populated = !sink.ycbcr420_exclusive_modes.as_slice().is_empty()
                    || !sink.ycbcr420_capable_modes.as_slice().is_empty();
                if lists_populated {
                    return Some(Violation::ColorEncodingUnsupported.into());
                }
            }
        }

        // Display-level check: covers non-YCbCr420 formats, the no-alloc path,
        // and the fallback for manually-constructed SinkCapabilities with empty
        // per-mode lists.
        if sink
            .color_capabilities
            .for_format(config.color_encoding)
            .is_empty()
        {
            Some(Violation::ColorEncodingUnsupported.into())
        } else {
            None
        }
    }
}

/// Checks that the requested bit depth is supported by the sink for the requested color encoding.
///
/// Note: `BitDepthCheck` does not gate on encoding support. Run [`ColorEncodingCheck`] first
/// so that an unsupported format is rejected before the depth check runs.
pub struct BitDepthCheck;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for BitDepthCheck {
    fn display_name(&self) -> &'static str {
        "bit_depth"
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
        _source: &SourceCapabilities,
        _cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V> {
        if !sink
            .color_capabilities
            .for_format(config.color_encoding)
            .supports(config.bit_depth)
        {
            Some(Violation::BitDepthUnsupported.into())
        } else {
            None
        }
    }
}

#[cfg(test)]
#[path = "color_tests.rs"]
mod tests;
