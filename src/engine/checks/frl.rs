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
#[path = "frl_tests.rs"]
mod tests;

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
#[path = "frl_capacity_tests.rs"]
mod capacity_tests;
