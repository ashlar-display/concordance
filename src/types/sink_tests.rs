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
fn audio_loads_from_the_audio_data_block_and_basic_audio() {
    use crate::frl::{AudioLoad, AudioPacketType, FrlAudio, UnmodelledAudio};
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
        sad(AudioFormat::MlpTrueHd, 8, Rates::empty()), // HBR whatever the rate
    ]);
    // Multi-stream and 3D audio: no packet model, reported rather than mapped.
    cea.hdmi_audio = Some(HdmiAudioBlock::new(
        2,
        false,
        Vec::from([sad(AudioFormat::Lpcm, 10, Rates::HZ_48000)]),
        None,
    ));
    with_cea(&mut caps, cea);
    let (sink, warnings) = sink_capabilities_from_display(&caps);
    assert_eq!(
        sink.audio_loads.as_deref(),
        Some(
            &[
                AudioLoad::BASIC,
                AudioLoad::new(AudioPacketType::Sample, 192_000, 8),
                AudioLoad::new(AudioPacketType::HighBitRate, 192_000, 8),
            ][..]
        )
    );
    assert_eq!(
        sink.unmodelled_audio,
        [
            UnmodelledAudio::Format(AudioFormat::Dst),
            UnmodelledAudio::MultiStream { max_streams: 2 },
            UnmodelledAudio::Audio3d { descriptors: 1 },
        ]
    );
    assert_eq!(
        warnings,
        [SinkBuildWarning::AudioDescriptorWithoutSampleRate(sad(
            AudioFormat::Ac3,
            6,
            Rates::empty()
        ))]
    );
    assert_eq!(
        sink.frl_audio(),
        Some(FrlAudio::AtMost(sink.audio_loads.as_deref().unwrap()))
    );
}

#[test]
fn no_cta_block_means_no_audio() {
    let (sink, _) = sink_capabilities_from_display(&DisplayCapabilities::default());
    assert_eq!(sink.audio_loads.as_deref(), Some(&[][..]));
    assert!(sink.unmodelled_audio.is_empty());
}

#[test]
fn a_hand_built_sink_has_unknown_audio() {
    let sink = SinkCapabilities::default();
    assert_eq!(sink.audio_loads, None);
    assert_eq!(sink.frl_audio(), None);
}

#[cfg(feature = "serde")]
#[test]
fn sink_without_audio_fields_deserializes_as_unknown_audio() {
    let mut json = serde_json::to_value(SinkCapabilities::default()).unwrap();
    let fields = json.as_object_mut().unwrap();
    fields.remove("audio_loads");
    fields.remove("unmodelled_audio");
    let sink: SinkCapabilities = serde_json::from_value(json).unwrap();
    assert_eq!(sink.audio_loads, None);
}
