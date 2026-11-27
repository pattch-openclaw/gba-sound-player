#!/usr/bin/env python3
"""Band-limited FFT resampler for the ULI2 source-preparation path.

Copied from the sibling `gba-audio` project's `eval/gba_dac_model.py`
(`fft_resample`, `_frac`, `resample`) so this repository no longer imports
anything from that checkout at asset-generation time. The arithmetic is kept
verbatim: the corpus recipe resamples 96 kHz / 48 kHz / 44.1 kHz material to
the codec's fixed 32,768 Hz, and reproducing the existing corpus means keeping
the same resampler, not merely a good one.

Only the FFT path is used by `scripts/uli_prep.py`; the streaming `soxr` arm and
the DAC model around it are deliberately not copied.

Requires numpy and scipy.
"""
import numpy as np


def _frac(v):
    from fractions import Fraction
    return Fraction(v).limit_denominator(1 << 16)


def fft_resample(x, fs_in, fs_out, pass_hz=None, stop_hz=None, pad_s=0.25):
    """Band-limited resampling by one whole-signal FFT per channel.

    Exact (0 dB, linear phase, no delay) up to ``pass_hz``, raised-cosine taper
    to 0 at ``stop_hz``, nothing above. Default stop = min(fs_in, fs_out)/2,
    pass = 0.98*stop (16056 Hz for 32768 Hz). Zero padding of ``pad_s`` seconds
    keeps the circular wrap out of the signal.
    """
    from scipy import fft as sfft

    x = np.asarray(x, dtype=np.float32)
    one_d = x.ndim == 1
    if one_d:
        x = x[:, None]
    ratio = _frac(fs_out) / _frac(fs_in)
    up, dn = ratio.numerator, ratio.denominator
    n = x.shape[0]
    n_out = int(round(n * float(ratio)))
    k = sfft.next_fast_len(int(np.ceil((n + int(pad_s * fs_in) + 1) / dn)))
    N_in, N_out = dn * k, up * k
    nyq = min(fs_in, fs_out) / 2.0
    stop = min(stop_hz or nyq, nyq)
    pas = pass_hz if pass_hz is not None else 0.98 * stop
    pas = min(pas, stop)
    nb = min(N_in, N_out) // 2 + 1
    f = np.arange(nb) * (float(fs_in) / N_in)
    w = np.ones(nb, dtype=np.float32)
    tr = (f > pas) & (f < stop)
    if stop > pas:
        w[tr] = 0.5 * (1 + np.cos(np.pi * (f[tr] - pas) / (stop - pas)))
    w[f >= stop] = 0.0
    w *= np.float32(N_out / N_in)
    out = np.empty((n_out, x.shape[1]), dtype=np.float32)
    for c in range(x.shape[1]):
        X = sfft.rfft(x[:, c], N_in)
        Y = np.zeros(N_out // 2 + 1, dtype=X.dtype)
        Y[:nb] = X[:nb] * w
        del X
        y = sfft.irfft(Y, N_out)
        del Y
        out[:, c] = y[:n_out]
        del y
    return out[:, 0] if one_d else out


def resample(x, fs_in, fs_out, pass_hz=None, stop_hz=None):
    """Resample ``x`` to ``fs_out`` through :func:`fft_resample`.

    ``x`` is (n,) or (n, ch), float. A same-rate request is a dtype-preserving
    copy, matching the sibling helper's behaviour.
    """
    if abs(fs_in - fs_out) < 1e-9 and pass_hz is None and stop_hz is None:
        return np.asarray(x, dtype=np.float64 if x.dtype == np.float64 else np.float32)
    r = _frac(fs_out) / _frac(fs_in)
    if r.denominator <= 1 << 16 and abs(float(r) - fs_out / fs_in) < 1e-9:
        return fft_resample(x, fs_in, fs_out, pass_hz, stop_hz)
    # The sibling falls back to soxr or polyphase for odd ratios. The ULI2
    # corpus uses musical rates whose ratio is exact at this denominator; keep
    # the refusal explicit rather than silently changing the resampler.
    raise ValueError(f"uli_resample: no exact FFT ratio for {fs_in} -> {fs_out}")
