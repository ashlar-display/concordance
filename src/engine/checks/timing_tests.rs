use super::*;
use crate::engine::rule::ConstraintRule;
use crate::output::warning::Violation;
use crate::types::{CableCapabilities, CandidateConfig, SourceCapabilities};
use display_types::cea861::{HdmiForumFrl, HdmiForumSinkCap, HdmiVsdb, HdmiVsdbFlags};
use display_types::{ColorBitDepth, ColorFormat, VideoMode};

fn mode(refresh_rate: u16) -> VideoMode {
    VideoMode::new(1920, 1080, refresh_rate, false)
}

fn config(mode: &VideoMode) -> CandidateConfig<'_> {
    CandidateConfig {
        mode,
        color_encoding: ColorFormat::Rgb444,
        bit_depth: ColorBitDepth::Depth8,
        frl_rate: HdmiForumFrl::NotSupported,
        dsc_enabled: false,
    }
}

fn tmds_check(
    sink: &SinkCapabilities,
    source: &SourceCapabilities,
    cable: &CableCapabilities,
    mode: &VideoMode,
    encoding: ColorFormat,
    depth: ColorBitDepth,
    frl_rate: HdmiForumFrl,
) -> Option<Violation> {
    ConstraintRule::<Violation>::check(
        &TmdsClockCheck,
        sink,
        source,
        cable,
        &CandidateConfig {
            mode,
            color_encoding: encoding,
            bit_depth: depth,
            frl_rate,
            dsc_enabled: false,
        },
    )
}

fn pixel_clock_check(sink: &SinkCapabilities, mode: &VideoMode) -> Option<Violation> {
    ConstraintRule::<Violation>::check(
        &PixelClockCheck,
        sink,
        &SourceCapabilities::default(),
        &CableCapabilities::default(),
        &config(mode),
    )
}

fn sink_with_tmds_limit(max_mhz: u16) -> SinkCapabilities {
    SinkCapabilities {
        hdmi_vsdb: Some(HdmiVsdb::new(
            0,
            HdmiVsdbFlags::empty(),
            Some(max_mhz),
            None,
            None,
            None,
            None,
        )),
        ..Default::default()
    }
}

fn source_with_tmds_limit(max_khz: u32) -> SourceCapabilities {
    SourceCapabilities {
        max_tmds_clock: max_khz,
        ..Default::default()
    }
}

/// Sink with an HF-SCDB carrying `max_tmds_rate_mhz` but no VSDB.
/// Pass `0` to exercise the "≤ 340 MHz" sentinel defined in HDMI 2.1a §10.3.6.
fn sink_with_hf_tmds_limit(max_tmds_rate_mhz: u16) -> SinkCapabilities {
    SinkCapabilities {
        hdmi_forum: Some(HdmiForumSinkCap::new(
            1,
            max_tmds_rate_mhz,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            HdmiForumFrl::Rate12Gbps4Lanes,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
            None,
            None,
        )),
        ..Default::default()
    }
}

fn check(sink: &SinkCapabilities, mode: &VideoMode) -> Option<Violation> {
    ConstraintRule::<Violation>::check(
        &RefreshRateCheck,
        sink,
        &SourceCapabilities::default(),
        &CableCapabilities::default(),
        &config(mode),
    )
}

#[test]
fn within_range_passes() {
    let sink = SinkCapabilities {
        min_v_rate: Some(24),
        max_v_rate: Some(144),
        ..Default::default()
    };
    assert!(check(&sink, &mode(60)).is_none());
}

#[test]
fn above_max_rejected() {
    let sink = SinkCapabilities {
        min_v_rate: Some(24),
        max_v_rate: Some(60),
        ..Default::default()
    };
    assert!(matches!(
        check(&sink, &mode(144)),
        Some(Violation::RefreshRateOutOfRange {
            min_hz: 24,
            max_hz: 60,
            ..
        })
    ));
}

#[test]
fn below_min_rejected() {
    let sink = SinkCapabilities {
        min_v_rate: Some(48),
        max_v_rate: Some(144),
        ..Default::default()
    };
    assert!(matches!(
        check(&sink, &mode(24)),
        Some(Violation::RefreshRateOutOfRange {
            min_hz: 48,
            max_hz: 144,
            ..
        })
    ));
}

#[test]
fn no_bounds_skips_check() {
    assert!(check(&SinkCapabilities::default(), &mode(240)).is_none());
}

#[test]
fn ignore_refresh_rate_range_quirk_bypasses_check() {
    use crate::types::source::QuirkFlags;
    let sink = SinkCapabilities {
        min_v_rate: Some(24),
        max_v_rate: Some(60),
        ..Default::default()
    };
    let source = SourceCapabilities {
        quirks: QuirkFlags::IGNORE_REFRESH_RATE_RANGE,
        ..Default::default()
    };
    // 144 Hz would normally be rejected (above declared max of 60 Hz).
    let result = ConstraintRule::<Violation>::check(
        &RefreshRateCheck,
        &sink,
        &source,
        &CableCapabilities::default(),
        &config(&mode(144)),
    );
    assert!(
        result.is_none(),
        "IGNORE_REFRESH_RATE_RANGE must suppress the range check"
    );
}

#[test]
fn at_boundary_passes() {
    let sink = SinkCapabilities {
        min_v_rate: Some(24),
        max_v_rate: Some(144),
        ..Default::default()
    };
    assert!(check(&sink, &mode(24)).is_none());
    assert!(check(&sink, &mode(144)).is_none());
}

// --- PixelClockCheck tests ---
// CVT-RB estimate for 1920×1080@60: ≈ 135_782 kHz ≈ 135 MHz.

#[test]
fn no_pixel_clock_limit_skips_check() {
    // SinkCapabilities::default() has max_pixel_clock_mhz = None — check is skipped.
    assert!(pixel_clock_check(&SinkCapabilities::default(), &mode(60)).is_none());
}

#[test]
fn within_pixel_clock_limit_passes() {
    let sink = SinkCapabilities {
        max_pixel_clock_mhz: Some(165),
        ..Default::default()
    };
    assert!(pixel_clock_check(&sink, &mode(60)).is_none());
}

#[test]
fn exceeds_pixel_clock_limit_rejected() {
    let sink = SinkCapabilities {
        max_pixel_clock_mhz: Some(100),
        ..Default::default()
    };
    assert!(matches!(
        pixel_clock_check(&sink, &mode(60)),
        Some(Violation::PixelClockExceeded { limit_mhz: 100, .. })
    ));
}

#[test]
fn pixel_clock_check_applies_to_frl_candidate() {
    // Unlike TmdsClockCheck, the pixel clock ceiling applies regardless of link type.
    let sink = SinkCapabilities {
        max_pixel_clock_mhz: Some(100),
        ..Default::default()
    };
    let m = mode(60);
    let result = ConstraintRule::<Violation>::check(
        &PixelClockCheck,
        &sink,
        &SourceCapabilities::default(),
        &CableCapabilities::default(),
        &CandidateConfig {
            mode: &m,
            color_encoding: ColorFormat::Rgb444,
            bit_depth: ColorBitDepth::Depth8,
            frl_rate: HdmiForumFrl::Rate12Gbps4Lanes,
            dsc_enabled: false,
        },
    );
    assert!(matches!(
        result,
        Some(Violation::PixelClockExceeded { limit_mhz: 100, .. })
    ));
}

// --- TmdsClockCheck tests ---
// CVT-RB estimate for 1920×1080@60: (1920+160)×(1080+8)×60/1000 = 135_782 kHz ≈ 136 MHz.
// At 8bpc RGB, TMDS clock == pixel clock (depth_numer=4, denom=4).
// At 10bpc RGB, TMDS clock = pixel_clock × 5/4 ≈ 169_727 kHz ≈ 170 MHz.
// At YCbCr420 8bpc, TMDS clock = pixel_clock / 2 ≈ 67_891 kHz ≈ 68 MHz.

#[test]
fn frl_candidate_skips_tmds_check() {
    // Any FRL rate bypasses the TMDS check regardless of limits.
    let sink = sink_with_tmds_limit(10); // absurdly low — would fail if checked
    let m = mode(60);
    assert!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::Rate3Gbps3Lanes,
        )
        .is_none()
    );
}

#[test]
fn no_limits_passes() {
    // Default sink (no VSDB), default source (max_tmds_clock=0), unconstrained cable.
    let m = mode(60);
    assert!(
        tmds_check(
            &SinkCapabilities::default(),
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        )
        .is_none()
    );
}

#[test]
fn within_sink_limit_passes() {
    // 165 MHz sink limit; 1080p60 8bpc ≈ 136 MHz — passes.
    let sink = sink_with_tmds_limit(165);
    let m = mode(60);
    assert!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        )
        .is_none()
    );
}

#[test]
fn exceeds_sink_limit_rejected() {
    // 100 MHz sink limit; 1080p60 8bpc ≈ 136 MHz — fails.
    let sink = sink_with_tmds_limit(100);
    let m = mode(60);
    assert!(matches!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded { limit_mhz: 100, .. })
    ));
}

#[test]
fn exceeds_source_limit_rejected() {
    // Source limited to 100_000 kHz; 1080p60 8bpc ≈ 135_782 kHz — fails.
    let source = source_with_tmds_limit(100_000);
    let m = mode(60);
    assert!(matches!(
        tmds_check(
            &SinkCapabilities::default(),
            &source,
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded { limit_mhz: 100, .. })
    ));
}

#[test]
fn deep_color_10bpc_increases_clock() {
    // At 165 MHz, 8bpc passes (≈136 MHz) but 10bpc fails (≈170 MHz).
    let sink = sink_with_tmds_limit(165);
    let m = mode(60);
    assert!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        )
        .is_none()
    );
    assert!(matches!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth10,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded { limit_mhz: 165, .. })
    ));
}

// --- TmdsClockCheck: HF-SCDB limit tests ---

#[test]
fn hf_scdb_zero_rate_applies_340mhz_ceiling() {
    // max_tmds_rate_mhz == 0 means ≤ 340 MHz. 1080p60 8bpc ≈ 136 MHz — passes.
    let sink = sink_with_hf_tmds_limit(0);
    let m = mode(60);
    assert!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        )
        .is_none()
    );
}

#[test]
fn hf_scdb_nonzero_rate_is_ceiling() {
    // Explicit HF-SCDB limit of 100 MHz; 1080p60 8bpc ≈ 136 MHz — rejected.
    let sink = sink_with_hf_tmds_limit(100);
    let m = mode(60);
    assert!(matches!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded { limit_mhz: 100, .. })
    ));
}

#[test]
fn hf_scdb_ceiling_tighter_than_vsdb() {
    // VSDB allows 600 MHz; HF-SCDB restricts to 100 MHz — HF-SCDB wins.
    let sink = SinkCapabilities {
        hdmi_vsdb: Some(HdmiVsdb::new(
            0,
            HdmiVsdbFlags::empty(),
            Some(600),
            None,
            None,
            None,
            None,
        )),
        hdmi_forum: Some(HdmiForumSinkCap::new(
            1,
            100,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            HdmiForumFrl::Rate12Gbps4Lanes,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            false,
            None,
            None,
            None,
        )),
        ..Default::default()
    };
    let m = mode(60);
    assert!(matches!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded { limit_mhz: 100, .. })
    ));
}

// --- TmdsClockCheck: LimitSource field tests ---

#[test]
fn tmds_limit_source_is_sink_when_sink_is_binding() {
    use crate::output::warning::LimitSource;
    // Sink limited to 100 MHz; source unconstrained; cable unconstrained.
    let sink = sink_with_tmds_limit(100);
    let m = mode(60);
    assert!(matches!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded {
            limit_source: LimitSource::Sink,
            ..
        })
    ));
}

#[test]
fn tmds_limit_source_is_source_when_source_is_binding() {
    use crate::output::warning::LimitSource;
    // Source limited to 100_000 kHz; sink unconstrained; cable unconstrained.
    let source = source_with_tmds_limit(100_000);
    let m = mode(60);
    assert!(matches!(
        tmds_check(
            &SinkCapabilities::default(),
            &source,
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded {
            limit_source: LimitSource::Source,
            ..
        })
    ));
}

#[test]
fn tmds_limit_source_is_cable_when_cable_is_binding() {
    use crate::output::warning::LimitSource;
    // Cable limited to 100_000 kHz; sink and source both unconstrained.
    let cable = CableCapabilities {
        max_tmds_clock: 100_000,
        ..CableCapabilities::unconstrained()
    };
    let m = mode(60);
    assert!(matches!(
        tmds_check(
            &SinkCapabilities::default(),
            &SourceCapabilities::default(),
            &cable,
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded {
            limit_source: LimitSource::Cable,
            ..
        })
    ));
}

#[test]
fn ycbcr420_halves_clock_stays_within_limit() {
    // 80 MHz limit; YCbCr420 8bpc ≈ 68 MHz — passes.
    // Same mode with RGB 8bpc ≈ 136 MHz would fail.
    let sink = sink_with_tmds_limit(80);
    let m = mode(60);
    assert!(matches!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        ),
        Some(Violation::TmdsClockExceeded { .. })
    ));
    assert!(
        tmds_check(
            &sink,
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::YCbCr420,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
        )
        .is_none()
    );
}

#[test]
fn extreme_pixel_clock_deep_color_does_not_overflow() {
    // A sink-supplied clock near u32::MAX at 16 bpc (× 2) overflowed the u32
    // multiply: panic with overflow checks, wrap-around (false accept) without.
    let m = mode(60).with_pixel_clock(u32::MAX);
    let v = tmds_check(
        &SinkCapabilities::default(),
        &source_with_tmds_limit(600_000),
        &CableCapabilities::default(),
        &m,
        ColorFormat::Rgb444,
        ColorBitDepth::Depth16,
        HdmiForumFrl::NotSupported,
    );
    assert!(matches!(
        v,
        Some(Violation::TmdsClockExceeded {
            required_mhz: 8_589_934,
            limit_mhz: 600,
            ..
        })
    ));
}

#[test]
fn extreme_pixel_clock_without_limits_does_not_overflow() {
    let m = mode(60).with_pixel_clock(u32::MAX);
    assert!(
        tmds_check(
            &SinkCapabilities::default(),
            &SourceCapabilities::default(),
            &CableCapabilities::default(),
            &m,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth16,
            HdmiForumFrl::NotSupported,
        )
        .is_none()
    );
}
