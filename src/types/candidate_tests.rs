use super::*;
use display_types::VideoMode;

#[test]
fn new_matches_struct_literal() {
    let mode = VideoMode::new(1920, 1080, 60u16, false);
    let via_new = CandidateConfig::new(
        &mode,
        ColorFormat::Rgb444,
        ColorBitDepth::Depth8,
        HdmiForumFrl::NotSupported,
        false,
    );
    let via_literal = CandidateConfig {
        mode: &mode,
        color_encoding: ColorFormat::Rgb444,
        bit_depth: ColorBitDepth::Depth8,
        frl_rate: HdmiForumFrl::NotSupported,
        dsc_enabled: false,
    };
    assert_eq!(via_new, via_literal);
}
