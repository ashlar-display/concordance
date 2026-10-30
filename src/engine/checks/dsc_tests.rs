use super::*;
use crate::engine::rule::ConstraintRule;
use crate::output::warning::Violation;
use crate::types::source::DscCapabilities;
use crate::types::{CableCapabilities, CandidateConfig, SourceCapabilities};
use display_types::cea861::{HdmiDscMaxSlices, HdmiForumDsc, HdmiForumFrl, HdmiForumSinkCap};
use display_types::{ColorBitDepth, ColorFormat, VideoMode};

fn mode() -> VideoMode {
    VideoMode::new(3840, 2160, 60u16, false)
}

fn config_dsc(mode: &VideoMode, dsc_enabled: bool) -> CandidateConfig<'_> {
    CandidateConfig {
        mode,
        color_encoding: ColorFormat::Rgb444,
        bit_depth: ColorBitDepth::Depth8,
        frl_rate: HdmiForumFrl::NotSupported,
        dsc_enabled,
    }
}

fn dsc_source() -> SourceCapabilities {
    SourceCapabilities {
        dsc: Some(DscCapabilities {
            dsc_1p2: true,
            max_slices: 4,
            max_bpp_x16: 128,
        }),
        ..Default::default()
    }
}

fn dsc_sink() -> SinkCapabilities {
    let dsc = HdmiForumDsc::new(
        true,  // dsc_1p2
        false, // native_420
        false, // qms_tfr_max
        false, // qms_tfr_min
        false, // all_bpc
        false, // bpc12
        false, // bpc10
        HdmiForumFrl::Rate12Gbps4Lanes,
        HdmiDscMaxSlices::Slices4,
        0, // max_chunk_bytes
    );
    let hdmi_forum = HdmiForumSinkCap::new(
        1,     // version
        0,     // max_tmds_rate_mhz
        false, // scdc_present
        false, // rr_capable
        false, // cable_status
        false, // ccbpci
        false, // lte_340mcsc_scramble
        false, // independent_view_3d
        false, // dual_view_3d
        false, // osd_disparity_3d
        HdmiForumFrl::Rate12Gbps4Lanes,
        false, // uhd_vic
        false, // dc_48bit_420
        false, // dc_36bit_420
        false, // dc_30bit_420
        false, // fapa_end_extended
        false, // qms
        false, // m_delta
        false, // fva
        false, // allm
        false, // fapa_start_location
        false, // neg_mvrr
        None,  // vrr_min_hz
        None,  // vrr_max_hz
        Some(dsc),
    );
    SinkCapabilities {
        hdmi_forum: Some(hdmi_forum),
        ..Default::default()
    }
}

fn check(
    sink: &SinkCapabilities,
    source: &SourceCapabilities,
    dsc_enabled: bool,
) -> Option<Violation> {
    let m = mode();
    ConstraintRule::<Violation>::check(
        &DscCheck,
        sink,
        source,
        &CableCapabilities::default(),
        &config_dsc(&m, dsc_enabled),
    )
}

#[test]
fn dsc_disabled_always_passes() {
    // DSC disabled: passes even with no DSC support anywhere.
    assert!(
        check(
            &SinkCapabilities::default(),
            &SourceCapabilities::default(),
            false
        )
        .is_none()
    );
}

#[test]
fn dsc_enabled_both_supported_passes() {
    assert!(check(&dsc_sink(), &dsc_source(), true).is_none());
}

#[test]
fn dsc_enabled_source_missing_rejected() {
    assert!(matches!(
        check(&dsc_sink(), &SourceCapabilities::default(), true),
        Some(Violation::DscUnsupported)
    ));
}

#[test]
fn dsc_enabled_sink_missing_rejected() {
    assert!(matches!(
        check(&SinkCapabilities::default(), &dsc_source(), true),
        Some(Violation::DscUnsupported)
    ));
}

#[test]
fn dsc_enabled_both_missing_rejected() {
    assert!(matches!(
        check(
            &SinkCapabilities::default(),
            &SourceCapabilities::default(),
            true
        ),
        Some(Violation::DscUnsupported)
    ));
}
