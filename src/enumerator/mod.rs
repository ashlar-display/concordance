//! Candidate enumerator trait and implementations.

use display_types::cea861::HdmiForumFrl;
use display_types::{ColorBitDepth, ColorFormat, VideoMode};

use crate::types::{CableCapabilities, CandidateConfig, SinkCapabilities, SourceCapabilities};

/// Generates all candidate configurations from the intersection of capabilities.
///
/// Completely policy-free: produces candidates without pre-filtering based on
/// perceived usefulness. No candidate is dropped at enumeration time — rejection
/// happens only in the constraint engine. Equivalent candidates are deduplicated
/// by the pipeline before ranking.
///
/// Custom enumerators can restrict or expand the candidate set (e.g. to limit
/// enumeration to a specific resolution list on embedded targets) without altering
/// constraint or ranking logic.
pub trait CandidateEnumerator {
    /// Iterator type yielding candidate configurations.
    type Iter<'a>: Iterator<Item = CandidateConfig<'a>>
    where
        Self: 'a;

    /// Enumerates all candidate configurations from the given capability triple.
    fn enumerate<'a>(
        &'a self,
        sink: &'a SinkCapabilities,
        source: &'a SourceCapabilities,
        cable: &'a CableCapabilities,
    ) -> Self::Iter<'a>;
}

/// Lazy iterator over the Cartesian product of modes × encodings × depths × FRL rates × DSC.
///
/// Advances like an odometer: the rightmost (innermost) dimension changes fastest.
/// All state is stored inline — no heap allocation.
#[derive(Debug)]
pub struct EnumeratorIter<'a> {
    modes: &'a [VideoMode],
    encodings: [ColorFormat; 4],
    enc_len: usize,
    depths: [[ColorBitDepth; 4]; 4],
    dep_lens: [usize; 4],
    frl_rates: [HdmiForumFrl; 7],
    frl_len: usize,
    dsc: [bool; 2],
    dsc_len: usize,

    mode_idx: usize,
    enc_idx: usize,
    dep_idx: usize,
    frl_idx: usize,
    dsc_idx: usize,
}

impl<'a> EnumeratorIter<'a> {
    /// Returns the number of candidates not yet yielded.
    fn remaining(&self) -> usize {
        if self.mode_idx >= self.modes.len() {
            return 0;
        }
        let dsc_rem = self.dsc_len - self.dsc_idx;
        let frl_rem = (self.frl_len - self.frl_idx - 1) * self.dsc_len;
        let dep_rem =
            (self.dep_lens[self.enc_idx] - self.dep_idx - 1) * self.frl_len * self.dsc_len;
        let enc_rem: usize = (self.enc_idx + 1..self.enc_len)
            .map(|e| self.dep_lens[e] * self.frl_len * self.dsc_len)
            .sum();
        let per_mode: usize = (0..self.enc_len)
            .map(|e| self.dep_lens[e] * self.frl_len * self.dsc_len)
            .sum();
        let mode_rem = (self.modes.len() - self.mode_idx - 1) * per_mode;
        dsc_rem + frl_rem + dep_rem + enc_rem + mode_rem
    }
}

impl<'a> Iterator for EnumeratorIter<'a> {
    type Item = CandidateConfig<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.mode_idx >= self.modes.len() {
            return None;
        }

        let candidate = CandidateConfig {
            mode: &self.modes[self.mode_idx],
            color_encoding: self.encodings[self.enc_idx],
            bit_depth: self.depths[self.enc_idx][self.dep_idx],
            frl_rate: self.frl_rates[self.frl_idx],
            dsc_enabled: self.dsc[self.dsc_idx],
        };

        // Advance odometer, innermost (dsc) first.
        self.dsc_idx += 1;
        if self.dsc_idx < self.dsc_len {
            return Some(candidate);
        }
        self.dsc_idx = 0;

        self.frl_idx += 1;
        if self.frl_idx < self.frl_len {
            return Some(candidate);
        }
        self.frl_idx = 0;

        self.dep_idx += 1;
        if self.dep_idx < self.dep_lens[self.enc_idx] {
            return Some(candidate);
        }
        self.dep_idx = 0;

        self.enc_idx += 1;
        if self.enc_idx < self.enc_len {
            return Some(candidate);
        }
        self.enc_idx = 0;

        self.mode_idx += 1;
        Some(candidate)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.remaining();
        (n, Some(n))
    }
}

impl ExactSizeIterator for EnumeratorIter<'_> {}

/// Constructs an [`EnumeratorIter`] from a mode slice and capability triple.
fn build_iter<'a>(
    modes: &'a [VideoMode],
    sink: &SinkCapabilities,
    source: &SourceCapabilities,
    cable: &CableCapabilities,
) -> EnumeratorIter<'a> {
    const ALL_FORMATS: [ColorFormat; 4] = [
        ColorFormat::Rgb444,
        ColorFormat::YCbCr444,
        ColorFormat::YCbCr422,
        ColorFormat::YCbCr420,
    ];
    const ALL_DEPTHS: [ColorBitDepth; 4] = [
        ColorBitDepth::Depth8,
        ColorBitDepth::Depth10,
        ColorBitDepth::Depth12,
        ColorBitDepth::Depth16,
    ];
    const ALL_FRL_DESC: [HdmiForumFrl; 7] = [
        HdmiForumFrl::Rate12Gbps4Lanes,
        HdmiForumFrl::Rate10Gbps4Lanes,
        HdmiForumFrl::Rate8Gbps4Lanes,
        HdmiForumFrl::Rate6Gbps4Lanes,
        HdmiForumFrl::Rate6Gbps3Lanes,
        HdmiForumFrl::Rate3Gbps3Lanes,
        HdmiForumFrl::NotSupported,
    ];

    // Color encodings: include those that have at least one depth in ALL_DEPTHS.
    let mut encodings = [ColorFormat::Rgb444; 4];
    let mut enc_len = 0usize;
    for &fmt in &ALL_FORMATS {
        let supported = sink.color_capabilities.for_format(fmt);
        if ALL_DEPTHS.iter().any(|&d| supported.supports(d)) {
            encodings[enc_len] = fmt;
            enc_len += 1;
        }
    }

    // Bit depths per encoding.
    let mut depths = [[ColorBitDepth::Depth8; 4]; 4];
    let mut dep_lens = [0usize; 4];
    for i in 0..enc_len {
        let supported = sink.color_capabilities.for_format(encodings[i]);
        let mut d = 0usize;
        for &depth in &ALL_DEPTHS {
            if supported.supports(depth) {
                depths[i][d] = depth;
                d += 1;
            }
        }
        dep_lens[i] = d;
    }

    // FRL rates: highest qualifying tier first; NotSupported (TMDS) always included.
    let sink_frl_ceil = sink
        .hdmi_forum
        .as_ref()
        .map_or(HdmiForumFrl::NotSupported, |hf| hf.max_frl_rate);
    let effective_ceil = sink_frl_ceil
        .min(source.max_frl_rate)
        .min(cable.max_frl_rate);
    let mut frl_rates = [HdmiForumFrl::NotSupported; 7];
    let mut frl_len = 0usize;
    for &rate in &ALL_FRL_DESC {
        if rate <= effective_ceil {
            frl_rates[frl_len] = rate;
            frl_len += 1;
        }
    }

    // DSC: include true only when both source and sink support DSC 1.2.
    let dsc_supported = source.dsc.is_some_and(|d| d.dsc_1p2)
        && sink
            .hdmi_forum
            .as_ref()
            .is_some_and(|hf| hf.dsc.as_ref().is_some_and(|d| d.dsc_1p2));
    let dsc_len = if dsc_supported { 2 } else { 1 };

    // If no usable encoding exists, set the exhausted sentinel immediately.
    let mode_idx = if enc_len == 0 { modes.len() } else { 0 };

    EnumeratorIter {
        modes,
        encodings,
        enc_len,
        depths,
        dep_lens,
        frl_rates,
        frl_len,
        dsc: [false, true],
        dsc_len,
        mode_idx,
        enc_idx: 0,
        dep_idx: 0,
        frl_idx: 0,
        dsc_idx: 0,
    }
}

/// Candidate enumerator backed by a caller-supplied mode slice.
///
/// The caller provides the mode list at construction time. This is the right
/// choice for embedded targets and for tests that want a controlled mode set.
///
/// ```
/// # use concordance::enumerator::SliceEnumerator;
/// # use display_types::VideoMode;
/// # let modes: &[VideoMode] = &[];
/// let enumerator = SliceEnumerator::new(modes);
/// ```
#[derive(Debug)]
pub struct SliceEnumerator<'modes> {
    modes: &'modes [VideoMode],
}

impl<'modes> SliceEnumerator<'modes> {
    /// Creates a new enumerator over the given mode slice.
    pub fn new(modes: &'modes [VideoMode]) -> Self {
        Self { modes }
    }
}

impl<'modes> CandidateEnumerator for SliceEnumerator<'modes> {
    type Iter<'a>
        = EnumeratorIter<'a>
    where
        Self: 'a;

    fn enumerate<'a>(
        &'a self,
        sink: &'a SinkCapabilities,
        source: &'a SourceCapabilities,
        cable: &'a CableCapabilities,
    ) -> Self::Iter<'a> {
        build_iter(self.modes, sink, source, cable)
    }
}

/// Default candidate enumerator.
///
/// Generates the full Cartesian product of supported modes, color encodings,
/// bit depths, and FRL tiers implied by the capability triple.
///
/// Available in `alloc` and `std` tiers only. For bare `no_std` targets use
/// [`SliceEnumerator`] with an explicit mode slice.
#[cfg(any(feature = "alloc", feature = "std"))]
#[derive(Debug, Default)]
pub struct DefaultEnumerator;

#[cfg(any(feature = "alloc", feature = "std"))]
impl CandidateEnumerator for DefaultEnumerator {
    type Iter<'a> = EnumeratorIter<'a>;

    fn enumerate<'a>(
        &'a self,
        sink: &'a SinkCapabilities,
        source: &'a SourceCapabilities,
        cable: &'a CableCapabilities,
    ) -> Self::Iter<'a> {
        build_iter(sink.supported_modes.as_slice(), sink, source, cable)
    }
}

#[cfg(all(test, any(feature = "alloc", feature = "std")))]
mod tests;
