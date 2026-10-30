use super::*;

#[test]
fn new_sets_fields_and_clears_quirks() {
    let src = SourceCapabilities::new(600_000, HdmiForumFrl::Rate12Gbps4Lanes, None);
    assert_eq!(src.max_tmds_clock, 600_000);
    assert_eq!(src.max_frl_rate, HdmiForumFrl::Rate12Gbps4Lanes);
    assert!(src.dsc.is_none());
    assert_eq!(src.quirks, QuirkFlags::empty());
}
