//! Source (GPU / transmitter) capability input types.

use display_types::cea861::HdmiForumFrl;

bitflags::bitflags! {
    /// Source quirk flags that relax specific constraint checks.
    ///
    /// Flags are defined by concordance and correspond to known cases where a
    /// source or its driver diverges from the HDMI specification in a predictable
    /// way. Pass `QuirkFlags::empty()` (the default) when no quirks apply.
    ///
    /// Callers cannot define their own bits; use
    /// [`NegotiatorBuilder::with_extra_rule`][crate::NegotiatorBuilder::with_extra_rule]
    /// to express constraints that fall outside the built-in rule set.
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct QuirkFlags: u32 {
        /// Ignore the sink's declared vertical refresh rate range.
        ///
        /// Some TVs and variable-rate panels declare a narrow `min_v_rate`/`max_v_rate`
        /// window in their EDID range limits descriptor that does not reflect their true
        /// operating range. Setting this flag suppresses
        /// [`Violation::RefreshRateOutOfRange`][crate::output::warning::Violation::RefreshRateOutOfRange]
        /// so that modes outside the declared range are still considered.
        const IGNORE_REFRESH_RATE_RANGE = 1 << 0;
    }
}

/// Display Stream Compression capabilities of the source.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DscCapabilities {
    /// Supports VESA DSC 1.2a.
    pub dsc_1p2: bool,
    /// Maximum number of slices the source encoder can produce.
    pub max_slices: u8,
    /// Maximum bits per pixel (in 1/16 increments) the source can encode.
    pub max_bpp_x16: u16,
}

/// Capabilities of the source (GPU or other HDMI transmitter).
///
/// The caller fills this struct manually. Populating it from actual GPU hardware
/// is the concern of the source capability discovery library in the integration
/// layer, not this library.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCapabilities {
    /// Maximum TMDS clock rate the source can output, in kHz.
    pub max_tmds_clock: u32,

    /// Maximum FRL rate the source supports.
    ///
    /// FRL rates are cumulative — this implies support for all lower tiers.
    /// [`HdmiForumFrl::NotSupported`] indicates a TMDS-only source.
    pub max_frl_rate: HdmiForumFrl,

    /// Source Display Stream Compression capabilities, if supported.
    pub dsc: Option<DscCapabilities>,

    /// Vendor-specific quirk overrides.
    pub quirks: QuirkFlags,
}

impl Default for SourceCapabilities {
    fn default() -> Self {
        Self {
            max_tmds_clock: 0,
            max_frl_rate: HdmiForumFrl::NotSupported,
            dsc: None,
            quirks: QuirkFlags::empty(),
        }
    }
}

impl SourceCapabilities {
    /// Constructs a `SourceCapabilities` with explicit values and no quirks.
    ///
    /// Pass `max_tmds_clock: 0` to indicate no TMDS limit (the source is unconstrained
    /// or operates exclusively over FRL). Pass `dsc: None` for sources without DSC.
    pub const fn new(
        max_tmds_clock: u32,
        max_frl_rate: HdmiForumFrl,
        dsc: Option<DscCapabilities>,
    ) -> Self {
        Self {
            max_tmds_clock,
            max_frl_rate,
            dsc,
            quirks: QuirkFlags::empty(),
        }
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
