use super::*;

#[test]
fn new_sets_all_fields() {
    let cable = CableCapabilities::new(HdmiSpec::Hdmi21, HdmiForumFrl::Rate12Gbps4Lanes, 600_000);
    assert_eq!(cable.hdmi_spec, HdmiSpec::Hdmi21);
    assert_eq!(cable.max_frl_rate, HdmiForumFrl::Rate12Gbps4Lanes);
    assert_eq!(cable.max_tmds_clock, 600_000);
}
