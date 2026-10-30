use super::*;
extern crate alloc;
use alloc::format;

#[test]
fn limit_source_display() {
    assert_eq!(format!("{}", LimitSource::Sink), "sink");
    assert_eq!(format!("{}", LimitSource::Source), "source");
    assert_eq!(format!("{}", LimitSource::Cable), "cable");
}

#[test]
fn frl_capacity_messages() {
    use crate::frl::{Assumptions, FrlShortfall, FrlUnverified, UnmodelledAudio};
    use display_types::cea861::{AudioFormat, HdmiForumFrl};

    let rate = HdmiForumFrl::Rate6Gbps4Lanes;
    for (reason, text) in [
        (
            FrlUnverified::NoBlanking,
            "the mode has no horizontal blanking",
        ),
        (
            FrlUnverified::NoPixelClock,
            "the mode has no exact pixel clock",
        ),
        (FrlUnverified::Tmds, "the candidate uses TMDS"),
        (
            FrlUnverified::UnknownRate,
            "the FRL rate is unknown to the computation",
        ),
        (
            FrlUnverified::UnsupportedEncoding,
            "the colour format is unknown to the computation",
        ),
        (FrlUnverified::Dsc, "DSC capacity is not computed yet"),
        (
            FrlUnverified::UnmodelledAudio(UnmodelledAudio::Format(AudioFormat::OneBitAudio)),
            "audio Format(OneBitAudio) has no packet model and is not counted",
        ),
        (FrlUnverified::AudioUnknown, "the sink's audio is unknown"),
    ] {
        assert_eq!(
            format!("{}", Warning::FrlCapacityUnverified { rate, reason }),
            format!("FRL capacity at Rate6Gbps4Lanes not verified: {text}")
        );
    }

    for (shortfall, text) in [
        (
            FrlShortfall::AudioBlanking {
                needed: 96,
                available: 95,
            },
            "blanking holds 95 tribytes, the audio needs 96",
        ),
        (
            FrlShortfall::Borrow,
            "the blanking is too short to lend the active video time",
        ),
        (
            FrlShortfall::BorrowLimit {
                borrowed: 401,
                limit: 400,
            },
            "the active video would borrow 401 tribytes, over the limit of 400",
        ),
        (
            FrlShortfall::Utilisation { margin_ppm: -1800 },
            "the line's characters exceed its capacity (margin -1800 ppm)",
        ),
    ] {
        assert_eq!(
            format!("{}", Violation::FrlCapacityExceeded { rate, shortfall }),
            format!("FRL rate Rate6Gbps4Lanes cannot carry the timing: {text}")
        );
    }

    assert_eq!(
        format!(
            "{}",
            Warning::FrlCapacityMarginal {
                rate,
                deciding: Assumptions::MARGIN_TOLERANCE,
                shortfall: FrlShortfall::Utilisation { margin_ppm: -1800 },
            }
        ),
        "FRL rate Rate6Gbps4Lanes is marginal for this timing: the line's characters exceed \
         its capacity (margin -1800 ppm) under the strict reading (deciding: \
         Assumptions(MARGIN_TOLERANCE))"
    );
}

#[test]
fn tagged_violation_display_includes_rule_and_message() {
    let tv = TaggedViolation {
        rule: "my_rule",
        violation: Violation::ColorEncodingUnsupported,
    };
    assert_eq!(
        format!("{tv}"),
        "[my_rule] color encoding not supported by sink"
    );
}

#[cfg(feature = "serde")]
#[test]
fn tagged_violation_rule_deserializes_as_empty_string() {
    let json = r#"{"rule":"original","violation":"ColorEncodingUnsupported"}"#;
    let de: TaggedViolation<Violation> = serde_json::from_str(json).unwrap();
    assert_eq!(de.rule, "");
    assert!(matches!(de.violation, Violation::ColorEncodingUnsupported));
}
