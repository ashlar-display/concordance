//! Negotiation policy type and named presets.

/// Controls how the ranker orders validated configurations.
///
/// `NegotiationPolicy` is a const-constructible struct. Named presets are provided
/// for common cases; custom implementations can encode platform-specific priorities.
///
/// # Policy matrix
///
/// The two flags [`prefer_color_fidelity`] and [`prefer_high_refresh`] are independent
/// and together select one of four ranking modes for the quality/performance dimension:
///
/// | `prefer_color_fidelity` | `prefer_high_refresh` | Ranking mode |
/// |:-----------------------:|:---------------------:|--------------|
/// | `true`                  | `false`               | **Color fidelity** — bit depth → color format → refresh rate (descending) |
/// | `false`                 | `true`                | **High refresh** — refresh rate → bit depth → color format (descending) |
/// | `true`                  | `true`                | **Color fidelity** — same as color-fidelity mode; fidelity takes precedence |
/// | `false`                 | `false`               | **Power saving** — all three criteria reversed (ascending), minimising bandwidth |
///
/// [`prefer_color_fidelity`]: NegotiationPolicy::prefer_color_fidelity
/// [`prefer_high_refresh`]: NegotiationPolicy::prefer_high_refresh
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiationPolicy {
    /// Prefer native resolution over non-native resolutions.
    pub prefer_native_resolution: bool,

    /// Prefer higher color fidelity (bit depth, color encoding) over higher refresh rate.
    pub prefer_color_fidelity: bool,

    /// Prefer higher refresh rate over other factors after color fidelity.
    ///
    /// When both `prefer_color_fidelity` and `prefer_high_refresh` are `true`, color
    /// fidelity takes precedence. When both are `false`, the ranker enters power-saving
    /// mode and reverses all three criteria to minimise bandwidth.
    pub prefer_high_refresh: bool,

    /// Penalize configurations that require DSC.
    pub penalize_dsc: bool,

    /// FRL candidates that fit their rate under some readings of the capacity computation
    /// only ([`Warning::FrlCapacityMarginal`][crate::Warning::FrlCapacityMarginal]).
    pub frl_marginal: FrlCandidateHandling,

    /// FRL candidates whose capacity could not be checked, or not fully
    /// ([`Warning::FrlCapacityUnverified`][crate::Warning::FrlCapacityUnverified]).
    pub frl_unverified: FrlCandidateHandling,
}

/// How the ranker treats an FRL candidate carrying a capacity warning.
///
/// There is no option to drop such candidates: the ranker has no way to report a
/// candidate it removes, and accepted candidates are never discarded silently. A caller
/// that never wants them filters on the warnings each configuration carries.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrlCandidateHandling {
    /// Rank it as any other configuration; the warning stays on it.
    Keep,
    /// Rank it after every configuration without such a warning (right after the DSC
    /// criterion), and record that in its trace.
    Deprioritize,
}

impl NegotiationPolicy {
    /// Prefer maximum quality: native resolution, max color fidelity, then refresh rate.
    ///
    /// All presets deprioritize marginal FRL candidates (there is evidence of risk) and
    /// keep unverified ones (mostly "could not check": one unmodelled audio format would
    /// otherwise push every FRL candidate below TMDS).
    pub const BEST_QUALITY: Self = Self {
        prefer_native_resolution: true,
        prefer_color_fidelity: true,
        prefer_high_refresh: false,
        penalize_dsc: true,
        frl_marginal: FrlCandidateHandling::Deprioritize,
        frl_unverified: FrlCandidateHandling::Keep,
    };

    /// Prefer maximum performance: native resolution, max refresh rate, then color fidelity.
    pub const BEST_PERFORMANCE: Self = Self {
        prefer_native_resolution: true,
        prefer_color_fidelity: false,
        prefer_high_refresh: true,
        penalize_dsc: false,
        frl_marginal: FrlCandidateHandling::Deprioritize,
        frl_unverified: FrlCandidateHandling::Keep,
    };

    /// Prefer power saving: lower refresh rates and simpler configurations are ranked higher.
    ///
    /// Native resolution is still preferred even in power-saving mode. Running at a
    /// non-native resolution forces the display scaler on, which consumes additional
    /// power and introduces scaling artifacts — both outcomes that undermine the goal.
    /// Power savings come from reduced refresh rate and color depth, not reduced resolution.
    pub const POWER_SAVING: Self = Self {
        prefer_native_resolution: true,
        prefer_color_fidelity: false,
        prefer_high_refresh: false,
        penalize_dsc: true,
        frl_marginal: FrlCandidateHandling::Deprioritize,
        frl_unverified: FrlCandidateHandling::Keep,
    };
}

impl Default for NegotiationPolicy {
    fn default() -> Self {
        Self::BEST_QUALITY
    }
}
