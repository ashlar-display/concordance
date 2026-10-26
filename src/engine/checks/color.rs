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
mod tests {
    use super::*;
    use crate::engine::rule::ConstraintRule;
    use crate::output::warning::Violation;
    use crate::types::{CableCapabilities, CandidateConfig, SourceCapabilities};
    use display_types::cea861::HdmiForumFrl;
    use display_types::{ColorBitDepth, ColorBitDepths, ColorCapabilities, ColorFormat, VideoMode};

    fn mode() -> VideoMode {
        VideoMode::new(1920, 1080, 60u16, false)
    }

    fn config<'a>(
        mode: &'a VideoMode,
        encoding: ColorFormat,
        depth: ColorBitDepth,
    ) -> CandidateConfig<'a> {
        CandidateConfig {
            mode,
            color_encoding: encoding,
            bit_depth: depth,
            frl_rate: HdmiForumFrl::NotSupported,
            dsc_enabled: false,
        }
    }

    fn check_encoding(sink: &SinkCapabilities, encoding: ColorFormat) -> Option<Violation> {
        let m = mode();
        ConstraintRule::<Violation>::check(
            &ColorEncodingCheck,
            sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &config(&m, encoding, ColorBitDepth::Depth8),
        )
    }

    fn check_depth(
        sink: &SinkCapabilities,
        encoding: ColorFormat,
        depth: ColorBitDepth,
    ) -> Option<Violation> {
        let m = mode();
        ConstraintRule::<Violation>::check(
            &BitDepthCheck,
            sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &config(&m, encoding, depth),
        )
    }

    fn rgb_only_sink() -> SinkCapabilities {
        let mut caps = ColorCapabilities::default();
        caps.rgb444 = ColorBitDepths::BPC_8;
        SinkCapabilities {
            color_capabilities: caps,
            ..Default::default()
        }
    }

    fn deep_color_sink() -> SinkCapabilities {
        let mut caps = ColorCapabilities::default();
        caps.rgb444 = ColorBitDepths::BPC_8
            .with(ColorBitDepth::Depth10)
            .with(ColorBitDepth::Depth12);
        caps.ycbcr444 = ColorBitDepths::BPC_8.with(ColorBitDepth::Depth10);
        caps.ycbcr422 = ColorBitDepths::BPC_8;
        caps.ycbcr420 = ColorBitDepths::BPC_8.with(ColorBitDepth::Depth10);
        SinkCapabilities {
            color_capabilities: caps,
            ..Default::default()
        }
    }

    // --- ColorEncodingCheck: per-mode YCbCr 4:2:0 (alloc path) ---

    #[cfg(any(feature = "alloc", feature = "std"))]
    fn exclusive_sink(exclusive_mode: VideoMode) -> SinkCapabilities {
        use crate::types::SupportedModes;
        let (modes, _) = SupportedModes::from_vec(alloc::vec![exclusive_mode]);
        SinkCapabilities {
            ycbcr420_exclusive_modes: modes,
            ..Default::default()
        }
    }

    #[cfg(any(feature = "alloc", feature = "std"))]
    fn capable_sink(capable_mode: VideoMode) -> SinkCapabilities {
        use crate::types::SupportedModes;
        let mut caps = ColorCapabilities::default();
        caps.rgb444 = ColorBitDepths::BPC_8;
        let (modes, _) = SupportedModes::from_vec(alloc::vec![capable_mode]);
        SinkCapabilities {
            color_capabilities: caps,
            ycbcr420_capable_modes: modes,
            ..Default::default()
        }
    }

    /// A mode in `ycbcr420_exclusive_modes` accepts YCbCr 4:2:0.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn exclusive_mode_allows_ycbcr420() {
        let m = mode();
        let sink = exclusive_sink(m.clone());
        let result = check_encoding(&sink, ColorFormat::YCbCr420);
        assert!(
            result.is_none(),
            "YCbCr 4:2:0 must be allowed on an exclusive mode"
        );
    }

    /// A mode in `ycbcr420_exclusive_modes` rejects any non-YCbCr 4:2:0 encoding.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn exclusive_mode_rejects_rgb() {
        let m = mode();
        let sink = exclusive_sink(m.clone());
        assert!(
            matches!(
                check_encoding(&sink, ColorFormat::Rgb444),
                Some(Violation::EncodingRestrictedToYCbCr420)
            ),
            "RGB must be rejected on an exclusive-4:2:0 mode"
        );
    }

    /// A mode in `ycbcr420_capable_modes` accepts YCbCr 4:2:0.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn capable_mode_allows_ycbcr420() {
        let m = mode();
        let sink = capable_sink(m.clone());
        let result = check_encoding(&sink, ColorFormat::YCbCr420);
        assert!(
            result.is_none(),
            "YCbCr 4:2:0 must be allowed on a capable mode"
        );
    }

    /// A mode in `ycbcr420_capable_modes` still accepts other declared encodings.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn capable_mode_also_allows_rgb() {
        let m = mode();
        let sink = capable_sink(m.clone());
        assert!(
            check_encoding(&sink, ColorFormat::Rgb444).is_none(),
            "RGB must still be allowed on a capable mode"
        );
    }

    /// When per-mode lists are populated but the candidate mode is absent,
    /// YCbCr 4:2:0 is rejected even if `color_capabilities.ycbcr420` is set.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn ycbcr420_rejected_when_mode_absent_from_populated_lists() {
        use crate::types::SupportedModes;
        let listed_mode = VideoMode::new(3840, 2160, 60u16, false);
        let other_mode = VideoMode::new(1920, 1080, 60u16, false);
        let mut caps = ColorCapabilities::default();
        caps.ycbcr420 = ColorBitDepths::BPC_8; // display-level claim
        let (modes, _) = SupportedModes::from_vec(alloc::vec![listed_mode]);
        let sink = SinkCapabilities {
            color_capabilities: caps,
            ycbcr420_exclusive_modes: modes,
            ..Default::default()
        };
        let result = ConstraintRule::<Violation>::check(
            &ColorEncodingCheck,
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &config(&other_mode, ColorFormat::YCbCr420, ColorBitDepth::Depth8),
        );
        assert!(
            matches!(result, Some(Violation::ColorEncodingUnsupported)),
            "YCbCr 4:2:0 must be rejected when mode is not in the per-mode lists"
        );
    }

    /// When both per-mode lists are empty, falls back to `color_capabilities.ycbcr420`.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[test]
    fn ycbcr420_fallback_to_display_level_caps() {
        let mut caps = ColorCapabilities::default();
        caps.ycbcr420 = ColorBitDepths::BPC_8;
        let sink = SinkCapabilities {
            color_capabilities: caps,
            ..Default::default()
        };
        let result = check_encoding(&sink, ColorFormat::YCbCr420);
        assert!(
            result.is_none(),
            "YCbCr 4:2:0 must be allowed via display-level fallback when per-mode lists are empty"
        );
    }

    // --- ColorEncodingCheck ---

    #[test]
    fn supported_encoding_passes() {
        assert!(check_encoding(&rgb_only_sink(), ColorFormat::Rgb444).is_none());
    }

    #[test]
    fn unsupported_encoding_rejected() {
        assert!(matches!(
            check_encoding(&rgb_only_sink(), ColorFormat::YCbCr444),
            Some(Violation::ColorEncodingUnsupported)
        ));
    }

    #[test]
    fn all_formats_pass_on_full_sink() {
        let sink = deep_color_sink();
        assert!(check_encoding(&sink, ColorFormat::Rgb444).is_none());
        assert!(check_encoding(&sink, ColorFormat::YCbCr444).is_none());
        assert!(check_encoding(&sink, ColorFormat::YCbCr422).is_none());
        assert!(check_encoding(&sink, ColorFormat::YCbCr420).is_none());
    }

    // --- BitDepthCheck ---

    #[test]
    fn supported_depth_passes() {
        assert!(
            check_depth(
                &deep_color_sink(),
                ColorFormat::Rgb444,
                ColorBitDepth::Depth10
            )
            .is_none()
        );
    }

    #[test]
    fn unsupported_depth_rejected() {
        assert!(matches!(
            check_depth(
                &deep_color_sink(),
                ColorFormat::Rgb444,
                ColorBitDepth::Depth16
            ),
            Some(Violation::BitDepthUnsupported)
        ));
    }

    #[test]
    fn depth_checked_per_format() {
        let sink = deep_color_sink();
        // 10 bpc is supported for RGB and YCbCr 4:4:4 but not 4:2:2.
        assert!(check_depth(&sink, ColorFormat::Rgb444, ColorBitDepth::Depth10).is_none());
        assert!(check_depth(&sink, ColorFormat::YCbCr444, ColorBitDepth::Depth10).is_none());
        assert!(matches!(
            check_depth(&sink, ColorFormat::YCbCr422, ColorBitDepth::Depth10),
            Some(Violation::BitDepthUnsupported)
        ));
    }
}
