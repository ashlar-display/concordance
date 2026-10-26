//! FRL capacity: whether an HDMI 2.1 Fixed Rate Link rate can carry an uncompressed video
//! timing.
//!
//! FRL carries each line's active video and its blanking (with audio and the other
//! packets) in a fixed character stream. A rate carries a timing only if the line's
//! audio fits in its blanking, the active video can borrow what it needs from the
//! blanking, and the characters used, with the link's overhead, fit in the line.
//! [`frl_capacity`] evaluates that for one rate.
//!
//! # Provenance: no ground truth
//!
//! The HDMI 2.1 specification is not public, and this computation does not come from it.
//! It follows the public implementations of the same procedure: amdgpu's
//! `dml1_frl_cap_chk` (Linux, `drivers/gpu/drm/amd/display/dc/dml/`) and i915's
//! `intel_hdmi_frl_dfm` (posted to intel-gfx, 2026-10). They share the procedure and its
//! constants and differ in details. Where one of them contradicts its own comments or its
//! own other formulas, the consistent reading is used:
//!
//! - the minimum blanking time counts one character per blanking tribyte (i915 counts
//!   1.5, against its own payload sum);
//! - the Audio Clock Regeneration packet counts against the compressible blanking (i915
//!   leaves it out, against its own comment);
//! - the borrow limit of 400 tribytes applies (one of amdgpu's copies omits it).
//!
//! Where neither reading is visibly wrong, the point is an [`Assumptions`] flag, and
//! [`frl_capacity`] evaluates every combination of readings. The report's
//! [`strict`][FrlCapacityReport::strict] and [`permissive`][FrlCapacityReport::permissive]
//! evaluations are the worst and the best of them; between them is the honest extent of
//! what is known. A timing that fits one way and not the other is
//! [`FrlVerdict::Marginal`], with the [`deciding`][FrlCapacityReport::deciding]
//! assumptions named. When a point is settled, its flag stops applying.
//!
//! All arithmetic is exact integer arithmetic; there are no floating-point values.
//!
//! # Audio
//!
//! Audio packets travel in the blanking, so the audio decides whether a short blanking
//! fits. [`FrlAudio`] says what audio to count: none, a bound (`AtMost`, such as the
//! sink's descriptors: the source may send any of them or none), or what the source may
//! send (`OneOf`). The loads are borrowed slices; nothing is copied or allocated.
//! [`AudioLoad::from_descriptor`] maps a Short Audio Descriptor to the heaviest load a
//! source may send for it.
//!
//! **Without allocation** (bare `no_std`), [`SinkCapabilities`][crate::SinkCapabilities]
//! has no audio loads, so the sink's audio is unknown to the negotiation engine. Firmware
//! that knows the audio calls [`frl_capacity`] with its own loads, from a `static` or stack
//! array; see [`FrlAudio`].

use bitflags::bitflags;
use display_types::cea861::{AudioFormat, AudioSampleRates, HdmiForumFrl, ShortAudioDescriptor};
use display_types::{ColorBitDepth, ColorFormat, VideoMode};

/// Maximum tribytes the active video may borrow from the blanking.
const TB_BORROWED_MAX: u32 = 400;
/// FRL characters per character block.
const C_FRL_CB: u128 = 510;

bitflags! {
    /// Points where the public implementations disagree and neither is visibly wrong.
    ///
    /// Each flag selects the second of two readings; an unset flag is the first. A set of
    /// flags is one combination of readings. [`frl_capacity`] evaluates every combination
    /// of the flags that apply to its input.
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Assumptions: u8 {
        /// FRL characters per line: floored (amdgpu), or, when set, rounded up (i915).
        /// Always applies.
        const CHARS_PER_LINE_ROUNDING = 1 << 0;
        /// The final margin must be above zero (i915), or, when set, may go down to −1/100
        /// (amdgpu's `EPSILON`). Always applies.
        const MARGIN_TOLERANCE = 1 << 1;
        /// The link overhead is exact (amdgpu), or, when set, each of its three terms is
        /// truncated to thousandths (i915). Always applies.
        const OVERHEAD_TRUNCATION = 1 << 2;
        /// Audio sample packets with more than two channels use layout 1 (i915), or, when
        /// set, layout 0 when the timing has at most 576 active lines (amdgpu). Applies to
        /// multichannel sample packets on such timings.
        const AUDIO_LAYOUT_SD_LINES = 1 << 3;
        /// High Bit Rate audio packets come at four times the declared sample rate, or,
        /// when set, at the declared rate. Applies to HBR audio.
        const HBR_RATE = 1 << 4;
        /// The audio is the input's [`audio`][FrlCapacityInput::audio], or, when set,
        /// none (Audio Clock Regeneration packets still count). Applies when the input
        /// has audio.
        const AUDIO_LOAD = 1 << 5;
        /// A Detailed Timing Descriptor's blanking includes its borders (EDID 1.3, as EDIDs
        /// in practice are read), or, when set, excludes them (EDID 1.4): `h_blank` is
        /// then larger by both borders, at the same pixel clock. Applies when the input has
        /// borders.
        const DTD_BORDER_READING = 1 << 6;
    }
}

/// The kind of audio packet carrying a load.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioPacketType {
    /// Audio Sample Packet (L-PCM and compressed audio at up to 192 kHz): one packet per
    /// four samples with up to two channels (layout 0), one per sample with more (layout 1).
    Sample,
    /// One Bit Audio Sample Packet: one packet per four samples.
    OneBitSample,
    /// High Bit Rate Audio Stream Packet: one packet per sample.
    HighBitRate,
}

/// An audio load carried in the blanking.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioLoad {
    /// The packet type.
    pub packet_type: AudioPacketType,
    /// The sample rate in Hz, as declared.
    pub sample_rate_hz: u32,
    /// The number of channels.
    pub channels: u8,
}

impl AudioLoad {
    /// Basic audio, which every sink declaring basic audio support accepts: two-channel
    /// L-PCM at up to 48 kHz (CTA-861).
    pub const BASIC: AudioLoad = AudioLoad::new(AudioPacketType::Sample, 48_000, 2);

    /// An audio load.
    pub const fn new(packet_type: AudioPacketType, sample_rate_hz: u32, channels: u8) -> Self {
        Self {
            packet_type,
            sample_rate_hz,
            channels,
        }
    }

    /// The heaviest load a source may send for a Short Audio Descriptor.
    ///
    /// Compressed formats travel as IEC 61937 bursts, whose rate depends on the format
    /// (as in FFmpeg's IEC 61937 muxer, `libavformat/spdifenc.c`). Where the descriptor
    /// does not say which variant the source sends, the heavier one is taken:
    ///
    /// | Format | Load |
    /// |---|---|
    /// | L-PCM | sample packets, the declared channels, the highest declared rate |
    /// | AC-3, MPEG-1, MP3, MPEG-2 multichannel, AAC LC, DTS, ATRAC, WMA Pro | sample packets, 2 channels, the highest declared rate |
    /// | Enhanced AC-3 | sample packets, 2 channels, 4 × the highest declared rate |
    /// | DTS-HD, MLP/Dolby TrueHD | High Bit Rate packets at 192 kHz (768 kHz on the link: MAT and DTS-HD Master Audio) |
    /// | One Bit Audio | one-bit sample packets, the declared channels, the highest declared rate |
    /// | DST, extended and reserved codes | [`AudioMapping::Unmodelled`]: no public implementation models them |
    ///
    /// A descriptor with no sample rate set is [`AudioMapping::NoSampleRate`].
    pub fn from_descriptor(sad: &ShortAudioDescriptor) -> AudioMapping {
        let Some(rate) = highest_rate(sad.sample_rates) else {
            return AudioMapping::NoSampleRate;
        };
        let sample = |channels| AudioLoad::new(AudioPacketType::Sample, rate, channels);
        AudioMapping::Load(match sad.format {
            AudioFormat::Lpcm => sample(sad.max_channels),
            AudioFormat::Ac3
            | AudioFormat::Mpeg1
            | AudioFormat::Mp3
            | AudioFormat::Mpeg2Multichannel
            | AudioFormat::AacLc
            | AudioFormat::Dts
            | AudioFormat::Atrac
            | AudioFormat::WmaPro => sample(2),
            AudioFormat::EnhancedAc3 => AudioLoad::new(AudioPacketType::Sample, 4 * rate, 2),
            AudioFormat::DtsHd | AudioFormat::MlpTrueHd => {
                AudioLoad::new(AudioPacketType::HighBitRate, 192_000, sad.max_channels)
            }
            AudioFormat::OneBitAudio => {
                AudioLoad::new(AudioPacketType::OneBitSample, rate, sad.max_channels)
            }
            other => return AudioMapping::Unmodelled(other),
        })
    }
}

/// The highest sample rate in a descriptor's rate set, in Hz.
fn highest_rate(rates: AudioSampleRates) -> Option<u32> {
    [
        (AudioSampleRates::HZ_192000, 192_000),
        (AudioSampleRates::HZ_176400, 176_400),
        (AudioSampleRates::HZ_96000, 96_000),
        (AudioSampleRates::HZ_88200, 88_200),
        (AudioSampleRates::HZ_48000, 48_000),
        (AudioSampleRates::HZ_44100, 44_100),
        (AudioSampleRates::HZ_32000, 32_000),
    ]
    .into_iter()
    .find(|(flag, _)| rates.contains(*flag))
    .map(|(_, hz)| hz)
}

/// What [`AudioLoad::from_descriptor`] makes of a Short Audio Descriptor.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMapping {
    /// The heaviest load a source may send for it.
    Load(AudioLoad),
    /// A format whose packet load no public implementation models; it cannot be bounded.
    Unmodelled(AudioFormat),
    /// The descriptor declares no sample rate.
    NoSampleRate,
}

/// The audio an FRL link must carry alongside the video.
///
/// The loads are borrowed: [`FrlCapacityInput`] stays `Copy`, and nothing is allocated.
/// Each evaluation counts the heaviest load under its own readings (the
/// [`HBR_RATE`][Assumptions::HBR_RATE] and
/// [`AUDIO_LAYOUT_SD_LINES`][Assumptions::AUDIO_LAYOUT_SD_LINES] readings can change which
/// one is heaviest).
///
/// # Without allocation
///
/// [`SinkCapabilities`][crate::SinkCapabilities] carries the sink's audio loads only with
/// `alloc`. In a bare `no_std` build it has none, so the negotiation engine cannot bound a
/// sink's audio and reports FRL candidates as unverified for audio. Firmware that knows the
/// audio calls [`frl_capacity`] directly with its own loads, from a `static` or stack array
/// (for example mapped from the descriptors with [`AudioLoad::from_descriptor`]).
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrlAudio<'a> {
    /// No audio. Audio Clock Regeneration packets still count.
    #[default]
    None,
    /// At most these loads, for example the sink's descriptors: the source may send any
    /// of them, or none. The [`AUDIO_LOAD`][Assumptions::AUDIO_LOAD] assumption applies.
    /// An empty slice is no audio.
    AtMost(&'a [AudioLoad]),
    /// One of these loads, for example what the source is configured to send. The
    /// heaviest is counted, and [`AUDIO_LOAD`][Assumptions::AUDIO_LOAD] does not apply.
    OneOf(&'a [AudioLoad]),
}

impl<'a> FrlAudio<'a> {
    /// The loads, whichever kind; empty for [`FrlAudio::None`].
    pub const fn loads(&self) -> &'a [AudioLoad] {
        match self {
            FrlAudio::None => &[],
            FrlAudio::AtMost(l) | FrlAudio::OneOf(l) => l,
        }
    }
}

/// Why a candidate's FRL capacity cannot be evaluated, or not fully.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrlUnverified {
    /// The mode has no horizontal blanking ([`VideoMode::h_blank`] is `None`).
    NoBlanking,
    /// The mode has no exact pixel clock ([`VideoMode::pixel_clock_khz`] is `None` or 0).
    NoPixelClock,
    /// The candidate uses TMDS, not FRL.
    Tmds,
    /// The FRL rate is one the computation does not know (a rate added after this
    /// release).
    UnknownRate,
    /// The colour format is not one the computation knows.
    UnsupportedEncoding,
    /// The candidate uses Display Stream Compression, whose capacity computation is not
    /// implemented yet.
    Dsc,
    /// The sink accepts an audio format whose packet load no public implementation
    /// models, so the audio is bounded without it (see [`AudioMapping::Unmodelled`]).
    UnmodelledAudio(AudioFormat),
    /// The sink's audio is unknown: bare `no_std` builds carry no audio loads in
    /// [`SinkCapabilities`][crate::SinkCapabilities], so the video was checked without
    /// audio (see [`FrlAudio`]).
    AudioUnknown,
}

impl core::fmt::Display for FrlUnverified {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoBlanking => f.write_str("the mode has no horizontal blanking"),
            Self::NoPixelClock => f.write_str("the mode has no exact pixel clock"),
            Self::Tmds => f.write_str("the candidate uses TMDS"),
            Self::UnknownRate => f.write_str("the FRL rate is unknown to the computation"),
            Self::UnsupportedEncoding => {
                f.write_str("the colour format is unknown to the computation")
            }
            Self::Dsc => f.write_str("DSC capacity is not computed yet"),
            Self::UnmodelledAudio(format) => {
                write!(
                    f,
                    "audio format {format:?} has no packet model and is not counted"
                )
            }
            Self::AudioUnknown => f.write_str("the sink's audio is unknown (no alloc)"),
        }
    }
}

/// The input to one FRL capacity evaluation.
///
/// Borrows its audio loads (see [`FrlAudio`]), so it serializes but does not deserialize.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrlCapacityInput<'a> {
    /// The FRL rate (lanes and Gbps per lane).
    pub rate: HdmiForumFrl,
    /// The pixel clock in kHz.
    pub pixel_clock_khz: u32,
    /// Active pixels per line, borders included: HDMI sends border pixels as video data.
    pub h_active: u16,
    /// Horizontal blanking in pixels, borders excluded.
    pub h_blank: u16,
    /// The horizontal border on each side, in pixels; only the
    /// [`DTD_BORDER_READING`][Assumptions::DTD_BORDER_READING] assumption uses it.
    pub h_border: u16,
    /// Active lines, or 0 when unknown; only the
    /// [`AUDIO_LAYOUT_SD_LINES`][Assumptions::AUDIO_LAYOUT_SD_LINES] assumption uses it, and
    /// does not apply when it is unknown.
    pub v_active: u16,
    /// The colour format.
    pub encoding: ColorFormat,
    /// The bit depth per component.
    pub depth: ColorBitDepth,
    /// The audio the link must carry.
    pub audio: FrlAudio<'a>,
}

impl<'a> FrlCapacityInput<'a> {
    /// An input with no borders and no audio; `v_active` is 0 until set.
    pub const fn new(
        rate: HdmiForumFrl,
        pixel_clock_khz: u32,
        h_active: u16,
        h_blank: u16,
        encoding: ColorFormat,
        depth: ColorBitDepth,
    ) -> Self {
        Self {
            rate,
            pixel_clock_khz,
            h_active,
            h_blank,
            h_border: 0,
            v_active: 0,
            encoding,
            depth,
            audio: FrlAudio::None,
        }
    }

    /// Sets the horizontal border on each side, in pixels, returning the updated input.
    /// `h_active` is not changed: it already includes the borders.
    pub const fn with_h_border(mut self, h_border: u16) -> Self {
        self.h_border = h_border;
        self
    }

    /// Sets the number of active lines, returning the updated input.
    pub const fn with_v_active(mut self, v_active: u16) -> Self {
        self.v_active = v_active;
        self
    }

    /// Sets the audio the link must carry, returning the updated input.
    pub const fn with_audio(mut self, audio: FrlAudio<'a>) -> Self {
        self.audio = audio;
        self
    }

    /// The input for `mode` at `rate`, in `encoding` and `depth`, with `audio`.
    ///
    /// `h_active` is the mode's width plus both borders.
    ///
    /// # Errors
    ///
    /// [`FrlUnverified::Tmds`] for [`HdmiForumFrl::NotSupported`],
    /// [`FrlUnverified::NoBlanking`] or [`FrlUnverified::NoPixelClock`] when the mode lacks
    /// them, [`FrlUnverified::UnsupportedEncoding`] for a colour format the computation
    /// does not know.
    pub fn from_mode(
        mode: &VideoMode,
        rate: HdmiForumFrl,
        encoding: ColorFormat,
        depth: ColorBitDepth,
        audio: FrlAudio<'a>,
    ) -> Result<Self, FrlUnverified> {
        if rate == HdmiForumFrl::NotSupported {
            return Err(FrlUnverified::Tmds);
        }
        if lanes_and_gbps(rate).is_none() {
            return Err(FrlUnverified::UnknownRate);
        }
        if !matches!(
            encoding,
            ColorFormat::Rgb444
                | ColorFormat::YCbCr444
                | ColorFormat::YCbCr422
                | ColorFormat::YCbCr420
        ) {
            return Err(FrlUnverified::UnsupportedEncoding);
        }
        let h_blank = mode.h_blank.ok_or(FrlUnverified::NoBlanking)?;
        let pixel_clock_khz = mode
            .pixel_clock_khz
            .filter(|&p| p > 0)
            .ok_or(FrlUnverified::NoPixelClock)?;
        let h_border = u16::from(mode.h_border);
        let h_active = mode.width.saturating_add(2 * h_border);
        Ok(Self {
            rate,
            pixel_clock_khz,
            h_active,
            h_blank,
            h_border,
            v_active: mode.height,
            encoding,
            depth,
            audio,
        })
    }

    /// The assumptions that apply to this input.
    pub fn applicable_assumptions(&self) -> Assumptions {
        let mut a = Assumptions::CHARS_PER_LINE_ROUNDING
            | Assumptions::MARGIN_TOLERANCE
            | Assumptions::OVERHEAD_TRUNCATION;
        let loads = self.audio.loads();
        if matches!(self.audio, FrlAudio::AtMost(_)) && !loads.is_empty() {
            a |= Assumptions::AUDIO_LOAD;
        }
        for load in loads {
            match load.packet_type {
                AudioPacketType::Sample
                    if load.channels > 2 && self.v_active != 0 && self.v_active <= 576 =>
                {
                    a |= Assumptions::AUDIO_LAYOUT_SD_LINES;
                }
                AudioPacketType::HighBitRate => a |= Assumptions::HBR_RATE,
                _ => {}
            }
        }
        if self.h_border > 0 {
            a |= Assumptions::DTD_BORDER_READING;
        }
        a
    }
}

/// An exact signed fraction: the share of a line's FRL characters left over.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Margin {
    numer: i128,
    denom: u128,
}

impl Margin {
    fn new(numer: i128, denom: u128) -> Self {
        let g = gcd(numer.unsigned_abs(), denom).max(1);
        Self {
            numer: numer / g as i128,
            denom: denom / g,
        }
    }

    /// The numerator, in lowest terms.
    pub const fn numer(&self) -> i128 {
        self.numer
    }

    /// The denominator, in lowest terms; always positive.
    pub const fn denom(&self) -> u128 {
        self.denom
    }

    /// The margin in parts per million, rounded towards negative infinity. For display;
    /// comparisons use the exact fraction.
    pub fn ppm(&self) -> i64 {
        let scaled = self.numer * 1_000_000;
        let d = self.denom as i128;
        let q = scaled.div_euclid(d);
        q.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }
}

impl PartialOrd for Margin {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Margin {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.numer * other.denom as i128).cmp(&(other.numer * self.denom as i128))
    }
}

impl core::fmt::Display for FrlShortfall {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::AudioBlanking { needed, available } => write!(
                f,
                "blanking holds {available} tribytes, the audio needs {needed}"
            ),
            Self::Borrow => f.write_str("the blanking is too short to lend the active video time"),
            Self::BorrowLimit { borrowed, limit } => write!(
                f,
                "the active video would borrow {borrowed} tribytes, over the limit of {limit}"
            ),
            Self::Utilisation { margin } => write!(
                f,
                "the line's characters exceed its capacity (margin {} ppm)",
                margin.ppm()
            ),
        }
    }
}

/// The condition a timing failed.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrlShortfall {
    /// The blanking cannot hold the line's audio packets.
    AudioBlanking {
        /// Blanking tribytes needed: 32 per packet plus 32.
        needed: u32,
        /// Blanking tribytes the timing has.
        available: u32,
    },
    /// The blanking is too short on the link to lend the active video any time.
    Borrow,
    /// The active video would borrow more tribytes from the blanking than allowed.
    BorrowLimit {
        /// Tribytes the active video would borrow.
        borrowed: u32,
        /// The limit (400).
        limit: u32,
    },
    /// The line's characters and the link overhead exceed the line's FRL characters.
    Utilisation {
        /// The margin left over (negative or, under the strict reading, zero).
        margin: Margin,
    },
}

/// One evaluation of the procedure, under one combination of readings.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrlEvaluation {
    /// The combination of readings (set flags take their second reading).
    pub readings: Assumptions,
    /// FRL characters per line, at the slowest permitted bit rate.
    pub chars_per_line: u32,
    /// Tribytes of active video per line.
    pub tribytes_active: u32,
    /// Tribytes of blanking per line.
    pub tribytes_blank: u32,
    /// Audio and clock regeneration packets per line (at least one).
    pub audio_packets_per_line: u32,
    /// Tribytes the active video borrows from the blanking (0 when it borrows none or the
    /// evaluation stopped before borrowing).
    pub tribytes_borrowed: u32,
    /// The share of the line's characters left over after payload and overhead.
    pub margin: Margin,
    /// `Ok` when the timing fits, otherwise the first condition it failed (audio, borrow,
    /// borrow limit, utilisation, in that order).
    pub result: Result<(), FrlShortfall>,
}

/// Whether a timing fits a rate, across the readings.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrlVerdict {
    /// It fits under every reading.
    Fits,
    /// It fits under some readings and not others; the report names the deciding
    /// assumptions.
    Marginal,
    /// It fits under no reading.
    Exceeded,
}

/// The result of [`frl_capacity`].
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrlCapacityReport {
    /// The assumptions that applied to the input.
    pub applicable: Assumptions,
    /// The worst evaluation: a failing one with the lowest margin if any fails, otherwise
    /// the one with the lowest margin.
    pub strict: FrlEvaluation,
    /// The best evaluation: a passing one with the highest margin if any passes,
    /// otherwise the one with the highest margin.
    pub permissive: FrlEvaluation,
    /// The assumptions whose reading alone, with every other reading held, changes
    /// whether the timing fits. Empty unless the verdict is
    /// [`Marginal`][FrlVerdict::Marginal], and never empty when it is.
    pub deciding: Assumptions,
}

impl FrlCapacityReport {
    /// Whether the timing fits under every reading, some, or none.
    pub fn verdict(&self) -> FrlVerdict {
        match (self.strict.result.is_ok(), self.permissive.result.is_ok()) {
            (true, _) => FrlVerdict::Fits,
            (false, true) => FrlVerdict::Marginal,
            (false, false) => FrlVerdict::Exceeded,
        }
    }
}

/// Evaluates whether `input`'s timing fits its FRL rate, under every combination of the
/// readings that apply (see [`Assumptions`]).
///
/// # Panics
///
/// When `rate` is not a known FRL rate (it is [`HdmiForumFrl::NotSupported`], or a rate
/// added after this release) or the pixel clock is 0. [`FrlCapacityInput::from_mode`]
/// rules out both.
pub fn frl_capacity(input: &FrlCapacityInput<'_>) -> FrlCapacityReport {
    assert!(
        lanes_and_gbps(input.rate).is_some(),
        "frl_capacity needs a known FRL rate"
    );
    assert!(
        input.pixel_clock_khz > 0,
        "frl_capacity needs a pixel clock"
    );
    let applicable = input.applicable_assumptions();
    let mask = applicable.bits();

    let mut passes = [false; 128];
    let mut strict: Option<FrlEvaluation> = None;
    let mut permissive: Option<FrlEvaluation> = None;
    // Every subset of `mask`, including the empty one.
    let mut sub = mask;
    loop {
        let e = evaluate(input, Assumptions::from_bits_truncate(sub));
        passes[sub as usize] = e.result.is_ok();
        strict = Some(match strict {
            None => e,
            Some(s) => worse(s, e),
        });
        permissive = Some(match permissive {
            None => e,
            Some(p) => better(p, e),
        });
        if sub == 0 {
            break;
        }
        sub = (sub - 1) & mask;
    }

    let mut deciding = Assumptions::empty();
    for flag in applicable.iter() {
        let bit = flag.bits();
        let mut sub = mask;
        loop {
            if sub & bit == 0 && passes[sub as usize] != passes[(sub | bit) as usize] {
                deciding |= flag;
                break;
            }
            if sub == 0 {
                break;
            }
            sub = (sub - 1) & mask;
        }
    }

    FrlCapacityReport {
        applicable,
        strict: strict.expect("at least one evaluation"),
        permissive: permissive.expect("at least one evaluation"),
        deciding,
    }
}

/// The worse of two evaluations: a failing one over a passing one, then the lower margin.
fn worse(a: FrlEvaluation, b: FrlEvaluation) -> FrlEvaluation {
    match (a.result.is_ok(), b.result.is_ok()) {
        (true, false) => b,
        (false, true) => a,
        _ if b.margin < a.margin => b,
        _ => a,
    }
}

/// The better of two evaluations: a passing one over a failing one, then the higher margin.
fn better(a: FrlEvaluation, b: FrlEvaluation) -> FrlEvaluation {
    match (a.result.is_ok(), b.result.is_ok()) {
        (false, true) => b,
        (true, false) => a,
        _ if b.margin > a.margin => b,
        _ => a,
    }
}

/// Lanes and Gbps per lane for an FRL rate; `None` for TMDS and unknown rates.
fn lanes_and_gbps(rate: HdmiForumFrl) -> Option<(u128, u128)> {
    match rate {
        HdmiForumFrl::Rate3Gbps3Lanes => Some((3, 3)),
        HdmiForumFrl::Rate6Gbps3Lanes => Some((3, 6)),
        HdmiForumFrl::Rate6Gbps4Lanes => Some((4, 6)),
        HdmiForumFrl::Rate8Gbps4Lanes => Some((4, 8)),
        HdmiForumFrl::Rate10Gbps4Lanes => Some((4, 10)),
        HdmiForumFrl::Rate12Gbps4Lanes => Some((4, 12)),
        _ => None,
    }
}

fn div_ceil(n: u128, d: u128) -> u128 {
    n.div_ceil(d)
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn to_u32(v: u128) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

/// One evaluation under the readings in `alt`.
///
/// Times are counted in periods of the pixel clock at its maximum tolerance (+0.5%), so
/// the line lasts `h_active + h_blank` periods, the active part `h_active`. Rates are
/// characters per such period.
fn evaluate(input: &FrlCapacityInput<'_>, alt: Assumptions) -> FrlEvaluation {
    let (lanes, gbps) = lanes_and_gbps(input.rate).expect("checked by frl_capacity");
    let p_khz = u128::from(input.pixel_clock_khz);
    let ha = u128::from(input.h_active);
    let hb = u128::from(input.h_blank)
        + if alt.contains(Assumptions::DTD_BORDER_READING) {
            2 * u128::from(input.h_border)
        } else {
            0
        };
    let line = ha + hb;

    // Link: super block 4 × 510 + lanes characters; overhead from framing (one character per
    // lane), RS parity (8 × 4) and FRL map characters (2.5), plus 0.3%.
    let c_sb = 4 * C_FRL_CB + lanes;
    let (o_n, o_d) = if alt.contains(Assumptions::OVERHEAD_TRUNCATION) {
        (1000 * lanes / c_sb + 32_000 / c_sb + 2500 / c_sb + 3, 1000)
    } else {
        // (lanes + 32 + 2.5) / c_sb + 3/1000
        ((2 * lanes + 69) * 1000 + 6 * c_sb, 2000 * c_sb)
    };

    // Raw link characters per period, all lanes, at the slowest bit rate (−300 ppm):
    // lanes · gbps·1e9 · 9997/10000 / 18 per second, over (p_khz·1000 · 201/200) periods
    // per second.
    let raw_n = lanes * gbps * 1_000_000 * 9997 * 200;
    let raw_d = 180_000 * p_khz * 201;
    let chars_per_line = if alt.contains(Assumptions::CHARS_PER_LINE_ROUNDING) {
        div_ceil(line * raw_n, raw_d)
    } else {
        line * raw_n / raw_d
    };
    // Effective characters per period, after the overhead.
    let k_n = raw_n * (o_d - o_n);
    let k_d = raw_d * o_d;

    // Audio: packets per second (ap per sample, plus two ACR packets' worth of 1500/s) at
    // +1000 ppm, over the line, for the heaviest load under these readings. `load4` is
    // four times the audio packets per second.
    let counted: &[AudioLoad] = match input.audio {
        FrlAudio::AtMost(_) if alt.contains(Assumptions::AUDIO_LOAD) => &[],
        audio => audio.loads(),
    };
    let load4 = counted
        .iter()
        .map(|a| {
            let rate = u128::from(a.sample_rate_hz);
            match a.packet_type {
                AudioPacketType::Sample => {
                    let layout0 = a.channels <= 2
                        || (alt.contains(Assumptions::AUDIO_LAYOUT_SD_LINES)
                            && input.v_active <= 576);
                    rate * if layout0 { 1 } else { 4 }
                }
                AudioPacketType::OneBitSample => rate,
                AudioPacketType::HighBitRate if alt.contains(Assumptions::HBR_RATE) => 4 * rate,
                AudioPacketType::HighBitRate => 16 * rate,
            }
        })
        .max()
        .unwrap_or(0);
    // ceil((load4/4 + 3000) · 1001/1000 · line · 200 / (p_khz·1000 · 201))
    let audio_packets = div_ceil(
        (load4 + 12_000) * 1001 * line * 200,
        4000 * p_khz * 1000 * 201,
    );
    let blank_audio_min = 32 + 32 * audio_packets;

    // Tribytes: bits per pixel 24 · k_cd / k_420, with k_cd = bpc/8 (1 for 4:2:2) and
    // k_420 = 2 for 4:2:0.
    let bpc = u128::from(input.depth.bits_per_primary());
    let k_420: u128 = if input.encoding == ColorFormat::YCbCr420 {
        2
    } else {
        1
    };
    let (kcd_n, kcd_d) = if input.encoding == ColorFormat::YCbCr422 {
        (1, 1)
    } else {
        (bpc, 8)
    };
    let bpp = 24 * kcd_n / (kcd_d * k_420);
    let tb_active = div_ceil(bpp * ha, 24);
    let tb_blank = div_ceil(hb * kcd_n, kcd_d * k_420);

    // Utilisation: blanking characters RC compression can shrink (not the guard bands, the
    // audio packets or the ACR packet), 7/8 of them less a margin of 4.
    let free = (hb * kcd_n / (kcd_d * k_420)).saturating_sub(32 * (1 + audio_packets) + 7);
    let savings = (7 * free / 8).saturating_sub(4);
    let payload = div_ceil(3 * tb_active, 2) + tb_blank - savings.min(tb_blank);
    // margin = 1 − payload / chars_per_line − o_n / o_d
    let margin = if chars_per_line == 0 {
        Margin::new(-1, 1)
    } else {
        let cl = chars_per_line as i128;
        Margin::new(
            cl * o_d as i128 - payload as i128 * o_d as i128 - o_n as i128 * cl,
            chars_per_line * o_d,
        )
    };

    let mut e = FrlEvaluation {
        readings: alt,
        chars_per_line: to_u32(chars_per_line),
        tribytes_active: to_u32(tb_active),
        tribytes_blank: to_u32(tb_blank),
        audio_packets_per_line: to_u32(audio_packets),
        tribytes_borrowed: 0,
        margin,
        result: Ok(()),
    };

    if blank_audio_min > tb_blank {
        e.result = Err(FrlShortfall::AudioBlanking {
            needed: to_u32(blank_audio_min),
            available: to_u32(tb_blank),
        });
        return e;
    }

    // Timing: the active part needs 1.5 · tb_active characters, the blanking tb_blank.
    // Compared in periods: h_active ≥ 1.5 · tb_active / k  ⇔  2 · h_active · k_n ≥
    // 3 · tb_active · k_d, and likewise for the blanking.
    let active_fits = 2 * ha * k_n >= 3 * tb_active * k_d;
    let blank_fits = hb * k_n >= tb_blank * k_d;
    if !active_fits {
        if !blank_fits {
            e.result = Err(FrlShortfall::Borrow);
            return e;
        }
        // Borrowed periods (1.5 · tb_active / k − h_active), at the average tribyte rate
        // (tb_active + tb_blank) / line per period.
        let borrowed = div_ceil(
            (3 * tb_active * k_d - 2 * ha * k_n) * (tb_active + tb_blank),
            2 * k_n * line,
        );
        e.tribytes_borrowed = to_u32(borrowed);
        if borrowed > u128::from(TB_BORROWED_MAX) {
            e.result = Err(FrlShortfall::BorrowLimit {
                borrowed: e.tribytes_borrowed,
                limit: TB_BORROWED_MAX,
            });
            return e;
        }
    } else if !blank_fits {
        e.result = Err(FrlShortfall::Borrow);
        return e;
    }

    let fits = if alt.contains(Assumptions::MARGIN_TOLERANCE) {
        // margin ≥ −1/100
        100 * margin.numer >= -(margin.denom as i128)
    } else {
        margin.numer > 0
    };
    if !fits {
        e.result = Err(FrlShortfall::Utilisation { margin });
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use display_types::cea861::vic_to_mode;

    // Expected values were derived with an exact rational transcription of amdgpu's
    // `dml1_frl_cap_chk_uncompressed` (seconds, Hz and bps, as written there; i915's
    // variants where an assumption selects them), independent of the integer rearrangement
    // above. That transcription also agreed with this module field by field on 3000 random
    // inputs (2026-10-25).

    const R3: HdmiForumFrl = HdmiForumFrl::Rate3Gbps3Lanes;
    const R6_3: HdmiForumFrl = HdmiForumFrl::Rate6Gbps3Lanes;
    const R6_4: HdmiForumFrl = HdmiForumFrl::Rate6Gbps4Lanes;
    const R12: HdmiForumFrl = HdmiForumFrl::Rate12Gbps4Lanes;

    fn rgb(
        rate: HdmiForumFrl,
        p_khz: u32,
        ha: u16,
        hb: u16,
        depth: ColorBitDepth,
    ) -> FrlCapacityInput<'static> {
        FrlCapacityInput::new(rate, p_khz, ha, hb, ColorFormat::Rgb444, depth)
    }

    const EIGHT_192K: AudioLoad = AudioLoad::new(AudioPacketType::Sample, 192_000, 8);
    const HBR_192K: AudioLoad = AudioLoad::new(AudioPacketType::HighBitRate, 192_000, 8);

    /// 3840×2160 @ 60 Hz (VIC 97: 594 MHz, h_blank 560), RGB 12-bit: the walkthrough mode.
    fn vic97_rgb12(rate: HdmiForumFrl) -> FrlCapacityReport {
        frl_capacity(&rgb(rate, 594_000, 3840, 560, ColorBitDepth::Depth12))
    }

    #[test]
    fn walkthrough_mode_across_rates() {
        // 3 Gbps × 3: 3684 characters per line for 5760 active tribytes (8640 characters);
        // the blanking cannot lend that much time.
        let r = vic97_rgb12(R3);
        assert_eq!(r.verdict(), FrlVerdict::Exceeded);
        assert_eq!(r.strict.result, Err(FrlShortfall::Borrow));

        // 6 Gbps × 3: it could borrow, but 2148 tribytes, over the 400 allowed.
        let r = vic97_rgb12(R6_3);
        assert_eq!(r.verdict(), FrlVerdict::Exceeded);
        assert_eq!(
            r.strict.result,
            Err(FrlShortfall::BorrowLimit {
                borrowed: 2148,
                limit: 400
            })
        );

        // 6 Gbps × 4: the lowest rate that fits. 9824 characters per line, 174 tribytes
        // borrowed, margin 1819267/22411000 ≈ +0.0812.
        let r = vic97_rgb12(R6_4);
        assert_eq!(r.verdict(), FrlVerdict::Fits);
        assert_eq!(r.strict.chars_per_line, 9824);
        assert_eq!(r.strict.tribytes_active, 5760);
        assert_eq!(r.strict.tribytes_blank, 840);
        assert_eq!(r.strict.audio_packets_per_line, 1);
        assert_eq!(r.strict.tribytes_borrowed, 174);
        assert_eq!(
            (r.strict.margin.numer(), r.strict.margin.denom()),
            (1_819_267, 22_411_000)
        );
        assert_eq!(r.strict.margin.ppm(), 81_177);
        assert!(r.deciding.is_empty());

        for rate in [
            HdmiForumFrl::Rate8Gbps4Lanes,
            HdmiForumFrl::Rate10Gbps4Lanes,
            R12,
        ] {
            assert_eq!(vic97_rgb12(rate).verdict(), FrlVerdict::Fits, "{rate:?}");
        }
    }

    #[test]
    fn borrow_limit_boundary() {
        // At 616.538 MHz the active video borrows exactly 400 tribytes: allowed.
        let r = frl_capacity(&rgb(R6_4, 616_538, 3840, 560, ColorBitDepth::Depth12));
        assert_eq!(r.verdict(), FrlVerdict::Fits);
        assert_eq!(r.strict.tribytes_borrowed, 400);

        // At 616.638 MHz it borrows 401 with the exact overhead; with i915's truncated
        // overhead it borrows less and fits. Only that assumption decides.
        let r = frl_capacity(&rgb(R6_4, 616_638, 3840, 560, ColorBitDepth::Depth12));
        assert_eq!(r.verdict(), FrlVerdict::Marginal);
        assert_eq!(r.deciding, Assumptions::OVERHEAD_TRUNCATION);
        assert_eq!(
            r.strict.result,
            Err(FrlShortfall::BorrowLimit {
                borrowed: 401,
                limit: 400
            })
        );
        assert!(r.permissive.result.is_ok());
    }

    #[test]
    fn audio_blanking_boundary() {
        // 8 channels at 192 kHz: 2 packets per line, so 32 + 2 × 32 = 96 blanking tribytes.
        let input = |hb| {
            rgb(R12, 533_250, 3840, hb, ColorBitDepth::Depth8)
                .with_v_active(2160)
                .with_audio(FrlAudio::AtMost(&[EIGHT_192K]))
        };
        let r = frl_capacity(&input(96));
        assert_eq!(r.verdict(), FrlVerdict::Fits);
        assert_eq!(r.strict.audio_packets_per_line, 2);

        // One short: fails with the audio, fits without it (ACR packets only: 64).
        let r = frl_capacity(&input(95));
        assert_eq!(r.verdict(), FrlVerdict::Marginal);
        assert_eq!(r.deciding, Assumptions::AUDIO_LOAD);
        assert_eq!(
            r.strict.result,
            Err(FrlShortfall::AudioBlanking {
                needed: 96,
                available: 95
            })
        );
        assert_eq!(r.permissive.audio_packets_per_line, 1);
    }

    #[test]
    fn margin_tolerance_alone_decides() {
        // 1920 active, 280 blanking, 10-bit at 3 Gbps × 3, 289.109 MHz: margin −579589/322113000 ≈ −0.0018,
        // below i915's > 0, within amdgpu's −1/100.
        let r = frl_capacity(&rgb(R3, 289_109, 1920, 280, ColorBitDepth::Depth10));
        assert_eq!(r.verdict(), FrlVerdict::Marginal);
        assert_eq!(r.deciding, Assumptions::MARGIN_TOLERANCE);
        assert!(matches!(
            r.strict.result,
            Err(FrlShortfall::Utilisation { .. })
        ));
        assert_eq!(
            (r.strict.margin.numer(), r.strict.margin.denom()),
            (-579_589, 322_113_000)
        );
        assert_eq!(r.strict.margin.ppm(), -1800);
    }

    #[test]
    fn chars_per_line_rounding_decides_with_the_margin_readings() {
        // Margin −0.000247 at 288.631 MHz: rounding the line's characters up, the tolerance
        // and the truncated overhead each turn it positive. Rounding only moves the margin,
        // so it never decides alone: wherever it does, the tolerance does too.
        let r = frl_capacity(&rgb(R3, 288_631, 1920, 280, ColorBitDepth::Depth10));
        assert_eq!(r.verdict(), FrlVerdict::Marginal);
        assert_eq!(
            r.deciding,
            Assumptions::CHARS_PER_LINE_ROUNDING
                | Assumptions::MARGIN_TOLERANCE
                | Assumptions::OVERHEAD_TRUNCATION
        );
    }

    #[test]
    fn sd_layout_decides_with_the_audio_load() {
        // 720×576, 100 blanking tribytes, 8 channels at 192 kHz: layout 1 needs 224, layout 0
        // fits; no audio fits too, so the audio load decides as well.
        let input = rgb(R3, 27_000, 720, 100, ColorBitDepth::Depth8)
            .with_v_active(576)
            .with_audio(FrlAudio::AtMost(&[EIGHT_192K]));
        assert!(
            input
                .applicable_assumptions()
                .contains(Assumptions::AUDIO_LAYOUT_SD_LINES)
        );
        let r = frl_capacity(&input);
        assert_eq!(r.verdict(), FrlVerdict::Marginal);
        assert_eq!(
            r.deciding,
            Assumptions::AUDIO_LAYOUT_SD_LINES | Assumptions::AUDIO_LOAD
        );
        assert_eq!(
            r.strict.result,
            Err(FrlShortfall::AudioBlanking {
                needed: 224,
                available: 100
            })
        );
    }

    #[test]
    fn hbr_rate_decides_with_the_audio_load() {
        // HBR at 4 × 192 kHz needs 224 blanking tribytes; at 192 kHz it fits in 96.
        let input = rgb(R12, 533_250, 3840, 96, ColorBitDepth::Depth8)
            .with_v_active(2160)
            .with_audio(FrlAudio::AtMost(&[HBR_192K]));
        let r = frl_capacity(&input);
        assert_eq!(r.verdict(), FrlVerdict::Marginal);
        assert_eq!(r.deciding, Assumptions::HBR_RATE | Assumptions::AUDIO_LOAD);
        assert_eq!(
            r.strict.result,
            Err(FrlShortfall::AudioBlanking {
                needed: 224,
                available: 96
            })
        );
    }

    #[test]
    fn dtd_border_reading_alone_decides() {
        // 1920 + 2 × 16 border, 80 blanking, 8-bit at 3 Gbps × 3, 332.996 MHz: EDID 1.3's
        // reading leaves margin −0.0118 (past even the tolerance), EDID 1.4's (96 more
        // blanking at the same clock) +0.0023.
        let input = rgb(R3, 332_996, 1952, 80, ColorBitDepth::Depth8).with_h_border(16);
        let r = frl_capacity(&input);
        assert_eq!(r.verdict(), FrlVerdict::Marginal);
        assert_eq!(r.deciding, Assumptions::DTD_BORDER_READING);
        assert!(
            r.permissive
                .readings
                .contains(Assumptions::DTD_BORDER_READING)
        );
    }

    #[test]
    fn applicable_assumptions() {
        let base = rgb(R12, 594_000, 3840, 560, ColorBitDepth::Depth8);
        let always = Assumptions::CHARS_PER_LINE_ROUNDING
            | Assumptions::MARGIN_TOLERANCE
            | Assumptions::OVERHEAD_TRUNCATION;
        assert_eq!(base.applicable_assumptions(), always);
        // Multichannel audio with the line count unknown: layout 1, no SD assumption.
        assert_eq!(
            base.with_audio(FrlAudio::AtMost(&[EIGHT_192K]))
                .applicable_assumptions(),
            always | Assumptions::AUDIO_LOAD
        );
        assert_eq!(
            base.with_h_border(8).applicable_assumptions(),
            always | Assumptions::DTD_BORDER_READING
        );
    }

    #[test]
    fn from_mode() {
        let mode = vic_to_mode(97).unwrap();
        let input = FrlCapacityInput::from_mode(
            &mode,
            R6_4,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth12,
            FrlAudio::None,
        )
        .unwrap();
        assert_eq!(
            (
                input.pixel_clock_khz,
                input.h_active,
                input.h_blank,
                input.v_active
            ),
            (594_000, 3840, 560, 2160)
        );
        assert_eq!(frl_capacity(&input), vic97_rgb12(R6_4));

        // Borders count as active.
        let mut bordered = mode.clone();
        bordered.h_border = 8;
        let input = FrlCapacityInput::from_mode(
            &bordered,
            R6_4,
            ColorFormat::Rgb444,
            ColorBitDepth::Depth12,
            FrlAudio::None,
        )
        .unwrap();
        assert_eq!((input.h_active, input.h_border), (3856, 8));

        let try_mode = |m: &VideoMode, rate| {
            FrlCapacityInput::from_mode(
                m,
                rate,
                ColorFormat::Rgb444,
                ColorBitDepth::Depth8,
                FrlAudio::None,
            )
        };
        assert_eq!(
            try_mode(&mode, HdmiForumFrl::NotSupported),
            Err(FrlUnverified::Tmds)
        );
        let mut no_blank = mode.clone();
        no_blank.h_blank = None;
        assert_eq!(try_mode(&no_blank, R6_4), Err(FrlUnverified::NoBlanking));
        let mut no_clock = mode.clone();
        no_clock.pixel_clock_khz = None;
        assert_eq!(try_mode(&no_clock, R6_4), Err(FrlUnverified::NoPixelClock));
        no_clock.pixel_clock_khz = Some(0);
        assert_eq!(try_mode(&no_clock, R6_4), Err(FrlUnverified::NoPixelClock));
    }

    #[test]
    fn margin_order_and_ppm() {
        let a = Margin::new(-1, 3);
        let b = Margin::new(1, 7);
        assert!(a < b);
        assert_eq!(Margin::new(2, 4), Margin::new(1, 2));
        assert_eq!(a.ppm(), -333_334); // rounded towards negative infinity
        assert_eq!(b.ppm(), 142_857);
    }

    #[test]
    fn heaviest_load_is_taken_per_reading() {
        // HBR declared at 96 kHz and 8-channel L-PCM at 192 kHz. With HBR at 4 × 96 kHz the
        // HBR load is the heavier (384 000 packets/s against 192 000); at 96 kHz it is the
        // lighter, and the L-PCM load must be counted instead.
        const HBR_96K: AudioLoad = AudioLoad::new(AudioPacketType::HighBitRate, 96_000, 8);
        let input = |audio| {
            rgb(R12, 594_000, 3840, 560, ColorBitDepth::Depth8)
                .with_v_active(2160)
                .with_audio(audio)
        };
        let both = frl_capacity(&input(FrlAudio::OneOf(&[HBR_96K, EIGHT_192K])));
        let lpcm = frl_capacity(&input(FrlAudio::OneOf(&[EIGHT_192K])));
        assert_eq!(both.applicable, lpcm.applicable | Assumptions::HBR_RATE);
        assert!(both.strict.audio_packets_per_line > lpcm.strict.audio_packets_per_line);
        assert_eq!(both.strict.audio_packets_per_line, 3);
        assert_eq!(lpcm.strict.audio_packets_per_line, 2);
        // The HBR_RATE reading's evaluation counts the L-PCM load, not the lighter HBR one.
        assert!(both.permissive.readings.contains(Assumptions::HBR_RATE));
        assert_eq!(both.permissive.audio_packets_per_line, 2);
    }

    #[test]
    fn audio_kinds() {
        let base = rgb(R12, 594_000, 3840, 560, ColorBitDepth::Depth8);
        // What the source may send: counted, no AUDIO_LOAD assumption.
        let one_of = base.with_audio(FrlAudio::OneOf(&[EIGHT_192K]));
        assert!(
            !one_of
                .applicable_assumptions()
                .contains(Assumptions::AUDIO_LOAD)
        );
        assert_eq!(frl_capacity(&one_of).permissive.audio_packets_per_line, 2);
        // An empty bound is no audio.
        let empty = base.with_audio(FrlAudio::AtMost(&[]));
        assert_eq!(
            empty.applicable_assumptions(),
            base.applicable_assumptions()
        );
        assert_eq!(frl_capacity(&empty), frl_capacity(&base));
    }

    #[test]
    fn descriptor_mapping() {
        use display_types::cea861::{AudioFormatInfo, AudioSampleRates as Rates};
        let sad = |format, channels, rates| {
            ShortAudioDescriptor::new(format, channels, rates, AudioFormatInfo::Raw(0))
        };
        let load = |t, hz, ch| AudioMapping::Load(AudioLoad::new(t, hz, ch));
        use AudioPacketType::{HighBitRate, OneBitSample, Sample};
        let all = Rates::HZ_32000 | Rates::HZ_48000 | Rates::HZ_96000 | Rates::HZ_192000;
        assert_eq!(
            AudioLoad::from_descriptor(&sad(AudioFormat::Lpcm, 8, all)),
            load(Sample, 192_000, 8)
        );
        // Compressed formats are two-channel IEC 61937 streams at the declared rate.
        assert_eq!(
            AudioLoad::from_descriptor(&sad(AudioFormat::Ac3, 6, Rates::HZ_48000)),
            load(Sample, 48_000, 2)
        );
        // Enhanced AC-3 at four times it.
        assert_eq!(
            AudioLoad::from_descriptor(&sad(AudioFormat::EnhancedAc3, 8, Rates::HZ_48000)),
            load(Sample, 192_000, 2)
        );
        // TrueHD and DTS-HD: HBR at 192 kHz whatever the declared rate (768 kHz MAT).
        for format in [AudioFormat::MlpTrueHd, AudioFormat::DtsHd] {
            assert_eq!(
                AudioLoad::from_descriptor(&sad(format, 8, Rates::HZ_48000)),
                load(HighBitRate, 192_000, 8)
            );
        }
        assert_eq!(
            AudioLoad::from_descriptor(&sad(AudioFormat::OneBitAudio, 6, Rates::HZ_44100)),
            load(OneBitSample, 44_100, 6)
        );
        assert_eq!(
            AudioLoad::from_descriptor(&sad(AudioFormat::Dst, 6, Rates::HZ_44100)),
            AudioMapping::Unmodelled(AudioFormat::Dst)
        );
        assert_eq!(
            AudioLoad::from_descriptor(&sad(AudioFormat::Lpcm, 2, Rates::empty())),
            AudioMapping::NoSampleRate
        );
    }
}
