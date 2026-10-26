//! Built-in warning and violation types.

use display_types::RefreshRate;

/// Identifies which party imposed the binding limit in a bandwidth violation.
///
/// When a bandwidth check fails, this value tells the caller *which* end of the
/// link is the bottleneck so they can suggest the right remediation:
/// - [`Sink`][LimitSource::Sink] — the display's declared ceiling is too low.
/// - [`Source`][LimitSource::Source] — the GPU or transmitter cannot drive the required rate.
/// - [`Cable`][LimitSource::Cable] — the cable cannot carry the required bandwidth;
///   replacing it with a higher-rated cable may resolve the violation.
///
/// When multiple parties share the same binding limit, `Cable` takes priority over
/// `Source`, which takes priority over `Sink`, because cable replacement is the most
/// actionable remediation.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitSource {
    /// The sink's declared capability is the binding constraint.
    Sink,
    /// The source's declared capability is the binding constraint.
    Source,
    /// The cable's declared capability is the binding constraint.
    Cable,
}

impl core::fmt::Display for LimitSource {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LimitSource::Sink => f.write_str("sink"),
            LimitSource::Source => f.write_str("source"),
            LimitSource::Cable => f.write_str("cable"),
        }
    }
}

/// A violation paired with the name of the rule that produced it.
///
/// [`DefaultConstraintEngine`][crate::engine::DefaultConstraintEngine] wraps every
/// violation in a `TaggedViolation` at collection time, so callers can tell which
/// rule rejected a candidate without knowing the violation-to-rule mapping by convention.
///
/// Access the rule name via the `rule` field and the violation via `violation`.
///
/// **Serde round-trip note:** the `rule` field serializes to its string value. On
/// deserialization it is set to `""` — `&'static str` cannot be recovered from runtime
/// data without leaking memory. The `violation` field round-trips faithfully.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct TaggedViolation<V = Violation> {
    /// The [`display_name`][crate::engine::rule::ConstraintRule::display_name] of the
    /// rule that produced this violation.
    #[cfg_attr(
        feature = "serde",
        serde(deserialize_with = "serde_de_ignore_rule_name")
    )]
    pub rule: &'static str,

    /// The violation that was emitted.
    pub violation: V,
}

impl<V: core::fmt::Display> core::fmt::Display for TaggedViolation<V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "[{}] {}", self.rule, self.violation)
    }
}

#[cfg(feature = "serde")]
fn serde_de_ignore_rule_name<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<&'static str, D::Error> {
    use serde::Deserialize as _;
    // &'static str cannot be recovered from deserialized data without leaking.
    // Consume and discard the string; the violation field is preserved faithfully.
    serde::de::IgnoredAny::deserialize(d)?;
    Ok("")
}

/// Non-fatal warning attached to an accepted configuration.
///
/// Warnings do not prevent a mode from being offered; they give the caller enough
/// information to surface concerns to the user or log them. Custom constraint
/// engines and rankers can define their own warning types by specifying an
/// associated `Warning` type bounded by [`Diagnostic`][crate::Diagnostic].
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, thiserror::Error)]
pub enum Warning {
    /// DSC is required for this configuration; lossy compression is active.
    #[error("DSC required; lossy compression active")]
    DscActive,

    /// Cable bandwidth is marginal for the selected mode and may degrade under load.
    #[error("cable bandwidth marginal for selected mode")]
    CableBandwidthMarginal,

    /// The FRL rate carries the timing under some readings of the public capacity
    /// computation and not under others; `deciding` names the assumptions that decide.
    ///
    /// The full report, with every intermediate value, is
    /// [`FrlCapacityCheck::report`][crate::engine::checks::FrlCapacityCheck::report].
    #[error(
        "FRL rate {rate:?} is marginal for this timing: {shortfall} under the strict reading (deciding: {deciding:?})"
    )]
    FrlCapacityMarginal {
        /// The candidate's FRL rate.
        rate: display_types::cea861::HdmiForumFrl,
        /// The assumptions that decide whether it fits.
        deciding: crate::frl::Assumptions,
        /// What the strict evaluation fails on.
        shortfall: crate::frl::FrlShortfall,
    },

    /// The FRL capacity could not be checked, or not fully: the candidate is accepted
    /// without that check.
    #[error("FRL capacity at {rate:?} not verified: {reason}")]
    FrlCapacityUnverified {
        /// The candidate's FRL rate.
        rate: display_types::cea861::HdmiForumFrl,
        /// What is missing.
        reason: crate::frl::FrlUnverified,
    },
}

/// A constraint violation produced when a candidate configuration is rejected.
///
/// Violations are returned by [`is_config_viable`][crate::is_config_viable] and by
/// the constraint engine during pipeline runs. Custom constraint engines can define
/// their own violation types via the `ConstraintEngine::Violation` associated type.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, thiserror::Error)]
pub enum Violation {
    /// Required pixel clock exceeds the sink's declared maximum.
    #[error("pixel clock {required_mhz} MHz exceeds {limit_source} limit of {limit_mhz} MHz")]
    PixelClockExceeded {
        /// Required pixel clock in MHz.
        required_mhz: u32,
        /// Binding limit in MHz.
        limit_mhz: u32,
        /// Which party imposed the binding limit.
        limit_source: LimitSource,
    },

    /// Required TMDS character rate exceeds what the sink, source, or cable supports.
    #[error("TMDS clock {required_mhz} MHz exceeds {limit_source} limit of {limit_mhz} MHz")]
    TmdsClockExceeded {
        /// Required TMDS character rate in MHz.
        required_mhz: u32,
        /// Binding limit in MHz.
        limit_mhz: u32,
        /// Which party imposed the binding limit.
        limit_source: LimitSource,
    },

    /// Required FRL rate exceeds what the sink, source, or cable supports.
    #[error("FRL rate {requested:?} exceeds {limit_source} limit of {limit:?}")]
    FrlRateExceeded {
        /// The FRL rate requested by the candidate configuration.
        requested: display_types::cea861::HdmiForumFrl,
        /// The effective ceiling imposed by the binding party.
        limit: display_types::cea861::HdmiForumFrl,
        /// Which party imposed the binding limit.
        limit_source: LimitSource,
    },

    /// The selected color encoding is not supported by the sink.
    #[error("color encoding not supported by sink")]
    ColorEncodingUnsupported,

    /// A non-YCbCr 4:2:0 encoding was requested for a mode that only supports YCbCr 4:2:0.
    ///
    /// The mode appears in the sink's Y420 Video Data Block, which declares it as a
    /// YCbCr 4:2:0-only mode per CTA-861-H §7.5.11.
    #[error("mode only supports YCbCr 4:2:0; other encodings are not valid for this mode")]
    EncodingRestrictedToYCbCr420,

    /// The selected bit depth is not supported by the sink.
    #[error("bit depth not supported by sink")]
    BitDepthUnsupported,

    /// DSC is required but not supported by all parties.
    #[error("DSC required but not supported")]
    DscUnsupported,

    /// The vertical refresh rate is outside the sink's declared range.
    #[error("refresh rate {rate_hz} outside sink range [{min_hz}, {max_hz}] Hz")]
    RefreshRateOutOfRange {
        /// Refresh rate of the candidate.
        rate_hz: RefreshRate,
        /// Minimum declared by the sink in Hz.
        min_hz: u16,
        /// Maximum declared by the sink in Hz.
        max_hz: u16,
    },

    /// The FRL rate cannot carry the timing under any reading of the public capacity
    /// computation (see [`frl`][crate::frl]).
    ///
    /// The full report, with every intermediate value, is
    /// [`FrlCapacityCheck::report`][crate::engine::checks::FrlCapacityCheck::report].
    #[error("FRL rate {rate:?} cannot carry the timing: {shortfall}")]
    FrlCapacityExceeded {
        /// The candidate's FRL rate.
        rate: display_types::cea861::HdmiForumFrl,
        /// The condition the strict evaluation failed.
        shortfall: crate::frl::FrlShortfall,
    },
}

#[cfg(test)]
mod tests {
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
}
