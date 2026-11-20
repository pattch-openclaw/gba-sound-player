//! S5: the resampler, held to the standard E-R2 gives it — flat level, a stopband
//! well under the encoder's own distortion, and no byte claim.
//!
//! The two claims that matter to the rest of the encoder are here rather than in a
//! measurement log: **32,768 Hz is a byte-for-byte copy** (so a native-rate WAV
//! reaches the identical blob), and **the passband is transparent enough that the
//! resample cannot be what a fidelity number is measuring**.

use uli_enc::resample::{RATE, resample_stereo};

fn sine(rate: u32, freq: f64, n_frames: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(n_frames * 2);
    for i in 0..n_frames {
        let v = (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin() as f32;
        out.extend_from_slice(&[v, v]);
    }
    out
}

#[test]
fn the_product_rate_is_an_unfiltered_copy() {
    let x = sine(RATE, 1000.0, 1000);
    let y = resample_stereo(&x, RATE).expect("copies");
    assert_eq!(x, y, "a natively 32,768 Hz source must not be touched");
    assert_eq!(
        x.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        y.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        "not even a rounding difference"
    );
}

#[test]
fn the_length_is_the_same_duration() {
    for (rate, n) in [
        (44100u32, 4410usize),
        (48000, 4800),
        (22050, 2205),
        (8000, 800),
    ] {
        let x = vec![0f32; n * 2];
        let y = resample_stereo(&x, rate).expect("resamples");
        let expected = n * RATE as usize / rate as usize;
        assert_eq!(y.len(), expected * 2, "{rate} Hz: {n} frames");
    }
}

#[test]
fn dc_gain_is_exactly_one() {
    // A level shift would read as an encoder bug forever, so each phase's taps are
    // normalised. 12,000 is a mid-scale PCM16 value.
    for rate in [44100u32, 48000, 22050] {
        let x = vec![12000f32; 440 * 2];
        let y = resample_stereo(&x, rate).expect("resamples");
        // Skip the zero-padded edges; the interior is what a listener hears.
        let interior = &y[64 * 2..y.len() - 64 * 2];
        let max_dev = interior
            .iter()
            .map(|v| (v - 12000.0).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_dev < 0.05,
            "{rate} Hz: DC level moved by {max_dev} PCM16 units"
        );
    }
}

/// SNR of `y` against an ideal 32,768 Hz render of the same tone, over the interior.
fn snr_db(y: &[f32], freq: f64) -> f64 {
    let start = 64usize;
    let end = y.len() / 2 - 64;
    let mut se = 0f64;
    let mut sx = 0f64;
    for n in start..end {
        let want = (2.0 * std::f64::consts::PI * freq * n as f64 / f64::from(RATE)).sin();
        let got = f64::from(y[n * 2]);
        se += (want - got) * (want - got);
        sx += want * want;
    }
    10.0 * (sx / se).log10()
}

#[test]
fn a_passband_tone_survives_either_direction() {
    // 44,100 → 32,768 (down) and 22,050 → 32,768 (up), at 1 kHz: deep in the
    // passband in both, so the number is the filter's quality, not its corner.
    for rate in [44100u32, 22050] {
        let x = sine(rate, 1000.0, (rate / 2) as usize);
        let y = resample_stereo(&x, rate).expect("resamples");
        let snr = snr_db(&y, 1000.0);
        assert!(snr > 80.0, "{rate} Hz → 32,768 at 1 kHz: SNR {snr:.1} dB");
    }
}

#[test]
fn the_guard_band_rejects_what_the_target_rate_cannot_carry() {
    // 44,100 Hz can hold a 20 kHz tone, 32,768 Hz cannot hold anything above
    // 16,384. Downsampling without a filter would fold it back into the band; the
    // Kaiser window plus the 0.95 guard is what keeps it out.
    let x = sine(44100, 20000.0, 22050);
    let y = resample_stereo(&x, 44100).expect("resamples");
    let interior = &y[64 * 2..y.len() - 64 * 2];
    let rms_in = (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt();
    let rms_out = (interior.iter().map(|v| v * v).sum::<f32>() / interior.len() as f32).sqrt();
    let db = 20.0 * (rms_out / rms_in).log10();
    assert!(db < -40.0, "20 kHz survived downsampling at {db:.1} dB");
}

#[test]
fn the_channels_are_filtered_independently() {
    // Left a tone, right silence: any cross-talk shows up as energy on the right.
    let n = 2205usize;
    let mut x = Vec::with_capacity(n * 2);
    for i in 0..n {
        let v = (2.0 * std::f64::consts::PI * 500.0 * i as f64 / 44100.0).sin() as f32;
        x.extend_from_slice(&[v, 0.0]);
    }
    let y = resample_stereo(&x, 44100).expect("resamples");
    let right_peak = y
        .iter()
        .skip(1)
        .step_by(2)
        .map(|v| v.abs())
        .fold(0.0f32, f32::max);
    assert_eq!(right_peak, 0.0, "channel separation is exact");
    let left_peak = y.iter().step_by(2).map(|v| v.abs()).fold(0.0f32, f32::max);
    assert!(
        left_peak > 0.9,
        "the left channel kept its tone ({left_peak})"
    );
}
