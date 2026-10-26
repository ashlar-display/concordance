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
mod tests {
    use alloc::vec::Vec;
    use core::cmp::Ordering;

    use display_types::cea861::HdmiForumFrl;
    use display_types::{ColorBitDepth, ColorFormat, ResolvedDisplayConfig, VideoMode};

    use crate::output::{config::NegotiatedConfig, trace::ReasoningTrace, warning::Warning};
    use crate::ranker::policy::NegotiationPolicy;

    use crate::output::trace::DecisionStep;
    use crate::ranker::DefaultRanker;

    use super::ConfigRanker;
    use super::{color_format_quality, compare_configs, pixel_area, record_preferences};

    fn mode(width: u16, height: u16) -> VideoMode {
        VideoMode::new(width, height, 60u16, false)
    }

    /// Builds a `NegotiatedConfig` with the given fields for use in ranking tests.
    fn config(
        width: u16,
        height: u16,
        refresh_rate: u16,
        interlaced: bool,
        color_encoding: ColorFormat,
        bit_depth: ColorBitDepth,
        frl_rate: HdmiForumFrl,
        dsc_required: bool,
    ) -> NegotiatedConfig<Warning> {
        NegotiatedConfig {
            resolved: ResolvedDisplayConfig::new(
                VideoMode::new(width, height, refresh_rate, interlaced),
                color_encoding,
                bit_depth,
                frl_rate,
                dsc_required,
                false,
            ),
            warnings: Vec::new(),
            trace: ReasoningTrace::new(),
        }
    }

    /// Shorthand for a typical 1080p60 progressive config with sensible defaults.
    fn base() -> NegotiatedConfig<Warning> {
        config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        )
    }

    #[test]
    fn color_format_quality_ordering() {
        // Full ordering: Rgb444 > YCbCr444 > YCbCr422 > YCbCr420.
        assert!(
            color_format_quality(ColorFormat::Rgb444) > color_format_quality(ColorFormat::YCbCr444)
        );
        assert!(
            color_format_quality(ColorFormat::YCbCr444)
                > color_format_quality(ColorFormat::YCbCr422)
        );
        assert!(
            color_format_quality(ColorFormat::YCbCr422)
                > color_format_quality(ColorFormat::YCbCr420)
        );
    }

    #[test]
    fn color_format_quality_exact_values() {
        assert_eq!(color_format_quality(ColorFormat::Rgb444), 3);
        assert_eq!(color_format_quality(ColorFormat::YCbCr444), 2);
        assert_eq!(color_format_quality(ColorFormat::YCbCr422), 1);
        assert_eq!(color_format_quality(ColorFormat::YCbCr420), 0);
    }

    #[test]
    fn pixel_area_multiplies_width_and_height() {
        assert_eq!(pixel_area(&mode(1920, 1080)), 1920 * 1080);
        assert_eq!(pixel_area(&mode(3840, 2160)), 3840 * 2160);
    }

    #[test]
    fn pixel_area_zero_dimension() {
        assert_eq!(pixel_area(&mode(0, 1080)), 0);
        assert_eq!(pixel_area(&mode(1920, 0)), 0);
    }

    #[test]
    fn pixel_area_does_not_overflow_u32() {
        // 65535 × 65535 = 4_294_836_225, which fits in u32 (max 4_294_967_295).
        let area = pixel_area(&mode(u16::MAX, u16::MAX));
        assert_eq!(area, u16::MAX as u32 * u16::MAX as u32);
    }

    // --- compare_configs ---

    const NATIVE: u32 = 1920 * 1080;

    #[test]
    fn dsc_penalized_ranks_lower() {
        let no_dsc = base();
        let with_dsc = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            true,
        );
        let policy = NegotiationPolicy::BEST_QUALITY; // penalize_dsc = true

        assert_eq!(
            compare_configs(&no_dsc, &with_dsc, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&with_dsc, &no_dsc, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn dsc_not_penalized_under_performance() {
        // BEST_PERFORMANCE has penalize_dsc = false, so DSC status should not affect order.
        // Make DSC config have higher refresh to confirm it wins on refresh instead.
        let no_dsc = base(); // 60 Hz
        let with_dsc = config(
            1920,
            1080,
            120,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            true,
        );
        let policy = NegotiationPolicy::BEST_PERFORMANCE;

        // DSC config has higher refresh — it should rank first since DSC is not penalized.
        assert_eq!(
            compare_configs(&with_dsc, &no_dsc, &policy, NATIVE),
            Ordering::Less
        );
    }

    #[test]
    fn native_resolution_ranked_first() {
        let uhd = config(
            3840,
            2160,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let fhd = base();
        let native_pixels = pixel_area(&uhd.resolved.mode); // 4K is native
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&uhd, &fhd, &policy, native_pixels),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&fhd, &uhd, &policy, native_pixels),
            Ordering::Greater
        );
    }

    #[test]
    fn native_resolution_equal_area_falls_through_to_next_criterion() {
        // Both configs share the same pixel area, so both are "native" and the
        // native-resolution criterion yields Equal. The next criterion (bit depth
        // under BEST_QUALITY) must then decide the order.
        let depth10 = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth10,
            HdmiForumFrl::NotSupported,
            false,
        );
        let depth8 = base();
        let policy = NegotiationPolicy::BEST_QUALITY; // prefer_native_resolution = true
        let native_pixels = NATIVE; // both configs match

        assert_eq!(
            compare_configs(&depth10, &depth8, &policy, native_pixels),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&depth8, &depth10, &policy, native_pixels),
            Ordering::Greater
        );
    }

    #[test]
    fn color_fidelity_prefers_higher_depth() {
        let depth10 = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth10,
            HdmiForumFrl::NotSupported,
            false,
        );
        let depth8 = base();
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&depth10, &depth8, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&depth8, &depth10, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn color_fidelity_prefers_rgb_over_ycbcr444() {
        let rgb = base(); // Rgb444
        let ycbcr = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::YCbCr444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&rgb, &ycbcr, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&ycbcr, &rgb, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn color_fidelity_equal_depth_falls_through_to_format() {
        // Same bit depth; color format quality must break the tie under BEST_QUALITY.
        let rgb = base(); // Rgb444
        let ycbcr = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::YCbCr444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&rgb, &ycbcr, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&ycbcr, &rgb, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn color_fidelity_equal_depth_equal_format_falls_through_to_refresh() {
        // Same bit depth and color format; refresh rate must break the tie under BEST_QUALITY.
        let hz120 = config(
            1920,
            1080,
            120,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let hz60 = base();
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&hz120, &hz60, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&hz60, &hz120, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn color_fidelity_prefers_higher_refresh_as_tiebreak() {
        // Same depth and format; refresh rate breaks the tie under BEST_QUALITY.
        let hz120 = config(
            1920,
            1080,
            120,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let hz60 = base();
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&hz120, &hz60, &policy, NATIVE),
            Ordering::Less
        );
    }

    #[test]
    fn performance_prefers_high_refresh_over_depth() {
        // 120 Hz at 8-bit should beat 60 Hz at 10-bit under BEST_PERFORMANCE.
        let hz120_8bit = base(); // 60 Hz — swap in 120 Hz below
        let hz120 = config(
            1920,
            1080,
            120,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let hz60_10bit = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth10,
            HdmiForumFrl::NotSupported,
            false,
        );
        let policy = NegotiationPolicy::BEST_PERFORMANCE;
        let _ = hz120_8bit;

        assert_eq!(
            compare_configs(&hz120, &hz60_10bit, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&hz60_10bit, &hz120, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn performance_equal_refresh_falls_through_to_depth() {
        // Same refresh rate; bit depth must break the tie under BEST_PERFORMANCE.
        let depth10 = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth10,
            HdmiForumFrl::NotSupported,
            false,
        );
        let depth8 = base();
        let policy = NegotiationPolicy::BEST_PERFORMANCE;

        assert_eq!(
            compare_configs(&depth10, &depth8, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&depth8, &depth10, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn performance_equal_refresh_equal_depth_falls_through_to_format() {
        // Same refresh rate and bit depth; color format quality must break the tie.
        let rgb = base(); // Rgb444
        let ycbcr = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::YCbCr444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let policy = NegotiationPolicy::BEST_PERFORMANCE;

        assert_eq!(
            compare_configs(&rgb, &ycbcr, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&ycbcr, &rgb, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn power_saving_prefers_low_refresh() {
        let hz60 = base();
        let hz120 = config(
            1920,
            1080,
            120,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let policy = NegotiationPolicy::POWER_SAVING;

        assert_eq!(
            compare_configs(&hz60, &hz120, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&hz120, &hz60, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn power_saving_prefers_low_depth() {
        let depth8 = base();
        let depth10 = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth10,
            HdmiForumFrl::NotSupported,
            false,
        );
        let policy = NegotiationPolicy::POWER_SAVING;

        assert_eq!(
            compare_configs(&depth8, &depth10, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&depth10, &depth8, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn power_saving_equal_refresh_falls_through_to_depth() {
        // Same refresh rate; lower bit depth must break the tie under POWER_SAVING.
        let depth8 = base();
        let depth10 = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth10,
            HdmiForumFrl::NotSupported,
            false,
        );
        let policy = NegotiationPolicy::POWER_SAVING;

        assert_eq!(
            compare_configs(&depth8, &depth10, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&depth10, &depth8, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn power_saving_equal_refresh_equal_depth_falls_through_to_format() {
        // Same refresh rate and bit depth; simpler color format must break the tie under POWER_SAVING.
        let y420 = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::YCbCr420,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let rgb = base();
        let policy = NegotiationPolicy::POWER_SAVING;

        assert_eq!(
            compare_configs(&y420, &rgb, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&rgb, &y420, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn power_saving_prefers_simpler_format() {
        let y420 = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::YCbCr420,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let rgb = base();
        let policy = NegotiationPolicy::POWER_SAVING;

        assert_eq!(
            compare_configs(&y420, &rgb, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&rgb, &y420, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn progressive_before_interlaced() {
        let progressive = base();
        let interlaced = config(
            1920,
            1080,
            60,
            true,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&progressive, &interlaced, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&interlaced, &progressive, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn lower_frl_rate_tiebreak() {
        let low_frl = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::Rate3Gbps3Lanes,
            false,
        );
        let high_frl = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::Rate6Gbps3Lanes,
            false,
        );
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&low_frl, &high_frl, &policy, NATIVE),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&high_frl, &low_frl, &policy, NATIVE),
            Ordering::Greater
        );
    }

    #[test]
    fn resolution_area_tiebreak() {
        // 1920×1200 has more pixels than 1920×1080; both are "non-native" (native = 4K here).
        let wider = config(
            1920,
            1200,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let narrower = base();
        let native_pixels = pixel_area(&VideoMode::new(3840, 2160, 60u16, false)); // 4K is native
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(
            compare_configs(&wider, &narrower, &policy, native_pixels),
            Ordering::Less
        );
        assert_eq!(
            compare_configs(&narrower, &wider, &policy, native_pixels),
            Ordering::Greater
        );
    }

    // --- record_preferences ---

    fn has_preference(config: &NegotiatedConfig<Warning>, rule: &str) -> bool {
        config
            .trace
            .steps
            .iter()
            .any(|step| matches!(step, DecisionStep::PreferenceApplied { rule: r } if r == rule))
    }

    #[test]
    fn trace_records_dsc_penalty() {
        let mut c = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            true,
        );
        record_preferences(&mut c, &NegotiationPolicy::BEST_QUALITY, NATIVE);
        assert!(has_preference(&c, "DSC penalized"));
    }

    #[test]
    fn trace_no_dsc_penalty_when_not_penalized() {
        let mut c = config(
            1920,
            1080,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            true,
        );
        record_preferences(&mut c, &NegotiationPolicy::BEST_PERFORMANCE, NATIVE); // penalize_dsc=false
        assert!(!has_preference(&c, "DSC penalized"));
    }

    #[test]
    fn trace_no_dsc_penalty_when_not_required() {
        let mut c = base(); // dsc_required=false
        record_preferences(&mut c, &NegotiationPolicy::BEST_QUALITY, NATIVE);
        assert!(!has_preference(&c, "DSC penalized"));
    }

    #[test]
    fn trace_records_native_resolution() {
        let mut c = base();
        record_preferences(&mut c, &NegotiationPolicy::BEST_QUALITY, NATIVE); // pixel_area matches
        assert!(has_preference(&c, "native resolution preferred"));
    }

    #[test]
    fn trace_no_native_resolution_when_not_native() {
        let mut c = base();
        let other_native = pixel_area(&VideoMode::new(3840, 2160, 60u16, false));
        record_preferences(&mut c, &NegotiationPolicy::BEST_QUALITY, other_native);
        assert!(!has_preference(&c, "native resolution preferred"));
    }

    #[test]
    fn trace_no_native_resolution_when_not_preferred() {
        let mut c = base();
        let policy = NegotiationPolicy {
            prefer_native_resolution: false,
            ..NegotiationPolicy::BEST_QUALITY
        };
        record_preferences(&mut c, &policy, NATIVE);
        assert!(!has_preference(&c, "native resolution preferred"));
    }

    #[test]
    fn trace_records_color_fidelity_preferred() {
        let mut c = base();
        record_preferences(&mut c, &NegotiationPolicy::BEST_QUALITY, NATIVE);
        assert!(has_preference(&c, "color fidelity preferred"));
    }

    #[test]
    fn trace_records_high_refresh_preferred() {
        let mut c = base();
        record_preferences(&mut c, &NegotiationPolicy::BEST_PERFORMANCE, NATIVE);
        assert!(has_preference(&c, "high refresh rate preferred"));
    }

    #[test]
    fn trace_records_power_saving_ordering() {
        let mut c = base();
        record_preferences(&mut c, &NegotiationPolicy::POWER_SAVING, NATIVE);
        assert!(has_preference(&c, "power saving ordering applied"));
    }

    #[test]
    fn trace_records_progressive_mode() {
        let mut c = base(); // interlaced=false
        record_preferences(&mut c, &NegotiationPolicy::BEST_QUALITY, NATIVE);
        assert!(has_preference(&c, "progressive mode preferred"));
    }

    #[test]
    fn trace_no_progressive_step_for_interlaced() {
        let mut c = config(
            1920,
            1080,
            60,
            true,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        record_preferences(&mut c, &NegotiationPolicy::BEST_QUALITY, NATIVE);
        assert!(!has_preference(&c, "progressive mode preferred"));
    }

    #[test]
    fn equal_configs_returns_equal() {
        let a = base();
        let b = base();
        let policy = NegotiationPolicy::BEST_QUALITY;

        assert_eq!(compare_configs(&a, &b, &policy, NATIVE), Ordering::Equal);
    }

    // --- DefaultRanker::rank() integration ---

    #[test]
    fn rank_empty_returns_empty() {
        let result = DefaultRanker.rank(Vec::new(), &NegotiationPolicy::BEST_QUALITY);
        assert!(result.is_empty());
    }

    #[test]
    fn rank_single_config_appends_trace() {
        // A single config should be returned with preference steps recorded on its trace.
        let configs = alloc::vec![base()];
        let result = DefaultRanker.rank(configs, &NegotiationPolicy::BEST_QUALITY);
        assert_eq!(result.len(), 1);
        assert!(!result[0].trace.steps.is_empty());
    }

    #[test]
    fn rank_orders_multiple_configs() {
        // Three configs at the same resolution; expected order under BEST_QUALITY:
        // 1. 10-bit no DSC, 2. 8-bit no DSC, 3. 8-bit with DSC.
        // Submitted in reverse order to confirm sorting actually reorders them.
        let configs = alloc::vec![
            config(
                1920,
                1080,
                60,
                false,
                ColorFormat::Rgb444,
                ColorBitDepth::Depth8,
                HdmiForumFrl::NotSupported,
                true
            ),
            base(),
            config(
                1920,
                1080,
                60,
                false,
                ColorFormat::Rgb444,
                ColorBitDepth::Depth10,
                HdmiForumFrl::NotSupported,
                false
            ),
        ];
        let result = DefaultRanker.rank(configs, &NegotiationPolicy::BEST_QUALITY);

        assert_eq!(result[0].resolved.bit_depth, ColorBitDepth::Depth10);
        assert!(!result[0].resolved.dsc_required);
        assert_eq!(result[1].resolved.bit_depth, ColorBitDepth::Depth8);
        assert!(!result[1].resolved.dsc_required);
        assert!(result[2].resolved.dsc_required);
    }

    #[test]
    fn rank_derives_native_pixels_from_accepted_set() {
        // The highest-resolution config in the set should be identified as native and
        // receive a "native resolution preferred" trace step; the lower-resolution config
        // should not — even though prefer_native_resolution is true for both.
        let uhd = config(
            3840,
            2160,
            60,
            false,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth8,
            HdmiForumFrl::NotSupported,
            false,
        );
        let fhd = base(); // 1920×1080

        let result = DefaultRanker.rank(alloc::vec![fhd, uhd], &NegotiationPolicy::BEST_QUALITY);

        // UHD is native, so it ranks first.
        assert_eq!(result[0].resolved.mode.width, 3840);

        // Only the native config should carry the native-resolution preference step.
        assert!(has_preference(&result[0], "native resolution preferred"));
        assert!(!has_preference(&result[1], "native resolution preferred"));
    }

    #[test]
    fn rank_records_preferences_on_all_configs() {
        // Every returned config should have at least the quality-dimension step recorded,
        // regardless of its position in the ranked list.
        let configs = alloc::vec![
            config(
                1920,
                1080,
                60,
                false,
                ColorFormat::Rgb444,
                ColorBitDepth::Depth10,
                HdmiForumFrl::NotSupported,
                false
            ),
            base(),
            config(
                1920,
                1080,
                60,
                false,
                ColorFormat::YCbCr420,
                ColorBitDepth::Depth8,
                HdmiForumFrl::NotSupported,
                false
            ),
        ];
        let result = DefaultRanker.rank(configs, &NegotiationPolicy::BEST_QUALITY);

        for c in &result {
            assert!(has_preference(c, "color fidelity preferred"));
        }
    }

    // --- FRL capacity warnings ---

    fn frl(depth: ColorBitDepth, rate: HdmiForumFrl) -> NegotiatedConfig<Warning> {
        config(
            3840,
            2160,
            60,
            false,
            ColorFormat::Rgb444,
            depth,
            rate,
            false,
        )
    }

    fn marginal() -> Warning {
        Warning::FrlCapacityMarginal {
            rate: HdmiForumFrl::Rate6Gbps4Lanes,
            deciding: crate::frl::Assumptions::MARGIN_TOLERANCE,
            shortfall: crate::frl::FrlShortfall::Borrow,
        }
    }

    fn unverified() -> Warning {
        Warning::FrlCapacityUnverified {
            rate: HdmiForumFrl::Rate6Gbps4Lanes,
            reason: crate::frl::FrlUnverified::NoBlanking,
        }
    }

    #[test]
    fn presets_deprioritize_marginal_and_keep_unverified() {
        use crate::ranker::policy::FrlCandidateHandling::{Deprioritize, Keep};
        for p in [
            NegotiationPolicy::BEST_QUALITY,
            NegotiationPolicy::BEST_PERFORMANCE,
            NegotiationPolicy::POWER_SAVING,
        ] {
            assert_eq!((p.frl_marginal, p.frl_unverified), (Deprioritize, Keep));
        }
    }

    #[test]
    fn marginal_frl_ranks_after_a_sure_config() {
        // 12-bit would rank first on colour fidelity; marginal, it goes after 10-bit.
        let mut deep = frl(ColorBitDepth::Depth12, HdmiForumFrl::Rate6Gbps4Lanes);
        deep.warnings.push(marginal());
        let sure = frl(ColorBitDepth::Depth10, HdmiForumFrl::Rate6Gbps4Lanes);
        let policy = NegotiationPolicy::BEST_QUALITY;
        assert_eq!(
            compare_configs(&sure, &deep, &policy, NATIVE),
            Ordering::Less
        );

        let ranked = DefaultRanker.rank(Vec::from([deep, sure]), &policy);
        assert_eq!(ranked[0].resolved.bit_depth, ColorBitDepth::Depth10);
        assert!(ranked[1].trace.steps.iter().any(|s| matches!(
            s,
            DecisionStep::PreferenceApplied { rule } if rule == "FRL capacity warning deprioritized"
        )));
        assert!(!ranked[0].trace.steps.iter().any(|s| matches!(
            s,
            DecisionStep::PreferenceApplied { rule } if rule == "FRL capacity warning deprioritized"
        )));
    }

    #[test]
    fn kept_warnings_rank_as_usual() {
        let mut policy = NegotiationPolicy::BEST_QUALITY;
        policy.frl_marginal = crate::ranker::policy::FrlCandidateHandling::Keep;
        let mut deep = frl(ColorBitDepth::Depth12, HdmiForumFrl::Rate6Gbps4Lanes);
        deep.warnings.push(marginal());
        let sure = frl(ColorBitDepth::Depth10, HdmiForumFrl::Rate6Gbps4Lanes);
        assert_eq!(
            compare_configs(&deep, &sure, &policy, NATIVE),
            Ordering::Less
        );
    }

    #[test]
    fn unverified_is_kept_by_default_and_can_be_deprioritized() {
        let mut deep = frl(ColorBitDepth::Depth12, HdmiForumFrl::Rate6Gbps4Lanes);
        deep.warnings.push(unverified());
        let sure = frl(ColorBitDepth::Depth10, HdmiForumFrl::Rate6Gbps4Lanes);
        let mut policy = NegotiationPolicy::BEST_QUALITY;
        assert_eq!(
            compare_configs(&deep, &sure, &policy, NATIVE),
            Ordering::Less
        );
        policy.frl_unverified = crate::ranker::policy::FrlCandidateHandling::Deprioritize;
        assert_eq!(
            compare_configs(&sure, &deep, &policy, NATIVE),
            Ordering::Less
        );
    }

    #[test]
    fn dsc_penalty_comes_before_the_frl_criterion() {
        // A marginal non-DSC config still ranks before a sure DSC one.
        let mut marginal_frl = frl(ColorBitDepth::Depth10, HdmiForumFrl::Rate6Gbps4Lanes);
        marginal_frl.warnings.push(marginal());
        let mut dsc = frl(ColorBitDepth::Depth10, HdmiForumFrl::Rate6Gbps4Lanes);
        dsc.resolved.dsc_required = true;
        let policy = NegotiationPolicy::BEST_QUALITY;
        assert_eq!(
            compare_configs(&marginal_frl, &dsc, &policy, NATIVE),
            Ordering::Less
        );
    }
}
