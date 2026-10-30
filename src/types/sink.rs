//! Sink (display) capability input type.

use display_types::ColorCapabilities;
use display_types::cea861::{ColorimetryBlock, HdmiForumSinkCap, HdmiVsdb, HdrStaticMetadata};

#[cfg(any(feature = "alloc", feature = "std"))]
use alloc::vec::Vec;
#[cfg(any(feature = "alloc", feature = "std"))]
use display_types::VideoMode;

/// A sorted, deduplicated list of video modes.
///
/// Constructed via [`SupportedModes::from_vec`], which normalises the input on
/// entry so that every downstream consumer — including the enumerator — can rely
/// on the invariant unconditionally.
///
/// Available in `alloc` and `std` tiers only.
#[cfg(any(feature = "alloc", feature = "std"))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SupportedModes(Vec<VideoMode>);

#[cfg(any(feature = "alloc", feature = "std"))]
impl SupportedModes {
    /// Deduplicates `modes`, returning the normalised list and any duplicate
    /// entries that were removed. Insertion order of the first occurrence of
    /// each mode is preserved.
    pub fn from_vec(modes: Vec<VideoMode>) -> (Self, Vec<VideoMode>) {
        let mut seen: Vec<VideoMode> = Vec::with_capacity(modes.len());
        let mut duplicates = Vec::new();
        for mode in modes {
            // Linear scan is O(n²), but EDID mode lists are small (< 100 entries)
            // and `HashSet` is not available in `alloc`-only builds.
            if seen.contains(&mode) {
                duplicates.push(mode);
            } else {
                seen.push(mode);
            }
        }
        (SupportedModes(seen), duplicates)
    }

    /// Returns the modes as a slice.
    pub fn as_slice(&self) -> &[VideoMode] {
        &self.0
    }
}

/// Warning produced by [`sink_capabilities_from_display`] during EDID parsing.
///
/// Returned alongside [`SinkCapabilities`] to surface construction-time anomalies
/// without making them fatal. The list is empty for well-formed EDIDs.
#[cfg(any(feature = "alloc", feature = "std"))]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SinkBuildWarning {
    /// One or more video modes appeared more than once in the EDID and were removed.
    #[error("{} duplicate video mode(s) removed", .0.len())]
    DuplicateModes(Vec<VideoMode>),
    /// A Short Audio Descriptor declares no sample rate, so it bounds no audio load; it is
    /// left out of [`SinkCapabilities::audio_loads`].
    #[error("audio descriptor without a sample rate ignored: {0:?}")]
    AudioDescriptorWithoutSampleRate(display_types::cea861::ShortAudioDescriptor),
}

/// Capabilities of the connected display.
///
/// The caller fills this struct manually, or constructs it from a parsed
/// [`DisplayCapabilities`][display_types::DisplayCapabilities] via
/// [`sink_capabilities_from_display`].
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SinkCapabilities {
    /// Video modes declared by the display.
    ///
    /// Absent in bare `no_std` builds; [`is_config_viable`][crate::is_config_viable]
    /// validates a caller-supplied candidate rather than enumerating one.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub supported_modes: SupportedModes,

    /// Maximum pixel clock in MHz (from EDID range limits descriptor).
    pub max_pixel_clock_mhz: Option<u16>,

    /// Minimum vertical rate in Hz.
    pub min_v_rate: Option<u16>,

    /// Maximum vertical rate in Hz.
    pub max_v_rate: Option<u16>,

    /// Modes that support *only* YCbCr 4:2:0 (from the Y420 Video Data Block).
    ///
    /// Per CTA-861-H §7.5.11, other color encodings are not valid for these modes.
    /// Populated by [`sink_capabilities_from_display`]; defaults to empty when
    /// constructing [`SinkCapabilities`] manually.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub ycbcr420_exclusive_modes: SupportedModes,

    /// Modes that *also* support YCbCr 4:2:0 (from the Y420 Capability Map Data Block).
    ///
    /// Other encodings remain valid for these modes; 4:2:0 is an additional option.
    /// Populated by [`sink_capabilities_from_display`]; defaults to empty when
    /// constructing [`SinkCapabilities`] manually.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub ycbcr420_capable_modes: SupportedModes,

    /// Supported color format and bit-depth combinations.
    ///
    /// Derived from the EDID base block color encoding field, the Deep Color flags
    /// in the HDMI VSDB and HF-SCDB, and the CEA/CTA extension YCbCr 4:2:0 blocks.
    /// The integration layer is responsible for assembling this from those sources.
    pub color_capabilities: ColorCapabilities,

    /// HDMI 1.x capabilities (from HDMI VSDB; `None` if not present).
    pub hdmi_vsdb: Option<HdmiVsdb>,

    /// HDMI 2.1 capabilities (from HF-SCDB; `None` for pre-HDMI-2.1 sinks).
    pub hdmi_forum: Option<HdmiForumSinkCap>,

    /// HDR static metadata capabilities.
    pub hdr_static: Option<HdrStaticMetadata>,

    /// Colorimetry standards supported.
    pub colorimetry: Option<ColorimetryBlock>,

    /// The heaviest audio load a source may send for each format the sink accepts, or
    /// `None` when the sink's audio is unknown.
    ///
    /// [`sink_capabilities_from_display`] always fills it: one load per Short Audio
    /// Descriptor of the CTA Audio Data Block, mapped by
    /// [`AudioLoad::from_descriptor`][crate::frl::AudioLoad::from_descriptor], plus
    /// [`AudioLoad::BASIC`][crate::frl::AudioLoad::BASIC] when the sink declares basic audio;
    /// `Some` and empty when it declares no audio. A sink built by hand starts with `None`
    /// (from `Default`, and when deserializing data without the field), which the FRL
    /// capacity check reports as unknown audio rather than counting none.
    ///
    /// Absent in bare `no_std` builds: there the sink's audio is always unknown, and FRL
    /// candidates are reported unverified for audio (see [`FrlAudio`][crate::frl::FrlAudio]).
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub audio_loads: Option<Vec<crate::frl::AudioLoad>>,

    /// Audio the sink accepts whose packet load no public implementation models: One Bit
    /// Audio, DST, extended and reserved formats, HDMI Multi-Stream Audio and HDMI 3D Audio.
    /// It cannot be bounded; the FRL capacity check reports it. Absent in bare `no_std`
    /// builds.
    #[cfg(any(feature = "alloc", feature = "std"))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub unmodelled_audio: Vec<crate::frl::UnmodelledAudio>,
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl SinkCapabilities {
    /// The sink's audio as a bound for the FRL capacity computation:
    /// [`FrlAudio::AtMost`][crate::frl::FrlAudio::AtMost] over
    /// [`audio_loads`][Self::audio_loads], or `None` when the audio is unknown.
    pub fn frl_audio(&self) -> Option<crate::frl::FrlAudio<'_>> {
        self.audio_loads
            .as_deref()
            .map(crate::frl::FrlAudio::AtMost)
    }
}

/// Derives [`SinkCapabilities`] from a parsed [`DisplayCapabilities`][display_types::DisplayCapabilities].
///
/// Extracts all fields that can be determined from the parsed EDID data, including
/// the CEA-861 extension block (stored at extension tag `0x02`). Fields that cannot
/// be derived from EDID data — specifically [`SinkCapabilities::hdmi_vsdb`]'s quirk
/// overrides — default to their zero values and can be set after the call.
///
/// The HDMI Forum Sink Capability Data Block is preferred over the HDMI Forum
/// Vendor-Specific Data Block when both are present (the former is the HDMI 2.1
/// mechanism; the latter is the older HDMI 2.0 mechanism for the same data).
#[cfg(any(feature = "alloc", feature = "std"))]
pub fn sink_capabilities_from_display(
    caps: &display_types::DisplayCapabilities,
) -> (SinkCapabilities, Vec<SinkBuildWarning>) {
    use display_types::cea861::{Cea861Capabilities, vic_to_mode};
    use display_types::{ColorBitDepth, color_capabilities_from_edid};

    let cea = caps.get_extension_data::<Cea861Capabilities>(0x02);

    let hdmi_vsdb = cea.and_then(|c| c.hdmi_vsdb.as_ref());
    // Prefer HF-SCDB (HDMI 2.1) over HF-VSDB (HDMI 2.0); both carry the same structure.
    let hdmi_forum = cea.and_then(|c| c.hf_scdb.as_ref().or(c.hf_vsdb.as_ref()));

    let mut color_capabilities = color_capabilities_from_edid(
        caps.digital_color_encoding,
        caps.color_bit_depth,
        hdmi_vsdb,
        hdmi_forum,
    );

    // If the YCbCr 4:2:0 Video Data Block or Capability Map Data Block is present,
    // at least one mode supports YCbCr 4:2:0 at 8 bpc — add baseline support.
    let has_y420_vdb =
        cea.is_some_and(|c| !c.y420_vics.is_empty() || !c.y420_capability_map.is_empty());
    if has_y420_vdb {
        color_capabilities.ycbcr420 = color_capabilities.ycbcr420.with(ColorBitDepth::Depth8);
    }

    // Modes that support *only* YCbCr 4:2:0 — direct VIC lookup from the Y420 VDB.
    let (ycbcr420_exclusive_modes, _) = SupportedModes::from_vec(
        cea.map(|c| {
            c.y420_vics
                .iter()
                .filter_map(|&vic| vic_to_mode(vic))
                .collect()
        })
        .unwrap_or_default(),
    );

    // Modes that *also* support YCbCr 4:2:0 — bitmap × SVD list from the Y420 CMB.
    // Bit N of the bitmap corresponds to vics[N]; if set, that VIC supports 4:2:0.
    let (ycbcr420_capable_modes, _) = SupportedModes::from_vec(
        cea.map(|c| {
            c.y420_capability_map
                .iter()
                .enumerate()
                .flat_map(|(byte_idx, &byte)| {
                    (0u8..8).filter_map(move |bit| {
                        if byte & (1 << bit) != 0 {
                            let vic_idx = byte_idx * 8 + bit as usize;
                            c.vics.get(vic_idx).and_then(|&(vic, _)| vic_to_mode(vic))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default(),
    );

    let (supported_modes, duplicates) = SupportedModes::from_vec(caps.supported_modes.clone());
    let mut warnings = Vec::new();
    if !duplicates.is_empty() {
        warnings.push(SinkBuildWarning::DuplicateModes(duplicates));
    }

    // Audio: every descriptor of the CTA Audio Data Block, and basic audio. The HDMI Audio
    // Data Block's multi-stream and 3D audio have no packet model: reported, not mapped.
    // Without a CTA extension the sink declares no audio (`Some`, empty).
    use crate::frl::{AudioLoad, AudioMapping, UnmodelledAudio};
    let mut audio_loads = Vec::new();
    let mut unmodelled_audio = Vec::new();
    if let Some(c) = cea {
        if c.flags
            .contains(display_types::cea861::Cea861Flags::BASIC_AUDIO)
        {
            audio_loads.push(AudioLoad::BASIC);
        }
        for sad in &c.audio_descriptors {
            match AudioLoad::from_descriptor(sad) {
                AudioMapping::Load(load) => {
                    if !audio_loads.contains(&load) {
                        audio_loads.push(load);
                    }
                }
                AudioMapping::Unmodelled(audio) => {
                    if !unmodelled_audio.contains(&audio) {
                        unmodelled_audio.push(audio);
                    }
                }
                AudioMapping::NoSampleRate => warnings.push(
                    SinkBuildWarning::AudioDescriptorWithoutSampleRate(sad.clone()),
                ),
            }
        }
        if let Some(b) = &c.hdmi_audio {
            if b.multi_stream() {
                unmodelled_audio.push(UnmodelledAudio::MultiStream {
                    max_streams: b.max_streams,
                });
            }
            if !b.audio_3d_descriptors.is_empty() {
                unmodelled_audio.push(UnmodelledAudio::Audio3d {
                    descriptors: u8::try_from(b.audio_3d_descriptors.len()).unwrap_or(u8::MAX),
                });
            }
        }
    }

    (
        SinkCapabilities {
            supported_modes,
            ycbcr420_exclusive_modes,
            ycbcr420_capable_modes,
            max_pixel_clock_mhz: caps.max_pixel_clock_mhz,
            min_v_rate: caps.min_v_rate,
            max_v_rate: caps.max_v_rate,
            color_capabilities,
            hdmi_vsdb: cea.and_then(|c| c.hdmi_vsdb.clone()),
            hdmi_forum: cea.and_then(|c| c.hf_scdb.clone().or_else(|| c.hf_vsdb.clone())),
            hdr_static: cea.and_then(|c| c.hdr_static_metadata.clone()),
            colorimetry: cea.and_then(|c| c.colorimetry),
            audio_loads: Some(audio_loads),
            unmodelled_audio,
        },
        warnings,
    )
}

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
#[path = "sink_tests.rs"]
mod tests;
