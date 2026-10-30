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
//! evaluations are the worst and the best of them. For amdgpu's procedure and the named
//! points that band is the honest extent of what is known; i915's integer arithmetic
//! truncates at further steps (the line time in nanoseconds, the character rate rounded to
//! whole Mchar/s, and others) that are not modelled, so an i915 result can fall outside
//! it. A timing that fits one way and not the other is
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
use display_types::{ColorBitDepth, ColorFormat, DtdBorderReading, VideoMode};

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
        /// FRL characters per line: the exact value floored (amdgpu), or, when set, rounded
        /// up (the direction i915 rounds; i915's value also passes through truncated
        /// intermediate steps, which are not modelled).
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
        /// borders and their reading was not settled
        /// ([`dtd_border_reading`][FrlCapacityInput::dtd_border_reading] is `None`).
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
    /// (FFmpeg's IEC 61937 muxer, `libavformat/spdifenc.c`, for AC-3, E-AC-3, MPEG, AAC,
    /// DTS and TrueHD). Where the descriptor does not say which variant the source sends,
    /// the heavier one is taken:
    ///
    /// | Format | Load |
    /// |---|---|
    /// | L-PCM | sample packets, the declared channels, the highest declared rate |
    /// | AC-3, MPEG-1, MP3, MPEG-2 multichannel, AAC LC, DTS, ATRAC, WMA Pro | sample packets, 2 channels, the highest declared rate (IEC 61937 at the audio rate; ATRAC and WMA Pro by the same rule, not in FFmpeg's muxer) |
    /// | Enhanced AC-3 | 2 channels at 4 × the highest declared rate: sample packets up to 192 kHz, High Bit Rate packets above (amdgpu switches above 192 kHz) |
    /// | DTS-HD, MLP/Dolby TrueHD | High Bit Rate packets at 192 kHz (768 kHz on the link: MAT and DTS-HD Master Audio), whatever the declared rate |
    /// | One Bit Audio, DST, extended and reserved codes | [`AudioMapping::Unmodelled`] |
    ///
    /// One Bit Audio and DST are not modelled: amdgpu refuses One Bit Audio sample packets
    /// and counts DST packets at a quarter packet per sample, a single source no other
    /// implementation corroborates; FFmpeg's muxer and i915 have neither. A descriptor of
    /// a modelled format other than DTS-HD and TrueHD with no sample rate set is
    /// [`AudioMapping::NoSampleRate`].
    pub fn from_descriptor(sad: &ShortAudioDescriptor) -> AudioMapping {
        let sample = |rate, channels| AudioLoad::new(AudioPacketType::Sample, rate, channels);
        let load = match (sad.format, highest_rate(sad.sample_rates)) {
            (AudioFormat::DtsHd | AudioFormat::MlpTrueHd, _) => {
                AudioLoad::new(AudioPacketType::HighBitRate, 192_000, sad.max_channels)
            }
            (AudioFormat::Lpcm, Some(rate)) => sample(rate, sad.max_channels),
            (
                AudioFormat::Ac3
                | AudioFormat::Mpeg1
                | AudioFormat::Mp3
                | AudioFormat::Mpeg2Multichannel
                | AudioFormat::AacLc
                | AudioFormat::Dts
                | AudioFormat::Atrac
                | AudioFormat::WmaPro,
                Some(rate),
            ) => sample(rate, 2),
            (AudioFormat::EnhancedAc3, Some(rate)) if 4 * rate <= 192_000 => sample(4 * rate, 2),
            (AudioFormat::EnhancedAc3, Some(rate)) => {
                AudioLoad::new(AudioPacketType::HighBitRate, rate, 2)
            }
            (
                AudioFormat::Lpcm
                | AudioFormat::Ac3
                | AudioFormat::Mpeg1
                | AudioFormat::Mp3
                | AudioFormat::Mpeg2Multichannel
                | AudioFormat::AacLc
                | AudioFormat::Dts
                | AudioFormat::Atrac
                | AudioFormat::WmaPro
                | AudioFormat::EnhancedAc3,
                None,
            ) => return AudioMapping::NoSampleRate,
            (other, _) => return AudioMapping::Unmodelled(UnmodelledAudio::Format(other)),
        };
        AudioMapping::Load(load)
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

/// Audio a sink accepts whose packet load no public implementation models; it cannot be
/// bounded, and the FRL capacity check reports it.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnmodelledAudio {
    /// A Short Audio Descriptor's format (One Bit Audio, DST, extended and reserved codes).
    Format(AudioFormat),
    /// HDMI Multi-Stream Audio: several streams at once (HDMI Audio Data Block).
    MultiStream {
        /// The most streams the sink accepts at once.
        max_streams: u8,
    },
    /// HDMI 3D Audio, up to 32 channels (HDMI Audio Data Block).
    Audio3d {
        /// The number of 3D audio descriptors.
        descriptors: u8,
    },
}

/// What [`AudioLoad::from_descriptor`] makes of a Short Audio Descriptor.
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMapping {
    /// The heaviest load a source may send for it.
    Load(AudioLoad),
    /// A format whose packet load no public implementation models; it cannot be bounded.
    Unmodelled(UnmodelledAudio),
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
    /// The sink accepts audio whose packet load no public implementation models, so the
    /// audio is bounded without it (see [`UnmodelledAudio`]).
    UnmodelledAudio(UnmodelledAudio),
    /// The sink's audio is unknown, so the video was checked without audio:
    /// [`SinkCapabilities::audio_loads`][crate::SinkCapabilities] was not filled in
    /// (`None`), or the build has no alloc (see [`FrlAudio`]).
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
            Self::UnmodelledAudio(audio) => {
                write!(f, "audio {audio:?} has no packet model and is not counted")
            }
            Self::AudioUnknown => f.write_str("the sink's audio is unknown"),
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
    /// The reading of the borders, when the decoder's caller settled it
    /// ([`VideoMode::dtd_border_reading`]): `h_blank` follows it, and the
    /// [`DTD_BORDER_READING`][Assumptions::DTD_BORDER_READING] assumption does not apply.
    /// `None` when it was not settled.
    pub dtd_border_reading: Option<DtdBorderReading>,
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
            dtd_border_reading: None,
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

    /// Records the settled reading of the borders, returning the updated input.
    /// `h_blank` must already follow it.
    pub const fn with_dtd_border_reading(mut self, reading: DtdBorderReading) -> Self {
        self.dtd_border_reading = Some(reading);
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
    /// `h_active` is the mode's width plus both horizontal borders, `v_active` its height
    /// plus both vertical borders (as amdgpu counts the active lines). Both saturate at
    /// `u16::MAX`, which no real timing reaches. The mode's
    /// [`dtd_border_reading`][VideoMode::dtd_border_reading] is kept.
    ///
    /// # Errors
    ///
    /// [`FrlUnverified::Tmds`] for [`HdmiForumFrl::NotSupported`],
    /// [`FrlUnverified::UnknownRate`] for a rate added after this release,
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
        if !known_encoding(encoding) {
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
            dtd_border_reading: mode.dtd_border_reading,
            v_active: mode.height.saturating_add(2 * u16::from(mode.v_border)),
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
        if self.h_border > 0 && self.dtd_border_reading.is_none() {
            a |= Assumptions::DTD_BORDER_READING;
        }
        a
    }
}

/// An exact signed fraction: the share of a line's FRL characters left over.
///
/// Always in lowest terms with a positive denominator, and both parts within
/// [`MARGIN_LIMIT`][Self::MARGIN_LIMIT], so comparisons and [`ppm`][Self::ppm] cannot
/// overflow. Deserializing checks the same: it reduces the fraction and rejects a zero
/// denominator or parts beyond the limit.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "MarginParts"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Margin {
    numer: i128,
    denom: u128,
}

/// A `Margin` as serialized, before it is checked.
#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct MarginParts {
    numer: i128,
    denom: u128,
}

#[cfg(feature = "serde")]
impl TryFrom<MarginParts> for Margin {
    type Error = &'static str;

    fn try_from(p: MarginParts) -> Result<Self, Self::Error> {
        let limit = Margin::MARGIN_LIMIT;
        if p.denom == 0 {
            Err("margin denominator is zero")
        } else if p.numer.unsigned_abs() > limit || p.denom > limit {
            Err("margin numerator or denominator out of range")
        } else {
            Ok(Margin::new(p.numer, p.denom))
        }
    }
}

impl Margin {
    /// The largest numerator magnitude or denominator a `Margin` holds, 2⁶². The
    /// computation's margins stay below about 1.4 × 10¹⁸ (< 2⁶¹) for every input, and
    /// within it the cross-multiplication in `cmp` and the scaling in `ppm` fit `i128`.
    pub const MARGIN_LIMIT: u128 = 1 << 62;

    fn new(numer: i128, denom: u128) -> Self {
        debug_assert!(denom > 0);
        debug_assert!(numer.unsigned_abs() <= Self::MARGIN_LIMIT && denom <= Self::MARGIN_LIMIT);
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
            Self::Utilisation { margin_ppm } => write!(
                f,
                "the line's characters exceed its capacity (margin {margin_ppm} ppm)"
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
        /// The margin left over in parts per million, rounded towards negative infinity
        /// (negative, or under the strict reading zero). The exact fraction is the
        /// evaluation's [`margin`][FrlEvaluation::margin]; this keeps the shortfall, and
        /// the violations and warnings carrying it, small.
        margin_ppm: i32,
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
    ///
    /// The count fields saturate at `u32::MAX` (reached only at absurd inputs, e.g. a
    /// pixel clock of a few kHz); the verdict uses the exact values.
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
    /// the one with the lowest margin. On equal margins the first evaluated is kept, so
    /// its [`readings`][FrlEvaluation::readings] may include lenient ones that made no
    /// difference.
    pub strict: FrlEvaluation,
    /// The best evaluation: a passing one with the highest margin if any passes,
    /// otherwise the one with the highest margin (ties as for `strict`).
    pub permissive: FrlEvaluation,
    /// The assumptions whose reading changes whether the timing fits, for at least one
    /// combination of the other readings (flipping it alone, the others held there).
    /// Empty unless the verdict is [`Marginal`][FrlVerdict::Marginal], and never empty when
    /// it is: between a passing and a failing combination some single flip changes the
    /// outcome. An assumption that only decides together with another (rounding, which
    /// only moves the margin, always comes with the margin tolerance) is listed with it.
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
/// added after this release), the pixel clock is 0, or the colour format is not one the
/// computation knows (RGB, YCbCr 4:4:4, 4:2:2, 4:2:0). [`FrlCapacityInput::from_mode`]
/// rules out all three.
pub fn frl_capacity(input: &FrlCapacityInput<'_>) -> FrlCapacityReport {
    assert!(
        lanes_and_gbps(input.rate).is_some(),
        "frl_capacity needs a known FRL rate"
    );
    assert!(
        input.pixel_clock_khz > 0,
        "frl_capacity needs a pixel clock"
    );
    assert!(
        known_encoding(input.encoding),
        "frl_capacity needs a known colour format"
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

/// The colour formats the computation knows.
fn known_encoding(encoding: ColorFormat) -> bool {
    matches!(
        encoding,
        ColorFormat::Rgb444 | ColorFormat::YCbCr444 | ColorFormat::YCbCr422 | ColorFormat::YCbCr420
    )
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
    // Blanking that does not fit has no time to lend. (It fails whether or not the active
    // part fits: the active part needs 1.5 times the blanking's characters per period.)
    if !blank_fits {
        e.result = Err(FrlShortfall::Borrow);
        return e;
    }
    if !active_fits {
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
    }

    let fits = if alt.contains(Assumptions::MARGIN_TOLERANCE) {
        // margin ≥ −1/100
        100 * margin.numer >= -(margin.denom as i128)
    } else {
        margin.numer > 0
    };
    if !fits {
        let margin_ppm = margin.ppm().clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        e.result = Err(FrlShortfall::Utilisation { margin_ppm });
    }
    e
}

#[cfg(test)]
#[path = "frl_tests.rs"]
mod tests;
