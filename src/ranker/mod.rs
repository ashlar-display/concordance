//! Configuration ranker trait and default implementation.

pub mod policy;

use alloc::vec::Vec;

use core::cmp::Ordering;

use display_types::{ColorFormat, VideoMode};

use crate::diagnostic::Diagnostic;
use crate::output::config::NegotiatedConfig;
use crate::output::trace::DecisionStep;
use crate::output::warning::Warning;
use crate::ranker::policy::{FrlCandidateHandling, NegotiationPolicy};

pub use policy::NegotiationPolicy as Policy;

/// Orders validated configurations according to a [`NegotiationPolicy`].
///
/// The default policy encodes a sensible preference (native resolution, max color
/// fidelity, then refresh rate, then fallback formats), but the caller can supply
/// an override via [`NegotiatorBuilder`][crate::NegotiatorBuilder].
pub trait ConfigRanker {
    /// Non-fatal diagnostic type attached to ranked configurations.
    type Warning: Diagnostic;

    /// Ranks and returns the validated configurations in priority order.
    fn rank(
        &self,
        configs: Vec<NegotiatedConfig<Self::Warning>>,
        policy: &NegotiationPolicy,
    ) -> Vec<NegotiatedConfig<Self::Warning>>;
}

/// Default configuration ranker.
///
/// Implements the built-in preference ordering: native resolution, maximum color
/// fidelity, highest refresh rate, then fallback formats. DSC configurations are
/// ranked lower by default.
#[derive(Debug, Default)]
pub struct DefaultRanker;

/// Returns the pixel area of a video mode (`width × height`).
///
/// Used to identify the native resolution of a display: the mode with the greatest pixel
/// area in the accepted set is treated as native.
fn pixel_area(mode: &VideoMode) -> u32 {
    mode.width as u32 * mode.height as u32
}

/// Returns a quality rank for a color encoding format (higher = better fidelity).
///
/// `Rgb444` ranks above `YCbCr444` at the same chroma resolution because it requires no
/// color-space conversion at the sink. In power-saving mode the caller inverts this value
/// to prefer simpler (lower-bandwidth) formats instead.
fn color_format_quality(fmt: ColorFormat) -> u8 {
    match fmt {
        ColorFormat::Rgb444 => 3,
        ColorFormat::YCbCr444 => 2,
        ColorFormat::YCbCr422 => 1,
        ColorFormat::YCbCr420 => 0,
        // ColorFormat is #[non_exhaustive]; treat any future variant as lowest quality.
        _ => 0,
    }
}

/// Compares two validated configurations according to `policy`.
///
/// Returns [`Ordering::Less`] when `a` should appear before `b` in the ranked output
/// (i.e. `a` is the preferred configuration). Criteria are applied in priority order;
/// the first non-equal result determines the outcome.
fn compare_configs(
    a: &NegotiatedConfig<Warning>,
    b: &NegotiatedConfig<Warning>,
    policy: &NegotiationPolicy,
    native_pixels: u32,
) -> Ordering {
    // 1. DSC penalty: non-DSC (false) sorts before DSC (true).
    if policy.penalize_dsc {
        let ord = a.resolved.dsc_required.cmp(&b.resolved.dsc_required);
        if ord != Ordering::Equal {
            return ord;
        }
    }

    // 1b. Deprioritized FRL capacity warnings: configurations without them first.
    let ord = frl_deprioritized(a, policy).cmp(&frl_deprioritized(b, policy));
    if ord != Ordering::Equal {
        return ord;
    }

    // 2. Native resolution: native sorts before non-native.
    if policy.prefer_native_resolution {
        let a_native = pixel_area(&a.resolved.mode) == native_pixels;
        let b_native = pixel_area(&b.resolved.mode) == native_pixels;
        // true > false, so reverse (b, a) to put native (true) first.
        let ord = b_native.cmp(&a_native);
        if ord != Ordering::Equal {
            return ord;
        }
    }

    // 3. Quality/performance dimension.
    if policy.prefer_color_fidelity {
        // Bit depth descending, then color format quality descending, then refresh rate descending.
        let ord = b
            .resolved
            .bit_depth
            .bits_per_primary()
            .cmp(&a.resolved.bit_depth.bits_per_primary());
        if ord != Ordering::Equal {
            return ord;
        }
        let ord = color_format_quality(b.resolved.color_encoding)
            .cmp(&color_format_quality(a.resolved.color_encoding));
        if ord != Ordering::Equal {
            return ord;
        }
        let ord = b
            .resolved
            .mode
            .refresh_rate
            .cmp(&a.resolved.mode.refresh_rate);
        if ord != Ordering::Equal {
            return ord;
        }
    } else if policy.prefer_high_refresh {
        // Refresh rate descending, then bit depth descending, then color format quality descending.
        let ord = b
            .resolved
            .mode
            .refresh_rate
            .cmp(&a.resolved.mode.refresh_rate);
        if ord != Ordering::Equal {
            return ord;
        }
        let ord = b
            .resolved
            .bit_depth
            .bits_per_primary()
            .cmp(&a.resolved.bit_depth.bits_per_primary());
        if ord != Ordering::Equal {
            return ord;
        }
        let ord = color_format_quality(b.resolved.color_encoding)
            .cmp(&color_format_quality(a.resolved.color_encoding));
        if ord != Ordering::Equal {
            return ord;
        }
    } else {
        // Power saving: lower refresh rate, lower bit depth, and simpler format are preferred.
        let ord = a
            .resolved
            .mode
            .refresh_rate
            .cmp(&b.resolved.mode.refresh_rate);
        if ord != Ordering::Equal {
            return ord;
        }
        let ord = a
            .resolved
            .bit_depth
            .bits_per_primary()
            .cmp(&b.resolved.bit_depth.bits_per_primary());
        if ord != Ordering::Equal {
            return ord;
        }
        let ord = color_format_quality(a.resolved.color_encoding)
            .cmp(&color_format_quality(b.resolved.color_encoding));
        if ord != Ordering::Equal {
            return ord;
        }
    }

    // 4. Progressive before interlaced: false sorts before true.
    let ord = a.resolved.mode.interlaced.cmp(&b.resolved.mode.interlaced);
    if ord != Ordering::Equal {
        return ord;
    }

    // 5. Lower FRL rate first: simpler link wins when all else is equal.
    let ord = a.resolved.frl_rate.cmp(&b.resolved.frl_rate);
    if ord != Ordering::Equal {
        return ord;
    }

    // 6. Larger resolution area first (final tiebreaker).
    pixel_area(&b.resolved.mode).cmp(&pixel_area(&a.resolved.mode))
}

/// Whether `config` carries an FRL capacity warning that `policy` deprioritizes.
fn frl_deprioritized(config: &NegotiatedConfig<Warning>, policy: &NegotiationPolicy) -> bool {
    config.warnings.iter().any(|w| match w {
        Warning::FrlCapacityMarginal { .. } => {
            policy.frl_marginal == FrlCandidateHandling::Deprioritize
        }
        Warning::FrlCapacityUnverified { .. } => {
            policy.frl_unverified == FrlCandidateHandling::Deprioritize
        }
        _ => false,
    })
}

/// Appends [`DecisionStep::PreferenceApplied`] steps to `config`'s trace for each
/// ranking criterion that applies to this specific configuration.
///
/// These are per-config facts, not relative comparisons. They give a diagnostic tool
/// enough context to explain a configuration's characteristics without requiring
/// knowledge of the full ranked list.
fn record_preferences(
    config: &mut NegotiatedConfig<Warning>,
    policy: &NegotiationPolicy,
    native_pixels: u32,
) {
    if policy.penalize_dsc && config.resolved.dsc_required {
        config.trace.steps.push(DecisionStep::PreferenceApplied {
            rule: "DSC penalized".into(),
        });
    }

    if frl_deprioritized(config, policy) {
        config.trace.steps.push(DecisionStep::PreferenceApplied {
            rule: "FRL capacity warning deprioritized".into(),
        });
    }

    if policy.prefer_native_resolution && pixel_area(&config.resolved.mode) == native_pixels {
        config.trace.steps.push(DecisionStep::PreferenceApplied {
            rule: "native resolution preferred".into(),
        });
    }

    let quality_rule = if policy.prefer_color_fidelity {
        "color fidelity preferred"
    } else if policy.prefer_high_refresh {
        "high refresh rate preferred"
    } else {
        "power saving ordering applied"
    };
    config.trace.steps.push(DecisionStep::PreferenceApplied {
        rule: quality_rule.into(),
    });

    if !config.resolved.mode.interlaced {
        config.trace.steps.push(DecisionStep::PreferenceApplied {
            rule: "progressive mode preferred".into(),
        });
    }
}

impl ConfigRanker for DefaultRanker {
    type Warning = crate::output::warning::Warning;

    fn rank(
        &self,
        mut configs: Vec<NegotiatedConfig<Self::Warning>>,
        policy: &NegotiationPolicy,
    ) -> Vec<NegotiatedConfig<Self::Warning>> {
        let native_pixels = configs
            .iter()
            .map(|c| pixel_area(&c.resolved.mode))
            .max()
            .unwrap_or(0);

        configs.sort_by(|a, b| compare_configs(a, b, policy, native_pixels));

        for config in &mut configs {
            record_preferences(config, policy, native_pixels);
        }

        configs
    }
}

#[cfg(test)]
mod tests;
