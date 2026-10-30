//! Cable capability input types.

use display_types::cea861::HdmiForumFrl;

/// HDMI specification version declared by the cable.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HdmiSpec {
    /// HDMI 1.4 cable.
    Hdmi14,
    /// HDMI 2.0 cable.
    Hdmi20,
    /// HDMI 2.1 (48G) cable.
    Hdmi21,
}

/// Capabilities of the HDMI cable.
///
/// The caller fills this struct manually. Populating it from the cable type marker
/// read from the sink EDID, or from a user-supplied override, is the concern of the
/// integration layer, not this library.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CableCapabilities {
    /// HDMI specification version declared by the cable.
    pub hdmi_spec: HdmiSpec,

    /// Maximum FRL rate the cable can carry.
    ///
    /// [`HdmiForumFrl::NotSupported`] indicates a TMDS-only cable.
    /// A cable may be the binding constraint even when both source and sink are HDMI 2.1.
    pub max_frl_rate: HdmiForumFrl,

    /// Maximum TMDS clock rate the cable can carry, in kHz.
    pub max_tmds_clock: u32,
}

impl Default for CableCapabilities {
    fn default() -> Self {
        Self::unconstrained()
    }
}

impl CableCapabilities {
    /// Constructs a `CableCapabilities` with explicit values.
    pub const fn new(hdmi_spec: HdmiSpec, max_frl_rate: HdmiForumFrl, max_tmds_clock: u32) -> Self {
        Self {
            hdmi_spec,
            max_frl_rate,
            max_tmds_clock,
        }
    }

    /// Returns a cable with no constraints — equivalent to assuming source and sink limits only.
    ///
    /// Useful for callers that have no cable information and wish to fall back to
    /// the optimistic assumption.
    pub const fn unconstrained() -> Self {
        Self {
            hdmi_spec: HdmiSpec::Hdmi21,
            max_frl_rate: HdmiForumFrl::Rate12Gbps4Lanes,
            max_tmds_clock: u32::MAX,
        }
    }
}

#[cfg(test)]
#[path = "cable_tests.rs"]
mod tests;
