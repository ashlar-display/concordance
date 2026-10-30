//! Reasoning trace types attached to negotiated configurations.

use alloc::string::String;
use alloc::vec::Vec;

/// A single step in the reasoning trace for a negotiated configuration.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub enum DecisionStep {
    /// The configuration was accepted, possibly with adjustments.
    Accepted {
        /// Adjustments applied to bring the candidate within limits.
        adjustments: Vec<Adjustment>,
    },
    /// The configuration was rejected by the constraint engine.
    Rejected {
        /// Human-readable reason for rejection.
        details: String,
    },
    /// A ranking preference was applied.
    PreferenceApplied {
        /// Description of the preference rule applied.
        rule: String,
    },
}

/// An adjustment applied to a candidate to bring it within declared limits.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub enum Adjustment {
    /// Bit depth was reduced to fit within available bandwidth.
    BitDepthReduced {
        /// Bit depth before adjustment.
        from: u8,
        /// Bit depth after adjustment.
        to: u8,
    },
    /// Color encoding was changed to fit within available bandwidth.
    ColorEncodingChanged {
        /// Description of the change.
        details: String,
    },
}

/// A full record of the decisions made during negotiation of one configuration.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone)]
pub struct ReasoningTrace {
    /// Ordered sequence of decision steps.
    pub steps: Vec<DecisionStep>,
}

impl ReasoningTrace {
    /// Returns an empty trace.
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }
}

impl Default for ReasoningTrace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod tests;
