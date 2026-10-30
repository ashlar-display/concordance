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
fn sink_with_dsc_frl(max_frl_rate: HdmiForumFrl, dsc_frl_rate: HdmiForumFrl) -> SinkCapabilities {
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
