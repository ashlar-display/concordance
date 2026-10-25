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

    /// The heaviest audio load a source may send for each format the sink accepts: one per
    /// Short Audio Descriptor (CTA Audio Data Block and HDMI Audio Data Block), mapped by
    /// [`AudioLoad::from_descriptor`][crate::frl::AudioLoad::from_descriptor], plus
    /// [`AudioLoad::BASIC`][crate::frl::AudioLoad::BASIC] when the sink declares basic audio.
    /// Empty when the sink accepts no audio. The FRL capacity check bounds the audio with
    /// these ([`frl_audio`][Self::frl_audio]).
    ///
    /// Absent in bare `no_std` builds: there the sink's audio is unknown, and FRL
    /// candidates are reported unverified for audio (see [`FrlAudio`][crate::frl::FrlAudio]).
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub audio_loads: Vec<crate::frl::AudioLoad>,

    /// Audio formats the sink accepts whose packet load no public implementation models
    /// (DST, extended and reserved codes); they cannot be bounded. Absent in bare `no_std`
    /// builds.
    #[cfg(any(feature = "alloc", feature = "std"))]
    pub unmodelled_audio: Vec<display_types::cea861::AudioFormat>,
}

#[cfg(any(feature = "alloc", feature = "std"))]
impl SinkCapabilities {
    /// The sink's audio as a bound for the FRL capacity computation:
    /// [`FrlAudio::AtMost`][crate::frl::FrlAudio::AtMost] over
    /// [`audio_loads`][Self::audio_loads].
    pub fn frl_audio(&self) -> crate::frl::FrlAudio<'_> {
        crate::frl::FrlAudio::AtMost(&self.audio_loads)
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

    // Audio: every descriptor from the CTA and HDMI Audio Data Blocks, and basic audio.
    let mut audio_loads = Vec::new();
    let mut unmodelled_audio = Vec::new();
    if let Some(c) = cea {
        if c.flags
            .contains(display_types::cea861::Cea861Flags::BASIC_AUDIO)
        {
            audio_loads.push(crate::frl::AudioLoad::BASIC);
        }
        let hdmi_audio = c.hdmi_audio.iter().flat_map(|b| b.audio_descriptors.iter());
        for sad in c.audio_descriptors.iter().chain(hdmi_audio) {
            match crate::frl::AudioLoad::from_descriptor(sad) {
                crate::frl::AudioMapping::Load(load) => {
                    if !audio_loads.contains(&load) {
                        audio_loads.push(load);
                    }
                }
                crate::frl::AudioMapping::Unmodelled(format) => {
                    if !unmodelled_audio.contains(&format) {
                        unmodelled_audio.push(format);
                    }
                }
                _ => warnings.push(SinkBuildWarning::AudioDescriptorWithoutSampleRate(
                    sad.clone(),
                )),
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
            audio_loads,
            unmodelled_audio,
        },
        warnings,
    )
}

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
mod tests {
    use super::*;
    use display_types::DisplayCapabilities;
    use display_types::cea861::{
        Cea861Capabilities, Cea861Flags, HdmiForumFrl, HdmiForumSinkCap, HdmiVsdb, HdmiVsdbFlags,
    };
    use display_types::{ColorBitDepth, VideoMode};

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Minimal `HdmiForumSinkCap` identified by its `max_tmds_rate_mhz` for easy
    /// equality checks in tests.
    fn hf_sink(max_tmds_rate_mhz: u16) -> HdmiForumSinkCap {
        HdmiForumSinkCap::new(
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
            HdmiForumFrl::NotSupported,
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

    /// Minimal `HdmiVsdb`.
    fn vsdb() -> HdmiVsdb {
        HdmiVsdb::new(0, HdmiVsdbFlags::empty(), None, None, None, None, None)
    }

    /// Attaches a `Cea861Capabilities` to a `DisplayCapabilities` at tag `0x02`.
    fn with_cea(caps: &mut DisplayCapabilities, cea: Cea861Capabilities) {
        caps.set_extension_data(0x02, cea);
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    /// With no CEA extension block attached, the function returns defaults with no warnings.
    #[test]
    fn no_cea_extension_yields_defaults() {
        let caps = DisplayCapabilities::default();
        let (sink, warnings) = sink_capabilities_from_display(&caps);
        assert!(warnings.is_empty());
        assert!(sink.hdmi_forum.is_none());
        assert!(sink.hdmi_vsdb.is_none());
        assert!(sink.hdr_static.is_none());
        assert!(sink.colorimetry.is_none());
    }

    /// Scalar range-limit fields are copied directly from the input.
    #[test]
    fn scalar_fields_are_copied() {
        let mut caps = DisplayCapabilities::default();
        caps.max_pixel_clock_mhz = Some(300);
        caps.min_v_rate = Some(24);
        caps.max_v_rate = Some(144);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert_eq!(sink.max_pixel_clock_mhz, Some(300));
        assert_eq!(sink.min_v_rate, Some(24));
        assert_eq!(sink.max_v_rate, Some(144));
    }

    /// Unique modes are accepted without any warning.
    #[test]
    fn unique_modes_produce_no_warning() {
        let mut caps = DisplayCapabilities::default();
        caps.supported_modes = alloc::vec![
            VideoMode::new(1920, 1080, 60u16, false),
            VideoMode::new(3840, 2160, 60u16, false),
        ];
        let (sink, warnings) = sink_capabilities_from_display(&caps);
        assert!(warnings.is_empty());
        assert_eq!(sink.supported_modes.as_slice().len(), 2);
    }

    /// A duplicate mode in the input triggers a `DuplicateModes` warning and the
    /// deduplicated list contains only the first occurrence.
    #[test]
    fn duplicate_modes_produce_warning() {
        let mode = VideoMode::new(1920, 1080, 60u16, false);
        let mut caps = DisplayCapabilities::default();
        caps.supported_modes = alloc::vec![mode.clone(), mode.clone()];
        let (sink, warnings) = sink_capabilities_from_display(&caps);
        assert_eq!(sink.supported_modes.as_slice().len(), 1);
        assert!(
            warnings
                .iter()
                .any(|w| matches!(w, SinkBuildWarning::DuplicateModes(_))),
            "expected a DuplicateModes warning"
        );
    }

    /// When the CEA block carries an HF-SCDB, it is used as `hdmi_forum`.
    #[test]
    fn cea_hf_scdb_populates_hdmi_forum() {
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.hf_scdb = Some(hf_sink(600));
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert!(sink.hdmi_forum.is_some());
        assert_eq!(sink.hdmi_forum.as_ref().unwrap().max_tmds_rate_mhz, 600);
    }

    /// When only HF-VSDB is present (no HF-SCDB), it is used as `hdmi_forum`.
    #[test]
    fn cea_hf_vsdb_used_when_scdb_absent() {
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.hf_vsdb = Some(hf_sink(340));
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert!(sink.hdmi_forum.is_some());
        assert_eq!(sink.hdmi_forum.as_ref().unwrap().max_tmds_rate_mhz, 340);
    }

    /// When both HF-SCDB and HF-VSDB are present, HF-SCDB (HDMI 2.1) takes precedence.
    #[test]
    fn hf_scdb_preferred_over_hf_vsdb() {
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.hf_scdb = Some(hf_sink(600)); // HDMI 2.1 — should win
        cea.hf_vsdb = Some(hf_sink(340)); // HDMI 2.0
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert_eq!(
            sink.hdmi_forum.as_ref().unwrap().max_tmds_rate_mhz,
            600,
            "HF-SCDB must be preferred over HF-VSDB"
        );
    }

    /// An HDMI 1.x VSDB in the CEA block is copied to `hdmi_vsdb`.
    #[test]
    fn cea_hdmi_vsdb_is_propagated() {
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.hdmi_vsdb = Some(vsdb());
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert!(sink.hdmi_vsdb.is_some());
    }

    /// A non-empty `y420_vics` list causes YCbCr 4:2:0 at 8 bpc to be added to
    /// the color capabilities, even when no HF-SCDB deep-color flags are set.
    #[test]
    fn y420_vics_adds_baseline_ycbcr420() {
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.y420_vics = alloc::vec![96]; // 4K@60 VIC
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert!(
            sink.color_capabilities
                .ycbcr420
                .supports(ColorBitDepth::Depth8),
            "y420_vics must add YCbCr 4:2:0 8 bpc baseline"
        );
    }

    /// A non-empty `y420_capability_map` also triggers the baseline YCbCr 4:2:0 addition.
    #[test]
    fn y420_capability_map_adds_baseline_ycbcr420() {
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.y420_capability_map = alloc::vec![0xFF];
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert!(
            sink.color_capabilities
                .ycbcr420
                .supports(ColorBitDepth::Depth8),
            "y420_capability_map must add YCbCr 4:2:0 8 bpc baseline"
        );
    }

    /// HDR static metadata in the CEA block is propagated to `hdr_static`.
    #[test]
    fn hdr_static_metadata_is_propagated() {
        use display_types::cea861::{HdrEotf, HdrStaticMetadata};
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.hdr_static_metadata = Some(HdrStaticMetadata::new(
            HdrEotf::empty(),
            0,
            None,
            None,
            None,
        ));
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert!(sink.hdr_static.is_some());
    }

    /// VICs in `y420_vics` are resolved to `VideoMode`s and stored in
    /// `ycbcr420_exclusive_modes`. VIC 97 is 4K@60 Hz.
    #[test]
    fn y420_vics_populates_exclusive_modes() {
        use display_types::cea861::vic_to_mode;
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.y420_vics = alloc::vec![97]; // 4K@60 Hz
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        let expected = vic_to_mode(97).unwrap();
        assert!(
            sink.ycbcr420_exclusive_modes.as_slice().contains(&expected),
            "VIC 97 must appear in ycbcr420_exclusive_modes"
        );
        assert!(
            sink.ycbcr420_capable_modes.as_slice().is_empty(),
            "ycbcr420_capable_modes must be empty when only y420_vics is set"
        );
    }

    /// A set bit N in `y420_capability_map` resolves through `vics[N]` to a
    /// `VideoMode` stored in `ycbcr420_capable_modes`.
    #[test]
    fn y420_capability_map_populates_capable_modes() {
        use display_types::cea861::vic_to_mode;
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        // Put VIC 16 (1080p@60) at index 0 in the SVD list.
        cea.vics = alloc::vec![(16, true)];
        // Bit 0 of byte 0 set → vics[0] = VIC 16 supports 4:2:0 additionally.
        cea.y420_capability_map = alloc::vec![0b0000_0001];
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        let expected = vic_to_mode(16).unwrap();
        assert!(
            sink.ycbcr420_capable_modes.as_slice().contains(&expected),
            "VIC 16 must appear in ycbcr420_capable_modes"
        );
        assert!(
            sink.ycbcr420_exclusive_modes.as_slice().is_empty(),
            "ycbcr420_exclusive_modes must be empty when only y420_capability_map is set"
        );
    }

    /// A clear bit in `y420_capability_map` must not produce an entry.
    #[test]
    fn y420_capability_map_clear_bit_excluded() {
        use display_types::cea861::vic_to_mode;
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.vics = alloc::vec![(16, true), (97, false)];
        // Bit 0 clear, bit 1 set → only vics[1] = VIC 97 is 4:2:0 capable.
        cea.y420_capability_map = alloc::vec![0b0000_0010];
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        let vic16 = vic_to_mode(16).unwrap();
        let vic97 = vic_to_mode(97).unwrap();
        assert!(
            !sink.ycbcr420_capable_modes.as_slice().contains(&vic16),
            "VIC 16 (bit clear) must not appear"
        );
        assert!(
            sink.ycbcr420_capable_modes.as_slice().contains(&vic97),
            "VIC 97 (bit set) must appear"
        );
    }

    /// Colorimetry data in the CEA block is propagated to `colorimetry`.
    #[test]
    fn colorimetry_is_propagated() {
        use display_types::cea861::{ColorimetryBlock, ColorimetryFlags};
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.colorimetry = Some(ColorimetryBlock::new(ColorimetryFlags::empty(), 0));
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert!(sink.colorimetry.is_some());
    }

    /// `color_capabilities` reflects the VSDB deep-color flags when a VSDB is present.
    #[test]
    fn vsdb_deep_color_flags_populate_color_capabilities() {
        let mut caps = DisplayCapabilities::default();
        caps.digital_color_encoding = Some(display_types::DigitalColorEncoding::Rgb444);
        let mut cea = Cea861Capabilities::new(Cea861Flags::empty());
        cea.hdmi_vsdb = Some(HdmiVsdb::new(
            0,
            HdmiVsdbFlags::DC_30BIT | HdmiVsdbFlags::DC_36BIT,
            None,
            None,
            None,
            None,
            None,
        ));
        with_cea(&mut caps, cea);
        let (sink, _) = sink_capabilities_from_display(&caps);
        assert!(
            sink.color_capabilities
                .rgb444
                .supports(ColorBitDepth::Depth10)
        );
        assert!(
            sink.color_capabilities
                .rgb444
                .supports(ColorBitDepth::Depth12)
        );
        assert!(
            !sink
                .color_capabilities
                .rgb444
                .supports(ColorBitDepth::Depth16)
        );
    }

    #[test]
    fn audio_loads_from_both_audio_blocks_and_basic_audio() {
        use crate::frl::{AudioLoad, AudioPacketType, FrlAudio};
        use display_types::cea861::{
            AudioFormat, AudioFormatInfo, AudioSampleRates as Rates, HdmiAudioBlock,
            ShortAudioDescriptor,
        };
        let sad = |format, channels, rates| {
            ShortAudioDescriptor::new(format, channels, rates, AudioFormatInfo::Raw(0))
        };
        let mut caps = DisplayCapabilities::default();
        let mut cea = Cea861Capabilities::new(Cea861Flags::BASIC_AUDIO);
        cea.audio_descriptors = Vec::from([
            sad(AudioFormat::Lpcm, 8, Rates::HZ_48000 | Rates::HZ_192000),
            sad(AudioFormat::Lpcm, 2, Rates::HZ_48000), // same load as basic audio
            sad(AudioFormat::Dst, 6, Rates::HZ_44100),
            sad(AudioFormat::Ac3, 6, Rates::empty()),
        ]);
        cea.hdmi_audio = Some(HdmiAudioBlock::new(
            false,
            Vec::from([sad(AudioFormat::MlpTrueHd, 8, Rates::HZ_48000)]),
        ));
        with_cea(&mut caps, cea);
        let (sink, warnings) = sink_capabilities_from_display(&caps);
        assert_eq!(
            sink.audio_loads,
            [
                AudioLoad::BASIC,
                AudioLoad::new(AudioPacketType::Sample, 192_000, 8),
                AudioLoad::new(AudioPacketType::HighBitRate, 192_000, 8),
            ]
        );
        assert_eq!(sink.unmodelled_audio, [AudioFormat::Dst]);
        assert_eq!(
            warnings,
            [SinkBuildWarning::AudioDescriptorWithoutSampleRate(sad(
                AudioFormat::Ac3,
                6,
                Rates::empty()
            ))]
        );
        assert_eq!(sink.frl_audio(), FrlAudio::AtMost(&sink.audio_loads));
    }

    #[test]
    fn no_cta_block_means_no_audio() {
        let (sink, _) = sink_capabilities_from_display(&DisplayCapabilities::default());
        assert!(sink.audio_loads.is_empty());
        assert!(sink.unmodelled_audio.is_empty());
    }
}
