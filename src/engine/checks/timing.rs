use crate::diagnostic::Diagnostic;
use crate::engine::rule::ConstraintRule;
use crate::output::warning::Violation;
use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};
use display_types::pixel_clock_khz;

/// Checks that the requested refresh rate falls within the sink's supported range.
pub struct RefreshRateCheck;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for RefreshRateCheck {
    fn display_name(&self) -> &'static str {
        "refresh_rate_range"
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
        use crate::types::source::QuirkFlags;
        if source
            .quirks
            .contains(QuirkFlags::IGNORE_REFRESH_RATE_RANGE)
        {
            return None;
        }
        let min_hz = sink.min_v_rate?;
        let max_hz = sink.max_v_rate?;
        let rate_hz = config.mode.refresh_rate?;
        if rate_hz < display_types::RefreshRate::from(min_hz)
            || rate_hz > display_types::RefreshRate::from(max_hz)
        {
            Some(
                Violation::RefreshRateOutOfRange {
                    rate_hz,
                    min_hz,
                    max_hz,
                }
                .into(),
            )
        } else {
            None
        }
    }
}

/// Checks that the pixel clock does not exceed the sink's declared maximum.
///
/// Reads `sink.max_pixel_clock_mhz`, which is populated from the EDID range limits
/// descriptor (descriptor type `0xFD`, byte 9). Applies to all link types — the pixel
/// clock ceiling is independent of encoding, bit depth, and whether the link is TMDS
/// or FRL.
pub struct PixelClockCheck;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for PixelClockCheck {
    fn display_name(&self) -> &'static str {
        "pixel_clock_ceiling"
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
        let limit_mhz = sink.max_pixel_clock_mhz? as u32;
        let pixel_clock_khz = pixel_clock_khz(config.mode);
        let required_mhz = pixel_clock_khz / 1000;
        if required_mhz > limit_mhz {
            use crate::output::warning::LimitSource;
            Some(
                Violation::PixelClockExceeded {
                    required_mhz,
                    limit_mhz,
                    limit_source: LimitSource::Sink,
                }
                .into(),
            )
        } else {
            None
        }
    }
}

/// Checks that the TMDS character rate does not exceed the ceiling for the link.
pub struct TmdsClockCheck;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for TmdsClockCheck {
    fn display_name(&self) -> &'static str {
        "tmds_clock_ceiling"
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
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V> {
        // Skip FRL candidates — TMDS clock is not relevant for FRL links.
        use display_types::cea861::HdmiForumFrl;
        if config.frl_rate != HdmiForumFrl::NotSupported {
            return None;
        }

        let pixel_clock_khz = pixel_clock_khz(config.mode);

        // TMDS character clock depends on color encoding and bit depth.
        // YCbCr 4:2:0 halves the pixel rate; deep color multiplies it.
        let encoding_denom = match config.color_encoding {
            display_types::ColorFormat::YCbCr420 => 2u32,
            _ => 1u32,
        };
        let depth_numer = match config.bit_depth {
            display_types::ColorBitDepth::Depth10 => 5u32,
            display_types::ColorBitDepth::Depth12 => 6u32,
            display_types::ColorBitDepth::Depth16 => 8u32,
            _ => 4u32, // 8 bpc: clock × 1
        };
        // tmds_clock = pixel_clock × depth_numer / (4 × encoding_denom)
        // Computed in u64: the pixel clock comes from sink-supplied EDID data and can be
        // anywhere in u32 range, so the deep-color multiply would overflow u32.
        let tmds_khz =
            u64::from(pixel_clock_khz) * u64::from(depth_numer) / u64::from(4 * encoding_denom);

        // Find the binding ceiling across sink, source, and cable.
        // A sink may declare its TMDS ceiling via the HDMI 1.x VSDB, the HDMI 2.x
        // HF-SCDB, or both; the tighter of the two is used.
        let vsdb_limit = sink
            .hdmi_vsdb
            .as_ref()
            .and_then(|v| v.max_tmds_clock_mhz)
            .map(|mhz| mhz as u32 * 1000)
            .unwrap_or(u32::MAX);
        // HF-SCDB max_tmds_rate_mhz == 0 means ≤ 340 MHz per HDMI 2.1a §10.3.6.
        let hf_limit = sink
            .hdmi_forum
            .as_ref()
            .map(|hf| {
                if hf.max_tmds_rate_mhz == 0 {
                    340_000
                } else {
                    hf.max_tmds_rate_mhz as u32 * 1000
                }
            })
            .unwrap_or(u32::MAX);
        let sink_limit = vsdb_limit.min(hf_limit);
        let source_limit = if source.max_tmds_clock > 0 {
            source.max_tmds_clock
        } else {
            u32::MAX
        };
        let cable_limit = if cable.max_tmds_clock > 0 {
            cable.max_tmds_clock
        } else {
            u32::MAX
        };
        let limit_khz = sink_limit.min(source_limit).min(cable_limit);

        if limit_khz == u32::MAX || tmds_khz <= u64::from(limit_khz) {
            None
        } else {
            use crate::output::warning::LimitSource;
            // Cable takes priority over source over sink when multiple parties share
            // the binding limit — cable replacement is the most actionable fix.
            let limit_source = if cable_limit == limit_khz {
                LimitSource::Cable
            } else if source_limit == limit_khz {
                LimitSource::Source
            } else {
                LimitSource::Sink
            };
            Some(
                Violation::TmdsClockExceeded {
                    required_mhz: u32::try_from(tmds_khz / 1000).unwrap_or(u32::MAX),
                    limit_mhz: limit_khz / 1000,
                    limit_source,
                }
                .into(),
            )
        }
    }
}

#[cfg(test)]
#[path = "timing_tests.rs"]
mod tests;
