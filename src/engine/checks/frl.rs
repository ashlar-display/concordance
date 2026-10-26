use display_types::cea861::HdmiForumFrl;

use crate::diagnostic::Diagnostic;
use crate::engine::rule::ConstraintRule;
use crate::frl::{
    FrlAudio, FrlCapacityInput, FrlCapacityReport, FrlUnverified, FrlVerdict, frl_capacity,
};
use crate::output::warning::{Violation, Warning};
use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};

/// Maps an `HdmiForumFrl` rate to a comparable tier index.
///
/// Higher values represent higher bandwidth tiers. Unknown future values
/// (from `#[non_exhaustive]` additions) map to `0` — treated conservatively
/// as unsupported rather than panicking.
pub(in crate::engine) fn frl_tier(rate: HdmiForumFrl) -> u8 {
    match rate {
        HdmiForumFrl::NotSupported => 0,
        HdmiForumFrl::Rate3Gbps3Lanes => 1,
        HdmiForumFrl::Rate6Gbps3Lanes => 2,
        HdmiForumFrl::Rate6Gbps4Lanes => 3,
        HdmiForumFrl::Rate8Gbps4Lanes => 4,
        HdmiForumFrl::Rate10Gbps4Lanes => 5,
        HdmiForumFrl::Rate12Gbps4Lanes => 6,
        _ => 0,
    }
}

/// Checks that the requested FRL rate does not exceed the ceiling imposed by
/// the sink, source, and cable.
///
/// TMDS candidates (`frl_rate == NotSupported`) are not subject to this check.
/// A sink without an HF-SCDB declares no FRL support and imposes a ceiling of 0.
pub struct FrlCeilingCheck;

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for FrlCeilingCheck {
    fn display_name(&self) -> &'static str {
        "frl_ceiling"
    }

    fn check(
        &self,
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V> {
        if config.frl_rate == HdmiForumFrl::NotSupported {
            return None;
        }

        let requested = frl_tier(config.frl_rate);

        let sink_rate = sink
            .hdmi_forum
            .as_ref()
            .map_or(HdmiForumFrl::NotSupported, |hf| {
                if config.dsc_enabled {
                    // DSC transport has its own FRL ceiling declared separately from the
                    // non-DSC maximum. If the sink has no DSC section, DscCheck will reject
                    // the candidate; treat the ceiling as 0 here for a consistent safe result.
                    hf.dsc
                        .as_ref()
                        .map_or(HdmiForumFrl::NotSupported, |d| d.max_frl_rate)
                } else {
                    hf.max_frl_rate
                }
            });
        let source_rate = source.max_frl_rate;
        let cable_rate = cable.max_frl_rate;

        let sink_max = frl_tier(sink_rate);
        let source_max = frl_tier(source_rate);
        let cable_max = frl_tier(cable_rate);

        let effective_max = sink_max.min(source_max).min(cable_max);

        if requested > effective_max {
            use crate::output::warning::LimitSource;
            // Cable takes priority over source over sink when multiple parties share
            // the binding limit — cable replacement is the most actionable fix.
            let (limit, limit_source) = if cable_max == effective_max {
                (cable_rate, LimitSource::Cable)
            } else if source_max == effective_max {
                (source_rate, LimitSource::Source)
            } else {
                (sink_rate, LimitSource::Sink)
            };
            Some(
                Violation::FrlRateExceeded {
                    requested: config.frl_rate,
                    limit,
                    limit_source,
                }
                .into(),
            )
        } else {
            None
        }
    }
}
/// Checks that the candidate's FRL rate can carry its timing ([`frl_capacity`]).
///
/// Rejects a candidate only when the timing fits under no reading of the public capacity
/// computation ([`Violation::FrlCapacityExceeded`]). A timing that fits under some
/// readings is accepted with [`Warning::FrlCapacityMarginal`]; whether to keep it is the
/// caller's policy. When the capacity cannot be checked (no blanking or pixel clock, DSC)
/// or not fully (an unmodelled audio format, or no audio data without alloc), the
/// candidate is accepted with [`Warning::FrlCapacityUnverified`]. TMDS candidates are
/// not its concern.
///
/// The sink's audio bounds the audio ([`SinkCapabilities::frl_audio`]). In bare `no_std`
/// builds the sink carries none, so the video is checked without audio and each FRL
/// candidate is reported unverified for audio.
pub struct FrlCapacityCheck;

impl FrlCapacityCheck {
    /// The sink's audio bound, or `None` when it is unknown.
    fn sink_audio(sink: &SinkCapabilities) -> Option<FrlAudio<'_>> {
        #[cfg(any(feature = "alloc", feature = "std"))]
        return sink.frl_audio();
        #[cfg(not(any(feature = "alloc", feature = "std")))]
        {
            let _ = sink;
            None
        }
    }

    /// The capacity report this rule bases its outcome on, with every intermediate
    /// value: `None` for TMDS candidates, `Err` when the capacity cannot be checked.
    ///
    /// The rule's violation and warnings carry only what explains the outcome (the
    /// shortfall, the deciding assumptions), to keep them small; this gives the rest, and
    /// is deterministic, so it reproduces exactly what the rule saw.
    pub fn report(
        sink: &SinkCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<Result<FrlCapacityReport, FrlUnverified>> {
        if config.frl_rate == HdmiForumFrl::NotSupported {
            return None;
        }
        if config.dsc_enabled {
            return Some(Err(FrlUnverified::Dsc));
        }
        // Unknown audio (`None`, or no alloc): the video is checked without audio, and
        // `warnings` reports it.
        let audio = Self::sink_audio(sink).unwrap_or(FrlAudio::None);
        Some(
            FrlCapacityInput::from_mode(
                config.mode,
                config.frl_rate,
                config.color_encoding,
                config.bit_depth,
                audio,
            )
            .map(|input| frl_capacity(&input)),
        )
    }
}

impl<V: Diagnostic + From<Violation>> ConstraintRule<V> for FrlCapacityCheck {
    fn display_name(&self) -> &'static str {
        "frl_capacity"
    }

    fn check(
        &self,
        sink: &SinkCapabilities,
        _source: &SourceCapabilities,
        _cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
    ) -> Option<V> {
        let report = Self::report(sink, config)?.ok()?;
        if report.verdict() != FrlVerdict::Exceeded {
            return None;
        }
        let shortfall = report.strict.result.err()?;
        Some(
            Violation::FrlCapacityExceeded {
                rate: config.frl_rate,
                shortfall,
            }
            .into(),
        )
    }

    fn warnings(
        &self,
        sink: &SinkCapabilities,
        _source: &SourceCapabilities,
        _cable: &CableCapabilities,
        config: &CandidateConfig<'_>,
        warn: &mut dyn FnMut(Warning),
    ) {
        let rate = config.frl_rate;
        let report = match Self::report(sink, config) {
            None => return,
            Some(Err(reason)) => {
                warn(Warning::FrlCapacityUnverified { rate, reason });
                return;
            }
            Some(Ok(report)) => report,
        };
        if Self::sink_audio(sink).is_none() {
            warn(Warning::FrlCapacityUnverified {
                rate,
                reason: FrlUnverified::AudioUnknown,
            });
        }
        #[cfg(any(feature = "alloc", feature = "std"))]
        for &audio in &sink.unmodelled_audio {
            warn(Warning::FrlCapacityUnverified {
                rate,
                reason: FrlUnverified::UnmodelledAudio(audio),
            });
        }
        if let (FrlVerdict::Marginal, Err(shortfall)) = (report.verdict(), report.strict.result) {
            warn(Warning::FrlCapacityMarginal {
                rate,
                deciding: report.deciding,
                shortfall,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::rule::ConstraintRule;
    use crate::output::warning::Violation;
    use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};
    use display_types::VideoMode;
    use display_types::cea861::{HdmiForumFrl, HdmiForumSinkCap};

    fn mode() -> VideoMode {
        VideoMode::new(3840, 2160, 60u16, false)
    }

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

    fn sink_with_frl(max_frl_rate: HdmiForumFrl) -> SinkCapabilities {
        SinkCapabilities {
            hdmi_forum: Some(hf_sink(max_frl_rate)),
            ..Default::default()
        }
    }

    /// Sink whose non-DSC FRL ceiling is `max_frl_rate` but whose DSC transport
    /// ceiling is the lower `dsc_frl_rate`.
    fn sink_with_dsc_frl(
        max_frl_rate: HdmiForumFrl,
        dsc_frl_rate: HdmiForumFrl,
    ) -> SinkCapabilities {
        use display_types::cea861::{HdmiDscMaxSlices, HdmiForumDsc};
        let dsc = HdmiForumDsc::new(
            true,
            false,
            false,
            false,
            false,
            false,
            false,
            dsc_frl_rate,
            HdmiDscMaxSlices::Slices4,
            0,
        );
        let mut hf = hf_sink(max_frl_rate);
        // hf_sink returns a non_exhaustive struct via ::new, so reassign through the field.
        // HdmiForumSinkCap::new takes dsc as its last argument; rebuild with DSC section.
        hf = HdmiForumSinkCap::new(
            hf.version,
            hf.max_tmds_rate_mhz,
            hf.scdc_present,
            hf.rr_capable,
            hf.cable_status,
            hf.ccbpci,
            hf.lte_340mcsc_scramble,
            hf.independent_view_3d,
            hf.dual_view_3d,
            hf.osd_disparity_3d,
            hf.max_frl_rate,
            hf.uhd_vic,
            hf.dc_48bit_420,
            hf.dc_36bit_420,
            hf.dc_30bit_420,
            hf.fapa_end_extended,
            hf.qms,
            hf.m_delta,
            hf.fva,
            hf.allm,
            hf.fapa_start_location,
            hf.neg_mvrr,
            hf.vrr_min_hz,
            hf.vrr_max_hz,
            Some(dsc),
        );
        SinkCapabilities {
            hdmi_forum: Some(hf),
            ..Default::default()
        }
    }

    fn source_with_frl(max_frl_rate: HdmiForumFrl) -> SourceCapabilities {
        SourceCapabilities {
            max_frl_rate,
            ..Default::default()
        }
    }

    fn cable_with_frl(max_frl_rate: HdmiForumFrl) -> CableCapabilities {
        CableCapabilities {
            max_frl_rate,
            ..CableCapabilities::unconstrained()
        }
    }

    fn check(
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        frl_rate: HdmiForumFrl,
    ) -> Option<Violation> {
        check_dsc(sink, source, cable, frl_rate, false)
    }

    fn check_dsc(
        sink: &SinkCapabilities,
        source: &SourceCapabilities,
        cable: &CableCapabilities,
        frl_rate: HdmiForumFrl,
        dsc_enabled: bool,
    ) -> Option<Violation> {
        let m = mode();
        ConstraintRule::<Violation>::check(
            &FrlCeilingCheck,
            sink,
            source,
            cable,
            &CandidateConfig {
                mode: &m,
                color_encoding: display_types::ColorFormat::Rgb444,
                bit_depth: display_types::ColorBitDepth::Depth8,
                frl_rate,
                dsc_enabled,
            },
        )
    }

    #[test]
    fn tmds_candidate_skips_check() {
        // NotSupported means TMDS transport; FRL ceiling is irrelevant.
        let sink = SinkCapabilities::default(); // no HF-SCDB → ceiling would be 0
        assert!(
            check(
                &sink,
                &SourceCapabilities::default(),
                &CableCapabilities::default(),
                HdmiForumFrl::NotSupported
            )
            .is_none()
        );
    }

    #[test]
    fn frl_within_all_ceilings_passes() {
        let sink = sink_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        assert!(
            check(
                &sink,
                &source,
                &CableCapabilities::unconstrained(),
                HdmiForumFrl::Rate6Gbps4Lanes
            )
            .is_none()
        );
    }

    #[test]
    fn frl_exceeds_sink_ceiling_rejected() {
        let sink = sink_with_frl(HdmiForumFrl::Rate6Gbps3Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        assert!(matches!(
            check(
                &sink,
                &source,
                &CableCapabilities::unconstrained(),
                HdmiForumFrl::Rate8Gbps4Lanes
            ),
            Some(Violation::FrlRateExceeded { .. })
        ));
    }

    #[test]
    fn frl_exceeds_source_ceiling_rejected() {
        let sink = sink_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate6Gbps3Lanes);
        assert!(matches!(
            check(
                &sink,
                &source,
                &CableCapabilities::unconstrained(),
                HdmiForumFrl::Rate8Gbps4Lanes
            ),
            Some(Violation::FrlRateExceeded { .. })
        ));
    }

    #[test]
    fn frl_exceeds_cable_ceiling_rejected() {
        let sink = sink_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let cable = cable_with_frl(HdmiForumFrl::Rate6Gbps4Lanes);
        assert!(matches!(
            check(&sink, &source, &cable, HdmiForumFrl::Rate10Gbps4Lanes),
            Some(Violation::FrlRateExceeded { .. })
        ));
    }

    #[test]
    fn no_hf_scdb_rejects_any_frl_request() {
        // Sink without HF-SCDB has effective FRL ceiling of 0.
        let sink = SinkCapabilities::default();
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        assert!(matches!(
            check(
                &sink,
                &source,
                &CableCapabilities::unconstrained(),
                HdmiForumFrl::Rate3Gbps3Lanes
            ),
            Some(Violation::FrlRateExceeded { .. })
        ));
    }

    #[test]
    fn cable_is_binding_constraint() {
        // Sink and source support 12G; cable only supports 6G 4-lane.
        // A request for 8G must be rejected.
        let sink = sink_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let cable = cable_with_frl(HdmiForumFrl::Rate6Gbps4Lanes);
        assert!(matches!(
            check(&sink, &source, &cable, HdmiForumFrl::Rate8Gbps4Lanes),
            Some(Violation::FrlRateExceeded { .. })
        ));
        // But 6G 4-lane itself is fine.
        assert!(check(&sink, &source, &cable, HdmiForumFrl::Rate6Gbps4Lanes).is_none());
    }

    // --- DSC FRL ceiling tests ---

    #[test]
    fn dsc_uses_dsc_frl_ceiling_not_main() {
        // Sink supports 12G non-DSC but only 6G 3-lane for DSC transport.
        // Non-DSC request for 8G passes; DSC request for 8G must be rejected.
        let sink = sink_with_dsc_frl(
            HdmiForumFrl::Rate12Gbps4Lanes,
            HdmiForumFrl::Rate6Gbps3Lanes,
        );
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        assert!(
            check(
                &sink,
                &source,
                &CableCapabilities::unconstrained(),
                HdmiForumFrl::Rate8Gbps4Lanes
            )
            .is_none()
        );
        assert!(matches!(
            check_dsc(
                &sink,
                &source,
                &CableCapabilities::unconstrained(),
                HdmiForumFrl::Rate8Gbps4Lanes,
                true
            ),
            Some(Violation::FrlRateExceeded { .. })
        ));
    }

    #[test]
    fn dsc_within_dsc_frl_ceiling_passes() {
        // Requesting a rate at or below the DSC ceiling passes.
        let sink = sink_with_dsc_frl(
            HdmiForumFrl::Rate12Gbps4Lanes,
            HdmiForumFrl::Rate6Gbps3Lanes,
        );
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        assert!(
            check_dsc(
                &sink,
                &source,
                &CableCapabilities::unconstrained(),
                HdmiForumFrl::Rate6Gbps3Lanes,
                true
            )
            .is_none()
        );
    }

    // --- LimitSource field tests ---

    #[test]
    fn limit_source_is_sink_when_sink_is_binding() {
        use crate::output::warning::LimitSource;
        let sink = sink_with_frl(HdmiForumFrl::Rate6Gbps3Lanes); // tier 2
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes); // tier 6
        let result = check(
            &sink,
            &source,
            &CableCapabilities::unconstrained(),
            HdmiForumFrl::Rate8Gbps4Lanes,
        );
        assert!(matches!(
            result,
            Some(Violation::FrlRateExceeded {
                limit_source: LimitSource::Sink,
                ..
            })
        ));
    }

    #[test]
    fn limit_source_is_source_when_source_is_binding() {
        use crate::output::warning::LimitSource;
        let sink = sink_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate6Gbps3Lanes); // tier 2, tighter
        let result = check(
            &sink,
            &source,
            &CableCapabilities::unconstrained(),
            HdmiForumFrl::Rate8Gbps4Lanes,
        );
        assert!(matches!(
            result,
            Some(Violation::FrlRateExceeded {
                limit_source: LimitSource::Source,
                ..
            })
        ));
    }

    #[test]
    fn limit_source_is_cable_when_cable_is_binding() {
        use crate::output::warning::LimitSource;
        let sink = sink_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let cable = cable_with_frl(HdmiForumFrl::Rate6Gbps3Lanes); // tier 2, tightest
        let result = check(&sink, &source, &cable, HdmiForumFrl::Rate8Gbps4Lanes);
        assert!(matches!(
            result,
            Some(Violation::FrlRateExceeded {
                limit_source: LimitSource::Cable,
                ..
            })
        ));
    }

    #[test]
    fn frl_rate_exceeded_carries_correct_rates() {
        let sink = sink_with_frl(HdmiForumFrl::Rate6Gbps3Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let result = check(
            &sink,
            &source,
            &CableCapabilities::unconstrained(),
            HdmiForumFrl::Rate8Gbps4Lanes,
        );
        assert!(matches!(
            result,
            Some(Violation::FrlRateExceeded {
                requested: HdmiForumFrl::Rate8Gbps4Lanes,
                limit: HdmiForumFrl::Rate6Gbps3Lanes,
                ..
            })
        ));
    }

    #[test]
    fn dsc_enabled_no_dsc_section_rejected() {
        // Sink has FRL support but no DSC section — DSC FRL ceiling is 0.
        let sink = sink_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        let source = source_with_frl(HdmiForumFrl::Rate12Gbps4Lanes);
        assert!(matches!(
            check_dsc(
                &sink,
                &source,
                &CableCapabilities::unconstrained(),
                HdmiForumFrl::Rate3Gbps3Lanes,
                true
            ),
            Some(Violation::FrlRateExceeded { .. })
        ));
    }
}

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
mod capacity_tests {
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
        let rejected: Result<Vec<Warning>, Vec<TaggedViolation<Violation>>> =
            engine.check(&sink, &source, &cable, &config);
        assert!(rejected.is_err());

        // A rule list with only the capacity check accepts it, with the warning.
        static ONLY_CAPACITY: crate::engine::CheckList<Violation> = &[&FrlCapacityCheck];
        let engine = DefaultConstraintEngine::with_checks(ONLY_CAPACITY);
        let warnings = engine.check(&sink, &source, &cable, &config).unwrap();
        assert!(matches!(
            warnings[..],
            [Warning::FrlCapacityMarginal { .. }]
        ));
    }
}
