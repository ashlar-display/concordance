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
