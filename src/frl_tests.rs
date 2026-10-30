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

/// 3840×2160 @ 120 Hz (VIC 118: 1188 MHz, h_blank 560), no audio.
fn vic118(rate: HdmiForumFrl, encoding: ColorFormat, depth: ColorBitDepth) -> FrlCapacityReport {
    frl_capacity(&FrlCapacityInput::new(
        rate, 1_188_000, 3840, 560, encoding, depth,
    ))
}

#[test]
fn ycbcr420_halves_the_tribytes() {
    // 4:2:0 12-bit: 18 bits per pixel, so 2880 active and 420 blanking tribytes.
    let r = vic118(R3, ColorFormat::YCbCr420, ColorBitDepth::Depth12);
    assert_eq!(r.verdict(), FrlVerdict::Exceeded);
    assert_eq!(r.strict.result, Err(FrlShortfall::Borrow));

    let r = vic118(R6_3, ColorFormat::YCbCr420, ColorBitDepth::Depth12);
    assert_eq!(r.verdict(), FrlVerdict::Exceeded);
    assert_eq!(
        r.strict.result,
        Err(FrlShortfall::BorrowLimit {
            borrowed: 1074,
            limit: 400
        })
    );

    // 6 Gbps × 4: 4912 characters per line, 87 tribytes borrowed, margin
    // 3337409/44822000 ≈ +0.0745.
    let r = vic118(R6_4, ColorFormat::YCbCr420, ColorBitDepth::Depth12);
    assert_eq!(r.verdict(), FrlVerdict::Fits);
    assert_eq!(r.strict.chars_per_line, 4912);
    assert_eq!(r.strict.tribytes_active, 2880);
    assert_eq!(r.strict.tribytes_blank, 420);
    assert_eq!(r.strict.audio_packets_per_line, 1);
    assert_eq!(r.strict.tribytes_borrowed, 87);
    assert_eq!(
        (r.strict.margin.numer(), r.strict.margin.denom()),
        (3_337_409, 44_822_000)
    );
    assert_eq!(r.strict.margin.ppm(), 74_459);

    // 10-bit at 6 Gbps × 3 borrows within the limit (346), but the payload does not fit
    // the line: margin −1980067/69689000 ≈ −0.0284, past the tolerance too.
    let r = vic118(R6_3, ColorFormat::YCbCr420, ColorBitDepth::Depth10);
    assert_eq!(r.verdict(), FrlVerdict::Exceeded);
    assert_eq!(r.strict.tribytes_borrowed, 346);
    assert_eq!(
        (r.strict.margin.numer(), r.strict.margin.denom()),
        (-1_980_067, 69_689_000)
    );
    assert_eq!(
        r.strict.result,
        Err(FrlShortfall::Utilisation {
            margin_ppm: -28_413
        })
    );
}

#[test]
fn ycbcr422_is_24_bits_at_any_depth() {
    // 4:2:2: 24 bits per pixel whatever the depth, so 3840 active and 560 blanking
    // tribytes, as RGB 8-bit.
    let r = vic118(R6_4, ColorFormat::YCbCr422, ColorBitDepth::Depth12);
    assert_eq!(r.verdict(), FrlVerdict::Exceeded);
    assert_eq!(
        r.strict.result,
        Err(FrlShortfall::BorrowLimit {
            borrowed: 1435,
            limit: 400
        })
    );

    // 8 Gbps × 4: 6549 characters per line, 116 tribytes borrowed, margin
    // 18578447/239038500 ≈ +0.0777.
    for depth in [ColorBitDepth::Depth8, ColorBitDepth::Depth12] {
        let r = vic118(HdmiForumFrl::Rate8Gbps4Lanes, ColorFormat::YCbCr422, depth);
        assert_eq!(r.verdict(), FrlVerdict::Fits, "{depth:?}");
        assert_eq!(r.strict.chars_per_line, 6549);
        assert_eq!(r.strict.tribytes_active, 3840);
        assert_eq!(r.strict.tribytes_blank, 560);
        assert_eq!(r.strict.tribytes_borrowed, 116);
        assert_eq!(
            (r.strict.margin.numer(), r.strict.margin.denom()),
            (18_578_447, 239_038_500)
        );
        assert_eq!(r.strict.margin.ppm(), 77_721);
    }
}

#[test]
fn better_and_worse_prefer_by_result_then_margin() {
    let eval = |result, numer| FrlEvaluation {
        readings: Assumptions::empty(),
        chars_per_line: 0,
        tribytes_active: 0,
        tribytes_blank: 0,
        audio_packets_per_line: 0,
        tribytes_borrowed: 0,
        margin: Margin::new(numer, 100),
        result,
    };
    let pass = eval(Ok(()), -1);
    let fail = eval(Err(FrlShortfall::Borrow), 5);
    // A passing evaluation wins over a failing one whatever the margins, in either order.
    assert_eq!(better(fail, pass), pass);
    assert_eq!(better(pass, fail), pass);
    assert_eq!(worse(fail, pass), fail);
    assert_eq!(worse(pass, fail), fail);
    // Otherwise the margin decides.
    let higher = eval(Ok(()), 2);
    assert_eq!(better(pass, higher), higher);
    assert_eq!(worse(pass, higher), pass);
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

    // Settled by the decoder's caller, one reading is evaluated: no longer marginal.
    let settled_13 = input.with_dtd_border_reading(DtdBorderReading::Edid13);
    let r = frl_capacity(&settled_13);
    assert_eq!(r.verdict(), FrlVerdict::Exceeded);
    assert_eq!(r.deciding, Assumptions::empty());
    let settled_14 = rgb(R3, 332_996, 1952, 80 + 96, ColorBitDepth::Depth8)
        .with_h_border(16)
        .with_dtd_border_reading(DtdBorderReading::Edid14);
    let r = frl_capacity(&settled_14);
    assert_eq!(r.verdict(), FrlVerdict::Fits);
    assert_eq!(r.deciding, Assumptions::empty());
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
    // A settled reading is not an assumption.
    assert_eq!(
        base.with_h_border(8)
            .with_dtd_border_reading(DtdBorderReading::Edid14)
            .applicable_assumptions(),
        always
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

    // Borders count as active, horizontally and vertically.
    let mut bordered = mode.clone();
    bordered.h_border = 8;
    bordered.v_border = 4;
    let input = FrlCapacityInput::from_mode(
        &bordered,
        R6_4,
        ColorFormat::Rgb444,
        ColorBitDepth::Depth12,
        FrlAudio::None,
    )
    .unwrap();
    assert_eq!(
        (input.h_active, input.h_border, input.v_active),
        (3856, 8, 2168)
    );
    assert_eq!(input.dtd_border_reading, None);
    let settled = bordered
        .clone()
        .with_dtd_border_reading(DtdBorderReading::Edid14);
    let input = FrlCapacityInput::from_mode(
        &settled,
        R6_4,
        ColorFormat::Rgb444,
        ColorBitDepth::Depth12,
        FrlAudio::None,
    )
    .unwrap();
    assert_eq!(input.dtd_border_reading, Some(DtdBorderReading::Edid14));

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
    use AudioPacketType::{HighBitRate, Sample};
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
    // Enhanced AC-3 at four times it: sample packets up to 192 kHz, HBR above.
    assert_eq!(
        AudioLoad::from_descriptor(&sad(AudioFormat::EnhancedAc3, 8, Rates::HZ_48000)),
        load(Sample, 192_000, 2)
    );
    assert_eq!(
        AudioLoad::from_descriptor(&sad(AudioFormat::EnhancedAc3, 8, Rates::HZ_96000)),
        load(HighBitRate, 96_000, 2)
    );
    // TrueHD and DTS-HD: HBR at 192 kHz whatever the declared rate (768 kHz MAT).
    for format in [AudioFormat::MlpTrueHd, AudioFormat::DtsHd] {
        assert_eq!(
            AudioLoad::from_descriptor(&sad(format, 8, Rates::HZ_48000)),
            load(HighBitRate, 192_000, 8)
        );
    }
    // ... and without a declared rate too: their load does not depend on it.
    assert_eq!(
        AudioLoad::from_descriptor(&sad(AudioFormat::MlpTrueHd, 8, Rates::empty())),
        load(HighBitRate, 192_000, 8)
    );
    // One Bit Audio and DST: no corroborated model; unmodelled, with or without rate.
    for format in [AudioFormat::OneBitAudio, AudioFormat::Dst] {
        for rates in [Rates::HZ_44100, Rates::empty()] {
            assert_eq!(
                AudioLoad::from_descriptor(&sad(format, 6, rates)),
                AudioMapping::Unmodelled(UnmodelledAudio::Format(format))
            );
        }
    }
    assert_eq!(
        AudioLoad::from_descriptor(&sad(AudioFormat::Lpcm, 2, Rates::empty())),
        AudioMapping::NoSampleRate
    );
}

#[cfg(feature = "serde")]
#[test]
fn margin_deserializes_only_valid_fractions() {
    let m: Margin = serde_json::from_str(r#"{"numer":2,"denom":4}"#).unwrap();
    assert_eq!(m, Margin::new(1, 2)); // reduced, so == and cmp agree
    assert!(serde_json::from_str::<Margin>(r#"{"numer":1,"denom":0}"#).is_err());
    let huge = alloc::format!(r#"{{"numer":{},"denom":1}}"#, Margin::MARGIN_LIMIT + 1);
    assert!(serde_json::from_str::<Margin>(&huge).is_err());
    let round_trip: Margin =
        serde_json::from_str(&serde_json::to_string(&Margin::new(-3, 7)).unwrap()).unwrap();
    assert_eq!(round_trip, Margin::new(-3, 7));
}
