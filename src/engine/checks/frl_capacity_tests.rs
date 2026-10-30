use super::*;
use crate::engine::{ConstraintEngine, DefaultConstraintEngine};
use crate::frl::{Assumptions, AudioLoad, AudioPacketType, FrlShortfall, UnmodelledAudio};
use crate::output::warning::TaggedViolation;
use alloc::vec::Vec;
use display_types::cea861::vic_to_mode;
use display_types::{ColorBitDepth, ColorFormat, VideoMode};

fn rule_check(sink: &SinkCapabilities, config: &CandidateConfig<'_>) -> Option<Violation> {
    <FrlCapacityCheck as ConstraintRule<Violation>>::check(
        &FrlCapacityCheck,
        sink,
        &SourceCapabilities::default(),
        &CableCapabilities::unconstrained(),
        config,
    )
}

fn rule_warnings(sink: &SinkCapabilities, config: &CandidateConfig<'_>) -> Vec<Warning> {
    let mut out = Vec::new();
    <FrlCapacityCheck as ConstraintRule<Violation>>::warnings(
        &FrlCapacityCheck,
        sink,
        &SourceCapabilities::default(),
        &CableCapabilities::unconstrained(),
        config,
        &mut |w| out.push(w),
    );
    out
}

/// A sink known to declare no audio (`audio_loads: Some(empty)`).
fn no_audio_sink() -> SinkCapabilities {
    SinkCapabilities {
        audio_loads: Some(Vec::new()),
        ..Default::default()
    }
}

fn rgb(mode: &VideoMode, depth: ColorBitDepth, rate: HdmiForumFrl) -> CandidateConfig<'_> {
    CandidateConfig::new(mode, ColorFormat::Rgb444, depth, rate, false)
}

#[test]
fn rejects_a_rate_too_slow_for_the_timing() {
    // 3840×2160 @ 60 Hz RGB 12-bit at 3 Gbps × 3: the walkthrough's old top config.
    let mode = vic_to_mode(97).unwrap();
    let sink = no_audio_sink();
    let v = rule_check(
        &sink,
        &rgb(&mode, ColorBitDepth::Depth12, HdmiForumFrl::Rate3Gbps3Lanes),
    );
    assert!(matches!(
        v,
        Some(Violation::FrlCapacityExceeded {
            rate: HdmiForumFrl::Rate3Gbps3Lanes,
            shortfall: FrlShortfall::Borrow,
            ..
        })
    ));
    // 6 Gbps × 4 carries it: no violation, no warning.
    let config = rgb(&mode, ColorBitDepth::Depth12, HdmiForumFrl::Rate6Gbps4Lanes);
    assert!(rule_check(&sink, &config).is_none());
    assert!(rule_warnings(&sink, &config).is_empty());
}

#[test]
fn marginal_rate_is_accepted_with_a_warning() {
    // 1920 + 280 at 289.109 MHz, 10-bit, 3 Gbps × 3: only the margin tolerance decides
    // (see frl::tests::margin_tolerance_alone_decides).
    let mode = VideoMode::new(1920, 1080, 120u32, false)
        .with_pixel_clock(289_109)
        .with_h_blank(280);
    let sink = no_audio_sink();
    let config = rgb(&mode, ColorBitDepth::Depth10, HdmiForumFrl::Rate3Gbps3Lanes);
    assert!(rule_check(&sink, &config).is_none());
    let w = rule_warnings(&sink, &config);
    assert_eq!(w.len(), 1);
    assert!(matches!(
        w[0],
        Warning::FrlCapacityMarginal {
            deciding: Assumptions::MARGIN_TOLERANCE,
            ..
        }
    ));
}

#[test]
fn sink_audio_bounds_the_audio() {
    // Blanking 95 at 8 bpc holds the ACR packets, not 8-channel 192 kHz audio: marginal
    // on the audio load alone (see frl::tests::audio_blanking_boundary).
    let mode = VideoMode::new(3840, 2160, 120u32, false)
        .with_pixel_clock(533_250)
        .with_h_blank(95);
    let mut sink = no_audio_sink();
    let config = rgb(&mode, ColorBitDepth::Depth8, HdmiForumFrl::Rate12Gbps4Lanes);
    assert!(rule_warnings(&sink, &config).is_empty());
    sink.audio_loads = Some(Vec::from([AudioLoad::new(
        AudioPacketType::Sample,
        192_000,
        8,
    )]));
    let w = rule_warnings(&sink, &config);
    assert!(matches!(
        w[..],
        [Warning::FrlCapacityMarginal {
            deciding: Assumptions::AUDIO_LOAD,
            ..
        }]
    ));
}

#[test]
fn unverifiable_candidates_are_accepted_with_a_warning() {
    let sink = SinkCapabilities::default();
    let rate = HdmiForumFrl::Rate12Gbps4Lanes;
    let unverified = |w: &[Warning]| match w {
        [Warning::FrlCapacityUnverified { reason, .. }] => Some(*reason),
        _ => None,
    };
    // No blanking (a mode known by resolution and refresh rate only).
    let bare = VideoMode::new(3840, 2160, 60u32, false).with_pixel_clock(594_000);
    let config = rgb(&bare, ColorBitDepth::Depth8, rate);
    assert!(rule_check(&sink, &config).is_none());
    assert_eq!(
        unverified(&rule_warnings(&sink, &config)),
        Some(FrlUnverified::NoBlanking)
    );
    // DSC.
    let mode = vic_to_mode(97).unwrap();
    let dsc = CandidateConfig::new(
        &mode,
        ColorFormat::Rgb444,
        ColorBitDepth::Depth8,
        rate,
        true,
    );
    assert!(rule_check(&sink, &dsc).is_none());
    assert_eq!(
        unverified(&rule_warnings(&sink, &dsc)),
        Some(FrlUnverified::Dsc)
    );
    // TMDS: not this rule's concern.
    let tmds = rgb(&mode, ColorBitDepth::Depth8, HdmiForumFrl::NotSupported);
    assert!(rule_check(&sink, &tmds).is_none());
    assert!(rule_warnings(&sink, &tmds).is_empty());
}

#[test]
fn unmodelled_audio_is_reported() {
    let mode = vic_to_mode(97).unwrap();
    let sink = SinkCapabilities {
        unmodelled_audio: Vec::from([
            UnmodelledAudio::Format(display_types::cea861::AudioFormat::Dst),
            UnmodelledAudio::MultiStream { max_streams: 2 },
        ]),
        ..no_audio_sink()
    };
    let config = rgb(&mode, ColorBitDepth::Depth8, HdmiForumFrl::Rate12Gbps4Lanes);
    assert!(matches!(
        rule_warnings(&sink, &config)[..],
        [
            Warning::FrlCapacityUnverified {
                reason: FrlUnverified::UnmodelledAudio(UnmodelledAudio::Format(
                    display_types::cea861::AudioFormat::Dst
                )),
                ..
            },
            Warning::FrlCapacityUnverified {
                reason: FrlUnverified::UnmodelledAudio(UnmodelledAudio::MultiStream {
                    max_streams: 2
                }),
                ..
            }
        ]
    ));
}

#[test]
fn unknown_sink_audio_is_reported_and_the_video_still_checked() {
    // A hand-built sink (audio_loads None): no silent "no audio".
    let mode = vic_to_mode(97).unwrap();
    let sink = SinkCapabilities::default();
    let ok = rgb(&mode, ColorBitDepth::Depth8, HdmiForumFrl::Rate12Gbps4Lanes);
    assert!(matches!(
        rule_warnings(&sink, &ok)[..],
        [Warning::FrlCapacityUnverified {
            reason: FrlUnverified::AudioUnknown,
            ..
        }]
    ));
    // The video is still checked: a rate too slow is still rejected.
    let slow = rgb(&mode, ColorBitDepth::Depth12, HdmiForumFrl::Rate3Gbps3Lanes);
    assert!(rule_check(&sink, &slow).is_some());
}

#[test]
fn default_engine_collects_warnings_of_accepted_candidates() {
    let mode = VideoMode::new(1920, 1080, 120u32, false)
        .with_pixel_clock(289_109)
        .with_h_blank(280);
    // No HDMI Forum block: the FRL ceiling check rejects, and no warnings come.
    let sink = no_audio_sink();
    let config = rgb(&mode, ColorBitDepth::Depth10, HdmiForumFrl::Rate3Gbps3Lanes);
    let engine = DefaultConstraintEngine::default();
    let source = SourceCapabilities::default();
    let cable = CableCapabilities::unconstrained();
    let rejected: crate::engine::CheckResult<Warning, TaggedViolation<Violation>> =
        engine.check(&sink, &source, &cable, &config);
    assert!(rejected.is_err());

    // A rule list with only the capacity check accepts it, with the warning.
    static ONLY_CAPACITY: crate::engine::CheckList<Violation> = &[&FrlCapacityCheck];
    let engine = DefaultConstraintEngine::with_checks(ONLY_CAPACITY);
    let warnings = engine
        .check(&sink, &source, &cable, &config)
        .unwrap()
        .into_vec();
    assert!(matches!(
        warnings[..],
        [Warning::FrlCapacityMarginal { .. }]
    ));
}
