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
mod tests {
    use super::*;
    use display_types::cea861::HdmiForumFrl;
    use display_types::{ColorBitDepth, ColorBitDepths, ColorCapabilities, ColorFormat, VideoMode};

    fn mode(refresh_rate: u16) -> VideoMode {
        VideoMode::new(1920, 1080, refresh_rate, false)
    }

    fn config(
        mode: &'_ VideoMode,
        encoding: ColorFormat,
        depth: ColorBitDepth,
    ) -> CandidateConfig<'_> {
        CandidateConfig {
            mode,
            color_encoding: encoding,
            bit_depth: depth,
            frl_rate: HdmiForumFrl::NotSupported,
            dsc_enabled: false,
        }
    }

    /// Sink with RGB 8 bpc and a 24–144 Hz refresh range.
    fn typical_sink() -> SinkCapabilities {
        let mut caps = ColorCapabilities::default();
        caps.rgb444 = ColorBitDepths::BPC_8
            .with(ColorBitDepth::Depth10)
            .with(ColorBitDepth::Depth12);
        caps.ycbcr420 = ColorBitDepths::BPC_8.with(ColorBitDepth::Depth10);
        SinkCapabilities {
            color_capabilities: caps,
            min_v_rate: Some(24),
            max_v_rate: Some(144),
            ..Default::default()
        }
    }

    fn probe(
        sink: &SinkCapabilities,
        mode: &VideoMode,
        encoding: ColorFormat,
        depth: ColorBitDepth,
    ) -> Result<(), alloc::vec::Vec<TaggedViolation<Violation>>> {
        is_config_viable(
            sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &config(mode, encoding, depth),
        )
        .map(|_| ())
        .map_err(crate::engine::Violations::into_vec)
    }

    #[test]
    fn viable_config_accepted() {
        let m = mode(60);
        assert!(
            probe(
                &typical_sink(),
                &m,
                ColorFormat::Rgb444,
                ColorBitDepth::Depth8
            )
            .is_ok()
        );
    }

    #[test]
    fn deep_color_accepted_when_supported() {
        let m = mode(60);
        assert!(
            probe(
                &typical_sink(),
                &m,
                ColorFormat::Rgb444,
                ColorBitDepth::Depth10
            )
            .is_ok()
        );
        assert!(
            probe(
                &typical_sink(),
                &m,
                ColorFormat::Rgb444,
                ColorBitDepth::Depth12
            )
            .is_ok()
        );
    }

    #[test]
    fn unsupported_encoding_rejected() {
        let m = mode(60);
        let result = probe(
            &typical_sink(),
            &m,
            ColorFormat::YCbCr444,
            ColorBitDepth::Depth8,
        );
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|v| matches!(v.violation, Violation::ColorEncodingUnsupported))
        );
    }

    #[test]
    fn unsupported_depth_rejected() {
        let m = mode(60);
        let result = probe(
            &typical_sink(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth16,
        );
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|v| matches!(v.violation, Violation::BitDepthUnsupported))
        );
    }

    #[test]
    fn refresh_rate_out_of_range_rejected() {
        let m = mode(240);
        let result = probe(
            &typical_sink(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
        );
        assert!(
            result
                .unwrap_err()
                .iter()
                .any(|v| matches!(v.violation, Violation::RefreshRateOutOfRange { .. }))
        );
    }

    #[test]
    fn multiple_violations_all_reported() {
        // Both encoding and depth are wrong; alloc mode collects all.
        let m = mode(240); // also out of range
        let violations = probe(
            &typical_sink(),
            &m,
            ColorFormat::YCbCr444,
            ColorBitDepth::Depth16,
        )
        .unwrap_err();
        assert!(
            violations
                .iter()
                .any(|v| matches!(v.violation, Violation::RefreshRateOutOfRange { .. }))
        );
        assert!(
            violations
                .iter()
                .any(|v| matches!(v.violation, Violation::ColorEncodingUnsupported))
        );
    }

    #[test]
    fn no_refresh_bounds_skips_refresh_check() {
        // A sink with color capabilities declared but no refresh rate range
        // does not reject a config on timing grounds.
        let mut caps = ColorCapabilities::default();
        caps.rgb444 = ColorBitDepths::BPC_8;
        let sink = SinkCapabilities {
            color_capabilities: caps,
            min_v_rate: None,
            max_v_rate: None,
            ..Default::default()
        };
        let m = mode(240);
        assert!(probe(&sink, &m, ColorFormat::Rgb444, ColorBitDepth::Depth8).is_ok());
    }
}
