use super::*;
use display_types::cea861::{HdmiDscMaxSlices, HdmiForumDsc, HdmiForumSinkCap};
use display_types::{ColorBitDepths, VideoMode};

fn mode(refresh_rate: u16) -> VideoMode {
    VideoMode::new(1920, 1080, refresh_rate, false)
}

/// Minimal `HdmiForumSinkCap` with just an FRL ceiling set.
fn hf_sink(max_frl_rate: HdmiForumFrl) -> HdmiForumSinkCap {
    HdmiForumSinkCap::new(
        1,
        0,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        max_frl_rate,
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
    )
}

/// `HdmiForumSinkCap` with an FRL ceiling and DSC 1.2 support.
fn hf_sink_dsc(max_frl_rate: HdmiForumFrl) -> HdmiForumSinkCap {
    let dsc = HdmiForumDsc::new(
        true,
        false,
        false,
        false,
        false,
        false,
        false,
        HdmiForumFrl::NotSupported,
        HdmiDscMaxSlices::Slices4,
        0,
    );
    HdmiForumSinkCap::new(
        1,
        0,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        false,
        max_frl_rate,
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
        Some(dsc),
    )
}

fn rgb8_sink() -> SinkCapabilities {
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8;
    SinkCapabilities {
        color_capabilities: caps,
        ..Default::default()
    }
}

fn frl6_sink() -> SinkCapabilities {
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8;
    SinkCapabilities {
        color_capabilities: caps,
        hdmi_forum: Some(hf_sink(HdmiForumFrl::Rate6Gbps4Lanes)),
        ..Default::default()
    }
}

fn frl6_source() -> SourceCapabilities {
    SourceCapabilities {
        max_frl_rate: HdmiForumFrl::Rate6Gbps4Lanes,
        ..Default::default()
    }
}

fn dsc_sink() -> SinkCapabilities {
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8;
    SinkCapabilities {
        color_capabilities: caps,
        hdmi_forum: Some(hf_sink_dsc(HdmiForumFrl::Rate6Gbps4Lanes)),
        ..Default::default()
    }
}

fn dsc_source() -> SourceCapabilities {
    use crate::types::source::DscCapabilities;
    SourceCapabilities {
        max_frl_rate: HdmiForumFrl::Rate6Gbps4Lanes,
        dsc: Some(DscCapabilities {
            dsc_1p2: true,
            max_slices: 4,
            max_bpp_x16: 128,
        }),
        ..Default::default()
    }
}

fn collect_from<'a>(
    modes: &'a [VideoMode],
    sink: &'a SinkCapabilities,
    source: &'a SourceCapabilities,
    cable: &'a CableCapabilities,
) -> alloc::vec::Vec<CandidateConfig<'a>> {
    build_iter(modes, sink, source, cable).collect()
}

#[test]
fn empty_mode_list_yields_nothing() {
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    assert!(collect_from(&[], &sink, &source, &cable).is_empty());
}

#[test]
fn no_usable_encoding_yields_nothing() {
    let modes = [mode(60)];
    let sink = SinkCapabilities::default();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    assert!(collect_from(&modes, &sink, &source, &cable).is_empty());
}

#[test]
fn tmds_only_single_mode_rgb8() {
    let modes = [mode(60)];
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    // 1 mode × 1 encoding × 1 depth × 1 FRL (NotSupported) × 1 DSC (false)
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].color_encoding, ColorFormat::Rgb444);
    assert_eq!(candidates[0].bit_depth, ColorBitDepth::Depth8);
    assert_eq!(candidates[0].frl_rate, HdmiForumFrl::NotSupported);
    assert!(!candidates[0].dsc_enabled);
}

#[test]
fn frl_rates_highest_first() {
    // RGB 8bpc, FRL ceiling = Rate6Gbps4Lanes → 4 tiers (6g4l, 6g3l, 3g3l, NotSupported).
    let modes = [mode(60)];
    let sink = frl6_sink();
    let source = frl6_source();
    let cable = CableCapabilities::unconstrained();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    let frl_rates: alloc::vec::Vec<_> = candidates.iter().map(|c| c.frl_rate).collect();
    for i in 1..frl_rates.len() {
        assert!(
            frl_rates[i - 1] >= frl_rates[i],
            "expected descending FRL order at position {i}: {:?} < {:?}",
            frl_rates[i - 1],
            frl_rates[i],
        );
    }
}

#[test]
fn dsc_false_before_true() {
    let modes = [mode(60)];
    let sink = dsc_sink();
    let source = dsc_source();
    let cable = CableCapabilities::unconstrained();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    assert!(
        !candidates[0].dsc_enabled,
        "first candidate should have dsc=false"
    );
    assert!(
        candidates[1].dsc_enabled,
        "second candidate should have dsc=true"
    );
}

#[test]
fn dsc_absent_when_source_lacks_it() {
    let modes = [mode(60)];
    let sink = dsc_sink();
    let source = frl6_source();
    let cable = CableCapabilities::unconstrained();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    assert!(candidates.iter().all(|c| !c.dsc_enabled));
}

#[test]
fn dsc_absent_when_sink_lacks_it() {
    let modes = [mode(60)];
    let sink = frl6_sink();
    let source = dsc_source();
    let cable = CableCapabilities::unconstrained();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    assert!(candidates.iter().all(|c| !c.dsc_enabled));
}

#[test]
fn candidate_count_matches_product() {
    // RGB only, 8+10 bpc, FRL ceiling = Rate6Gbps4Lanes (4 tiers), no DSC.
    // Expected: 2 modes × 1 encoding × 2 depths × 4 FRL rates × 1 DSC = 16.
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8.with(ColorBitDepth::Depth10);
    let sink = SinkCapabilities {
        color_capabilities: caps,
        hdmi_forum: Some(hf_sink(HdmiForumFrl::Rate6Gbps4Lanes)),
        ..Default::default()
    };
    let modes = [mode(60), mode(30)];
    let source = frl6_source();
    let cable = CableCapabilities::unconstrained();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    assert_eq!(candidates.len(), 16);
}

// --- Encoding order ---

#[test]
fn encoding_order_is_rgb_ycbcr444_ycbcr422_ycbcr420() {
    // When all four encodings are supported the candidates must appear in the
    // canonical order: Rgb444, YCbCr444, YCbCr422, YCbCr420 (encoding is the
    // second-slowest dimension, changing once per full depth×frl×dsc block).
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8;
    caps.ycbcr444 = ColorBitDepths::BPC_8;
    caps.ycbcr422 = ColorBitDepths::BPC_8;
    caps.ycbcr420 = ColorBitDepths::BPC_8;
    let sink = SinkCapabilities {
        color_capabilities: caps,
        ..Default::default()
    };
    let modes = [mode(60)];
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let candidates = collect_from(&modes, &sink, &source, &cable);

    // Extract the encoding at each transition (first occurrence of each value).
    let mut seen_order = alloc::vec::Vec::new();
    for c in &candidates {
        if seen_order.last() != Some(&c.color_encoding) {
            seen_order.push(c.color_encoding);
        }
    }

    assert_eq!(
        seen_order,
        alloc::vec![
            ColorFormat::Rgb444,
            ColorFormat::YCbCr444,
            ColorFormat::YCbCr422,
            ColorFormat::YCbCr420,
        ]
    );
}

// --- build_iter pre-filtering ---

#[test]
fn no_hf_scdb_yields_only_tmds() {
    // A sink with no hdmi_forum block has no FRL support; only NotSupported should appear.
    let modes = [mode(60)];
    let sink = rgb8_sink(); // no hdmi_forum
    let source = frl6_source();
    let cable = CableCapabilities::unconstrained();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    assert!(
        candidates
            .iter()
            .all(|c| c.frl_rate == HdmiForumFrl::NotSupported),
        "expected only TMDS candidates when sink has no HF-SCDB"
    );
}

#[test]
fn cable_is_binding_frl_ceiling() {
    // Source and sink both support Rate6Gbps4Lanes, but cable only Rate3Gbps3Lanes.
    let modes = [mode(60)];
    let sink = frl6_sink();
    let source = frl6_source();
    let cable = CableCapabilities {
        max_frl_rate: HdmiForumFrl::Rate3Gbps3Lanes,
        ..CableCapabilities::unconstrained()
    };
    let candidates = collect_from(&modes, &sink, &source, &cable);
    let max_frl = candidates.iter().map(|c| c.frl_rate).max().unwrap();
    assert_eq!(max_frl, HdmiForumFrl::Rate3Gbps3Lanes);
}

#[test]
fn all_seven_frl_tiers_when_ceiling_is_max() {
    // When effective ceiling is Rate12Gbps4Lanes, all 7 tiers should be emitted.
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8;
    let sink = SinkCapabilities {
        color_capabilities: caps,
        hdmi_forum: Some(hf_sink(HdmiForumFrl::Rate12Gbps4Lanes)),
        ..Default::default()
    };
    let source = SourceCapabilities {
        max_frl_rate: HdmiForumFrl::Rate12Gbps4Lanes,
        ..Default::default()
    };
    let modes = [mode(60)];
    let cable = CableCapabilities::unconstrained();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    let mut frl_rates: alloc::vec::Vec<_> = candidates.iter().map(|c| c.frl_rate).collect();
    frl_rates.dedup();
    assert_eq!(frl_rates.len(), 7);
}

#[test]
fn multiple_encodings_all_emitted() {
    // Sink with RGB and YCbCr420 should produce candidates for both.
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8;
    caps.ycbcr420 = ColorBitDepths::BPC_8;
    let sink = SinkCapabilities {
        color_capabilities: caps,
        ..Default::default()
    };
    let modes = [mode(60)];
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    let has_rgb = candidates
        .iter()
        .any(|c| c.color_encoding == ColorFormat::Rgb444);
    let has_y420 = candidates
        .iter()
        .any(|c| c.color_encoding == ColorFormat::YCbCr420);
    assert!(has_rgb, "expected RGB candidates");
    assert!(has_y420, "expected YCbCr420 candidates");
}

#[test]
fn per_encoding_depths_are_independent() {
    // RGB supports 8+10 bpc; YCbCr420 supports only 8 bpc.
    // RGB candidates should include Depth10; YCbCr420 candidates should not.
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8.with(ColorBitDepth::Depth10);
    caps.ycbcr420 = ColorBitDepths::BPC_8;
    let sink = SinkCapabilities {
        color_capabilities: caps,
        ..Default::default()
    };
    let modes = [mode(60)];
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let candidates = collect_from(&modes, &sink, &source, &cable);

    let rgb_depths: alloc::vec::Vec<_> = candidates
        .iter()
        .filter(|c| c.color_encoding == ColorFormat::Rgb444)
        .map(|c| c.bit_depth)
        .collect();
    let y420_depths: alloc::vec::Vec<_> = candidates
        .iter()
        .filter(|c| c.color_encoding == ColorFormat::YCbCr420)
        .map(|c| c.bit_depth)
        .collect();

    assert!(
        rgb_depths.contains(&ColorBitDepth::Depth10),
        "RGB should include Depth10"
    );
    assert!(
        !y420_depths.contains(&ColorBitDepth::Depth10),
        "YCbCr420 should not include Depth10"
    );
}

// --- Odometer ordering ---

#[test]
fn odometer_sequence_within_mode() {
    // Minimal case: 1 mode × 1 encoding (RGB) × 2 depths × 2 FRL tiers × 2 DSC states = 8
    // candidates, in exact odometer order (depth slowest, then frl, then dsc fastest).
    use crate::types::source::DscCapabilities;
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8.with(ColorBitDepth::Depth10);
    let sink = SinkCapabilities {
        color_capabilities: caps,
        hdmi_forum: Some(hf_sink_dsc(HdmiForumFrl::Rate3Gbps3Lanes)),
        ..Default::default()
    };
    let source = SourceCapabilities {
        max_frl_rate: HdmiForumFrl::Rate3Gbps3Lanes,
        dsc: Some(DscCapabilities {
            dsc_1p2: true,
            max_slices: 4,
            max_bpp_x16: 128,
        }),
        ..Default::default()
    };
    let modes = [mode(60)];
    let cable = CableCapabilities::unconstrained();
    let candidates = collect_from(&modes, &sink, &source, &cable);

    // Expected: depth changes slower than frl, frl slower than dsc.
    type T = (ColorBitDepth, HdmiForumFrl, bool);
    let expected: &[T] = &[
        (ColorBitDepth::Depth8, HdmiForumFrl::Rate3Gbps3Lanes, false),
        (ColorBitDepth::Depth8, HdmiForumFrl::Rate3Gbps3Lanes, true),
        (ColorBitDepth::Depth8, HdmiForumFrl::NotSupported, false),
        (ColorBitDepth::Depth8, HdmiForumFrl::NotSupported, true),
        (ColorBitDepth::Depth10, HdmiForumFrl::Rate3Gbps3Lanes, false),
        (ColorBitDepth::Depth10, HdmiForumFrl::Rate3Gbps3Lanes, true),
        (ColorBitDepth::Depth10, HdmiForumFrl::NotSupported, false),
        (ColorBitDepth::Depth10, HdmiForumFrl::NotSupported, true),
    ];

    assert_eq!(candidates.len(), expected.len());
    for (i, (c, &(depth, frl, dsc))) in candidates.iter().zip(expected).enumerate() {
        assert_eq!(c.bit_depth, depth, "depth mismatch at {i}");
        assert_eq!(c.frl_rate, frl, "frl mismatch at {i}");
        assert_eq!(c.dsc_enabled, dsc, "dsc mismatch at {i}");
    }
}

#[test]
fn mode_is_slowest_dimension() {
    // All candidates for mode[0] must precede all candidates for mode[1].
    let modes = [mode(60), mode(30)];
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    let switch = candidates
        .windows(2)
        .position(|w| !core::ptr::eq(w[0].mode, w[1].mode));
    if let Some(i) = switch {
        // After the first mode switch there must be no further reference to modes[0].
        assert!(
            candidates[i + 1..]
                .iter()
                .all(|c| !core::ptr::eq(c.mode, &modes[0])),
            "mode[0] appeared again after the switch at position {i}"
        );
    }
}

#[test]
fn all_candidates_borrow_from_mode_slice() {
    let modes = [mode(60), mode(30)];
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let candidates = collect_from(&modes, &sink, &source, &cable);
    let mid = candidates.len() / 2;
    assert!(
        candidates[..mid]
            .iter()
            .all(|c| core::ptr::eq(c.mode, &modes[0]))
    );
    assert!(
        candidates[mid..]
            .iter()
            .all(|c| core::ptr::eq(c.mode, &modes[1]))
    );
}

// --- SliceEnumerator ---

#[test]
fn slice_enumerator_uses_provided_slice_not_sink_modes() {
    // SliceEnumerator should enumerate the caller-supplied slice regardless of
    // what sink.supported_modes contains.
    use crate::types::sink::SupportedModes;
    let sink_mode = VideoMode::new(3840, 2160, 60u16, false);
    let slice_mode = VideoMode::new(1920, 1080, 60u16, false);

    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8;
    let (supported_modes, _) = SupportedModes::from_vec(alloc::vec![sink_mode]);
    let sink = SinkCapabilities {
        color_capabilities: caps,
        supported_modes,
        ..Default::default()
    };
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();

    let enumerator = SliceEnumerator::new(core::slice::from_ref(&slice_mode));
    let candidates: alloc::vec::Vec<_> =
        CandidateEnumerator::enumerate(&enumerator, &sink, &source, &cable).collect();

    assert!(!candidates.is_empty());
    assert!(
        candidates.iter().all(|c| c.mode.width == 1920),
        "expected only the slice mode (1920-wide), not the sink mode (3840-wide)"
    );
}

#[test]
fn slice_enumerator_enumerate_is_repeatable() {
    // enumerate() takes &self, so calling it twice must produce the same sequence.
    let modes = [mode(60), mode(30)];
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let enumerator = SliceEnumerator::new(&modes);

    let first: alloc::vec::Vec<_> =
        CandidateEnumerator::enumerate(&enumerator, &sink, &source, &cable).collect();
    let second: alloc::vec::Vec<_> =
        CandidateEnumerator::enumerate(&enumerator, &sink, &source, &cable).collect();

    assert_eq!(first, second);
}

// --- DefaultEnumerator ---

#[test]
fn default_enumerator_empty_supported_modes_yields_nothing() {
    // SinkCapabilities::default() has an empty SupportedModes list.
    let sink = rgb8_sink(); // supported_modes is default (empty)
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let candidates: alloc::vec::Vec<_> =
        CandidateEnumerator::enumerate(&DefaultEnumerator, &sink, &source, &cable).collect();
    assert!(candidates.is_empty());
}

#[test]
fn default_enumerator_matches_slice_enumerator() {
    // DefaultEnumerator must produce the same sequence as
    // SliceEnumerator::new(sink.supported_modes.as_slice()) for identical inputs.
    use crate::types::sink::SupportedModes;
    let mut caps = display_types::ColorCapabilities::default();
    caps.rgb444 = ColorBitDepths::BPC_8.with(ColorBitDepth::Depth10);
    let (supported_modes, _) = SupportedModes::from_vec(alloc::vec![mode(60), mode(30), mode(24)]);
    let sink = SinkCapabilities {
        color_capabilities: caps,
        supported_modes,
        hdmi_forum: Some(hf_sink(HdmiForumFrl::Rate6Gbps4Lanes)),
        ..Default::default()
    };
    let source = frl6_source();
    let cable = CableCapabilities::unconstrained();

    let from_default: alloc::vec::Vec<_> =
        CandidateEnumerator::enumerate(&DefaultEnumerator, &sink, &source, &cable).collect();
    let slice_enumerator = SliceEnumerator::new(sink.supported_modes.as_slice());
    let from_slice: alloc::vec::Vec<_> =
        CandidateEnumerator::enumerate(&slice_enumerator, &sink, &source, &cable).collect();

    assert_eq!(from_default, from_slice);
}

// --- size_hint / ExactSizeIterator ---

#[test]
fn size_hint_equals_collect_len() {
    // Initial size_hint must match the total number of candidates produced.
    let modes = [mode(60), mode(30)];
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let mut iter = build_iter(&modes, &sink, &source, &cable);
    let (lo, hi) = iter.size_hint();
    let collected: alloc::vec::Vec<_> = iter.by_ref().collect();
    assert_eq!(lo, collected.len());
    assert_eq!(hi, Some(collected.len()));
}

#[test]
fn size_hint_decrements_on_each_next() {
    // size_hint must decrease by exactly 1 after each call to next().
    let modes = [mode(60)];
    let sink = rgb8_sink();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::default();
    let mut iter = build_iter(&modes, &sink, &source, &cable);
    let total = iter.len();
    for consumed in 1..=total {
        iter.next();
        assert_eq!(
            iter.len(),
            total - consumed,
            "wrong len after {consumed} calls"
        );
    }
    assert_eq!(iter.len(), 0);
    assert!(iter.next().is_none());
}
