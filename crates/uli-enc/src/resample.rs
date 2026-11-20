//! The rate half of the WAV front end: any rate → 32,768 Hz.
//!
//! **This is a convenience, and it is not in the byte contract.** E-R2 holds
//! byte-equality at the PCM boundary and explicitly exempts the resample, because
//! the reference's own preparation dispatches across libraries (`prep.py` →
//! `gba_dac_model.resample`, FFT path or `soxr` or `scipy`) and the same flags can
//! therefore produce different bytes on two machines. So this module's job is a
//! *good* resample — flat passband, steep enough stopband that the encoder's own
//! distortion dwarfs it — and its witness is SNR, not a hash.
//!
//! The design is a polyphase windowed sinc: a Kaiser-windowed sinc of
//! `2 · HALF` taps, evaluated per phase, table normalised so DC gain is exactly
//! one (a resampler that quietly shifted the album's level by 0.2 dB would look
//! like an encoder bug forever). `up`/`down` are the reduced ratio
//! `32_768 : rate`, so a common rate costs a small table — 44,100 Hz gives
//! `up = 8,192`, 48,000 Hz gives `up = 1,024`.
//!
//! Determinism (R5b): output sample `n` reads `n · down / up`, sums its taps in
//! ascending index order, and touches no shared state — one thread or many, the
//! same f32 bits come out.
//!
//! `rate == 32_768` returns the input **unchanged**, without building a filter:
//! that is the path a natively-32,768 Hz WAV takes, and it is why such a file
//! reaches the identical blob its `album.f32` would (S5's gate).

use crate::Error;

/// The one rate the format plays at (`uli_lite::format::RATE`).
pub const RATE: u32 = 32_768;

/// Half-width of the sinc, in source samples per side. 16 zero crossings each way
/// is 32 taps per phase: ~90 dB of stopband with this window, at a cost that is
/// invisible next to the encoder's own work.
const HALF: usize = 16;

/// Kaiser β. 8.0 with 32 taps puts the first sidelobe near −90 dB; the guard band
/// (below) is what keeps that rejection where the content can reach it.
const BETA: f64 = 8.0;

/// The passband stops at 0.95 of the lower Nyquist. Downsampling needs the guard
/// so the transition band lands below 32,768 Hz's Nyquist rather than across it.
const CUTOFF: f64 = 0.475;

/// Resample interleaved stereo `x` from `rate` to [`RATE`].
///
/// Length is `n_in · up / down` interleaved frames — the same duration, rounded
/// down by less than one output sample. A rate equal to [`RATE`] is a
/// byte-for-byte copy (and no filter table is built).
pub fn resample_stereo(x: &[f32], rate: u32) -> Result<Vec<f32>, Error> {
    if rate == 0 {
        return Err(Error::Wav("a sample rate of 0 Hz".to_string()));
    }
    if rate == RATE {
        return Ok(x.to_vec());
    }
    let g = gcd(RATE, rate);
    let up = RATE / g;
    let down = rate / g;
    // The lowpass corner belongs to **the lower of the two Nyquists**: downsampling
    // must cut before 32,768 Hz's Nyquist or the content above it folds back in;
    // upsampling only has to reject the images above the source's.
    let cut = CUTOFF * (f64::from(up) / f64::from(down)).min(1.0);
    let filter = Filter::new(up, down, cut);
    let n_frames = x.len() / 2;
    let out_frames = filter.out_frames(n_frames);
    let mut out = vec![0f32; out_frames * 2];
    for n in 0..out_frames {
        // `n · down` is the source position of this output sample in `up`-ths of a
        // sample: the integer part is the tap origin, the remainder is the phase.
        let t = n as u64 * down as u64;
        let i0 = (t / up as u64) as isize;
        let phase = (t % up as u64) as usize;
        let taps = filter.phase(phase);
        for ch in 0..2 {
            let mut acc = 0f32;
            for (k, &h) in taps.iter().enumerate() {
                // Tap `k` reads `i0 + k − (HALF − 1)`; outside the input is silence,
                // the same zero-padding the analysis window applies at the edges.
                let i = i0 + k as isize - (HALF as isize - 1);
                let s = if i >= 0 && (i as usize) < n_frames {
                    x[i as usize * 2 + ch]
                } else {
                    0.0
                };
                acc += s * h;
            }
            out[n * 2 + ch] = acc;
        }
    }
    Ok(out)
}

/// One polyphase filter: `up` phases of `2 · HALF` taps each.
struct Filter {
    up: u32,
    down: u32,
    table: Vec<f32>,
}

impl Filter {
    fn new(up: u32, down: u32, cut: f64) -> Self {
        let taps = 2 * HALF;
        let mut table = vec![0f32; up as usize * taps];
        let i0 = bessel_i0(BETA);
        let mut scratch = vec![0f64; taps];
        for p in 0..up {
            let frac = p as f64 / up as f64;
            let mut sum = 0f64;
            for (k, s) in scratch.iter_mut().enumerate() {
                // Distance from the output position to tap k, in source samples.
                let d = k as f64 - (HALF as f64 - 1.0) - frac;
                let window = if d.abs() < HALF as f64 {
                    bessel_i0(BETA * (1.0 - (d / HALF as f64).powi(2)).max(0.0).sqrt()) / i0
                } else {
                    0.0
                };
                // Normalised sinc: sin(πx)/(πx) with x = 2·fc·d.
                let x = 2.0 * cut * d;
                let sinc = if x.abs() < 1e-12 {
                    1.0
                } else {
                    (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                };
                *s = sinc * window;
                sum += *s;
            }
            // Unity DC gain per phase. Without this the level would move with the
            // rate ratio, which reads as an encoder bug from every angle.
            if sum.abs() > 1e-9 {
                for s in scratch.iter_mut() {
                    *s /= sum;
                }
            }
            for (k, &s) in scratch.iter().enumerate() {
                table[p as usize * taps + k] = s as f32;
            }
        }
        Self { up, down, table }
    }

    fn out_frames(&self, n_in: usize) -> usize {
        n_in * self.up as usize / self.down as usize
    }

    fn phase(&self, p: usize) -> &[f32] {
        let taps = 2 * HALF;
        &self.table[p * taps..(p + 1) * taps]
    }
}

/// `I₀(z)`, by its power series: all terms positive, so there is no cancellation
/// and 40 terms is plenty for the range this window uses (z ≤ 8).
fn bessel_i0(z: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    let half_z = z * 0.5;
    for k in 1..40 {
        term *= (half_z / k as f64).powi(2);
        sum += term;
    }
    sum
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}
