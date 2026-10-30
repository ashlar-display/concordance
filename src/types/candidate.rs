//! Candidate configuration type used as input to the constraint engine.

use display_types::cea861::HdmiForumFrl;
use display_types::{ColorBitDepth, ColorFormat, VideoMode};

/// A specific configuration candidate to be evaluated by the constraint engine.
///
/// Produced by the [`CandidateEnumerator`][crate::enumerator::CandidateEnumerator] during
/// full pipeline runs, or supplied directly by the caller to [`is_config_viable`][crate::is_config_viable].
///
/// Borrows the [`VideoMode`] from the sink's mode list rather than owning a copy —
/// most candidates are rejected, so deferring the copy to acceptance time (in
/// [`NegotiatedConfig`][crate::output::config::NegotiatedConfig]) avoids redundant
/// allocation. Pixel clock information is accessed via `config.mode.pixel_clock_khz`.
///
/// # Serde
///
/// Only `Serialize` is derived (behind the `serde` feature flag). Deserialization is not
/// supported for borrowed types; construct `CandidateConfig` programmatically.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateConfig<'a> {
    /// The video mode being evaluated.
    pub mode: &'a VideoMode,

    /// Color encoding format.
    pub color_encoding: ColorFormat,

    /// Color bit depth per channel.
    pub bit_depth: ColorBitDepth,

    /// FRL rate tier, or [`HdmiForumFrl::NotSupported`] for TMDS transport.
    pub frl_rate: HdmiForumFrl,

    /// Whether Display Stream Compression is applied.
    pub dsc_enabled: bool,
}

impl<'a> CandidateConfig<'a> {
    /// Constructs a `CandidateConfig`.
    pub fn new(
        mode: &'a VideoMode,
        color_encoding: ColorFormat,
        bit_depth: ColorBitDepth,
        frl_rate: HdmiForumFrl,
        dsc_enabled: bool,
    ) -> Self {
        Self {
            mode,
            color_encoding,
            bit_depth,
            frl_rate,
            dsc_enabled,
        }
    }
}

#[cfg(test)]
#[path = "candidate_tests.rs"]
mod tests;
