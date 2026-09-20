#!/usr/bin/env python3
"""frame_vectors.py — deterministic source synthesis + independent FLAC frame
header parser, used to generate the frame-header golden vectors.

Subcommands:

  synth SRC_DIR            write the deterministic source WAVs into SRC_DIR
  emit  SRC_DIR OUT_FILE   parse SRC_DIR/*.flac, write the vector table
  emit_bodies SRC_DIR OUT  walk whole subframes of the body-vector streams,
                          write the subframe-body table (step 3e ground truth)
  spike_assets SRC_DIR DIR stage the perf-gate spike's embedded clips (raw
                          frame regions, reference PCM, generated manifest;
                          arm censuses fail closed — FLAC.md "Perf gate spike")

Why this exists: the frame-header golden vectors must not be hand-packed.
Hand-packing is exactly how the "fixed fields are 31 bits / channels is 3 bits"
error got baked into the scaffold and then *asserted* by a unit test (FLAC.md,
"Frame header: measured byte layout"). Real encoder output cannot be wrong
about what encoders emit.

Why the parser is independent: expected values are derived here, from RFC 9639
section 9.1, by reading bits — never by asking libFLAC what it produced. The
bug this harness was built for is a mis-transcription of the spec into code;
an encoder-derived oracle would have reproduced that mis-transcription
faithfully. This file and crates/flac-lite are two independent readings of the
same spec, and the vectors are where they meet.

Self-checks that make a wrong parser loud instead of silent:
  * every frame of every encode must have a verifying header CRC-8
  * frame numbers must be exactly 0..N-1, each appearing once
  * frame count must equal ceil(total_samples / blocksize) from STREAMINFO

Determinism: the source PCM is synthesized with integer arithmetic only (no
float, no RNG, periods chosen to divide the sample rate exactly), so the bytes
are identical on every platform. The *vectors* still depend on libFLAC's
per-frame choices, so the generating libFLAC version is recorded in the output.

Stdlib only. `flac`/`metaflac` must be on PATH for the encodes (run via
scripts/gen_frame_vectors.sh).
"""

import math
import os
import struct
import subprocess
import sys
import wave

# ------------------------------------------------------------- RFC 9639 tables

# 9.1.1 block size bits -> samples per subframe. "u8"/"u16" = uncommon block
# size minus 1, stored big-endian after the coded number.
BLOCKSIZE = {
    0b0000: None,
    0b0001: 192,
    0b0010: 576, 0b0011: 1152, 0b0100: 2304, 0b0101: 4608,
    0b0110: "u8", 0b0111: "u16",
    0b1000: 256, 0b1001: 512, 0b1010: 1024, 0b1011: 2048,
    0b1100: 4096, 0b1101: 8192, 0b1110: 16384, 0b1111: 32768,
}

# 9.1.2 sample rate bits -> Hz. None = "only stored in STREAMINFO"; the
# "uncommon-*" codes carry the value after the coded number; 0b1111 is forbidden.
SAMPLERATE = {
    0b0000: None,
    0b0001: 88200, 0b0010: 176400, 0b0011: 192000,
    0b0100: 8000, 0b0101: 16000, 0b0110: 22050, 0b0111: 24000,
    0b1000: 32000, 0b1001: 44100, 0b1010: 48000, 0b1011: 96000,
    0b1100: "uncommon-kHz", 0b1101: "uncommon-Hz", 0b1110: "uncommon-Hz/10",
    0b1111: "FORBIDDEN",
}

# 9.1.3 channels bits -> (subframes in frame, decorrelation). NOTE: 4 bits wide.
# 0b0010..0b0111 are 3..8-channel and 0b1011..0b1111 are reserved; all are
# outside our profile, and all become unrepresentable if this field is misread
# as 3 bits — which is the scaffold bug this harness caught. There is no
# "swapped mid/side" variant in FLAC: 0b1000 left-side, 0b1001 side-right,
# 0b1010 mid-side, each unambiguous.
CHANNELS = {
    0b0000: (1, "none"),
    0b0001: (2, "none"),
    0b0010: (3, "unsupported"), 0b0011: (4, "unsupported"),
    0b0100: (5, "unsupported"), 0b0101: (6, "unsupported"),
    0b0110: (7, "unsupported"), 0b0111: (8, "unsupported"),
    0b1000: (2, "left-side"), 0b1001: (2, "side-right"),
    0b1010: (2, "mid-side"),
    0b1011: (0, "reserved"), 0b1100: (0, "reserved"), 0b1101: (0, "reserved"),
    0b1110: (0, "reserved"), 0b1111: (0, "reserved"),
}

# 9.1.4 bit depth bits (3 bits) -> bits per sample; None = from STREAMINFO,
# and 0b011 is reserved.
SAMPLESIZE = {
    0b000: None,
    0b001: 8, 0b010: 12, 0b011: None,
    0b100: 16, 0b101: 20, 0b110: 24, 0b111: 32,
}

# 9.2.1 subframe type bits (6 bits, following the leading 0 bit).
SUBFRAME = {}
for _v in range(64):
    if _v == 0:
        SUBFRAME[_v] = "constant"
    elif _v == 1:
        SUBFRAME[_v] = "verbatim"
    elif _v <= 7:
        SUBFRAME[_v] = "reserved"
    elif _v <= 12:
        SUBFRAME[_v] = "fixed%d" % (_v - 8)
    elif _v <= 31:
        SUBFRAME[_v] = "reserved"
    else:
        SUBFRAME[_v] = "lpc%d" % (_v - 31)


def crc8(raw):
    """RFC 9639 9.1.8: init 0, poly x^8 + x^2 + x + 1 (0x07), no reflection,
    no final xor. Table-free on purpose (a 256-byte table is ROM we could spend
    elsewhere) — matches crates/flac-lite's planned `crc8`."""
    crc = 0
    for byte in raw:
        crc ^= byte
        for _ in range(8):
            if crc & 0x80:
                crc = ((crc << 1) ^ 0x07) & 0xFF
            else:
                crc = (crc << 1) & 0xFF
    return crc


# ---------------------------------------------------------------- synthesis

def tri(index, period):
    """Integer triangle wave in [-1000, 1000]. `period` MUST divide the stream's
    sample rate exactly, so the waveform is periodic in samples and the whole
    synthesis is exact integer arithmetic — no float, no RNG, no accumulated
    rounding drift, byte-identical on any platform."""
    x = index % period
    half = period // 2
    if x < half:
        v = (2000 * x) // half - 1000
    else:
        v = 1000 - (2000 * (x - half)) // half
    return v


def square(index, period, duty_pct=50):
    return 1000 if (index % period) * 100 < period * duty_pct else -1000


def hash_noise(index, mult, span):
    """Deterministic cheap pseudo-noise: integer, position-dependent, stable."""
    return ((index * mult) % (2 * span + 1)) - span


def hash_pcm(count):
    """Incompressible full-range source for the VERBATIM vectors: the top 16
    bits of the Knuth multiplicative hash, i * 2654435761 mod 2**32 >> 16,
    offset to signed.

    Why this beats VERBATIM (measured, libFLAC 1.5.0, `flac -a` census): FIXED
    order >= 1 predicts from neighbour differences; uncorrelated full-range
    noise defeats even order 1, so the residual costs more bits than the raw
    samples and the encoder falls back to verbatim -- at `-l 0` every frame of
    this source encodes VERBATIM with wasted_bits 0. The same source at the
    default `-l 12` encodes zero verbatim frames (LPC wins): `-l 0` is the
    switch, and the generator's fail-closed census pins both facts.

    Why the math (not a fetched random blob): the multiplier is odd, so the
    map is a bijection mod 2**32 -- every 16-bit value appears exactly once
    per 65536 samples. Guaranteed full range, guaranteed no clipping, exact
    integer arithmetic, no RNG: byte-identical on every platform, like every
    other source here. A fetched asset could not be regenerated byte-exact,
    which is the harness rule this project's witness discipline demands."""
    return [(((i * 2654435761) & 0xFFFFFFFF) >> 16) - 32768
            for i in range(count)]


def synth_pcm(seconds, rate, center_partials, side_period, side_weight,
              noise_l, noise_r, mono=False, exact=None):
    """Correlated stereo (or mono) material tuned to make the *encoder* choose
    the features the vectors are supposed to demonstrate.

    Two properties, both measured (see the `measure`-adjacent notes in FLAC.md,
    "What the encoder actually chooses"):

    * **L = center + side, R = center - side.** Mid/side-shaped correlation is
      what makes libFLAC actually pick decorrelated channel assignments; with
      uncorrelated channels it encodes plain independent L/R and the mid-side
      vector simply never appears.
    * **Enough broadband content that linear prediction wins.** A too-pure
      tonal source is predicted perfectly by a low-order FIXED predictor, so
      `-l 4` and `-l 0` encodes look identical (both FIXED) and the vectors
      would *appear* to prove the opposite of what they claim. With the noise
      floor below, `-l 4` picks LPC order 3-4 on essentially every frame while
      `-l 0` stays FIXED -- which is the `-l` semantics the profile must state.

    Integer arithmetic only (no float accumulation, no RNG), so the bytes are
    identical on every platform and every run.
    """
    # `exact` overrides the float-derived count: the tail-length streams need a
    # precise sample count (it decides the final frame's blocksize), and a float
    # multiply must never be allowed to shave one sample off it.
    count = exact if exact is not None else int(seconds * rate)
    square_period = 32
    samples = []
    for i in range(count):
        center = sum(weight * tri(i, period) // 1000
                     for period, weight in center_partials)
        center += 2 * square(i, square_period) // 1000
        side = (side_weight * tri(i, side_period)) // 1000 \
            + hash_noise(i, 7919, noise_l)
        if mono:
            samples.append(max(-32768, min(32767, center + side)))
            continue
        left = center + side + hash_noise(i, 104729, noise_r)
        right = center - side + hash_noise(i, 15485863, noise_r)
        samples.append(max(-32768, min(32767, left)))
        samples.append(max(-32768, min(32767, right)))
    return samples


def write_wav(path, rate, channels, samples):
    with wave.open(path, "wb") as wav:
        wav.setnchannels(channels)
        wav.setsampwidth(2)
        wav.setframerate(rate)
        wav.writeframes(struct.pack("<%dh" % len(samples), *samples))


# Center partials as (period_in_samples, weight). Determinism comes from exact
# integer arithmetic alone, so periods need not divide the sample rate; most
# here do (32000 = 2^9*5^3, 65536 = 2^16) but 192 does not, deliberately --
# non-periodic partials are part of what pushes the encoder toward LPC, and
# pinning every period to a divisor would quietly change the encoder's choices.
PARTIALS_32K = [(128, 5), (192, 4), (250, 3), (80, 2)]
PARTIALS_64K = [(64, 5), (128, 4), (256, 3), (512, 2)]


# Streams whose *final frame* cannot be expressed by the 4-bit blocksize
# table, so libFLAC must emit the uncommon ("get 8/16-bit") form. Both sample
# counts are chosen against a 2048 blocksize:
#   321000 = 156*2048 + 1512  -> 1512 is not a table size -> 16-bit form
#   20580  = 10*2048  + 100   -> 100  ""                       -> 8-bit form
TAIL16_SAMPLES = 321000
TAIL8_SAMPLES = 20580

# Rates with no place in the 4-bit table, carried by the uncommon sample-rate
# codes. Measured on libFLAC 1.5.0: none of these need `--lax` (unlike 65536,
# which has no code at all and travels as `0b0000` + stream default).
RATE_STREAMS = (("rate_khz.wav", 56000, 0.25),    # 0b1100, kHz as 8-bit
                ("rate_hz.wav", 48001, 0.25),     # 0b1101, Hz as 16-bit
                ("rate_hz10.wav", 10010, 0.25))   # 0b1110, Hz/10 as 16-bit


# Frame-run vector geometry (step 3f part 2): 356 samples @ -b 256 -> frames
# [256, 100]. 100 is not a table blocksize (uncommon 8-bit code), and a
# 100-sample decorrelated tail into a 256-slot buffer is exactly what forces
# the tail contract to be witnessed, not assumed.
RUN_SAMPLES = 356


def _clamp16(v):
    # -32768..32767, the signed 16-bit domain (asymmetric: symmetric clamps
    # before negations produced an out-of-range sample in an early probe).
    return max(-32768, min(32767, v))


def _tri_amp(index, period, amp):
    """Integer triangle in [-amp, amp]; same exact-integer shape as tri()."""
    x = index % period
    half = period // 2
    if x < half:
        return (2 * amp * x) // half - amp
    return amp - (2 * amp * (x - half)) // half


def _sum_tri(index):
    """Smooth multi-period material (peaks well beyond 16-bit -- callers
    clamp). Chosen by measurement (run-census drafts): it keeps libFLAC on
    predictors and lets the stereo baits below keep their channel modes."""
    return (3 * _tri_amp(index, 512, 4000) + 3 * _tri_amp(index, 131, 3500)
            + 4 * _tri_amp(index, 97, 4500) + 2 * _tri_amp(index, 61, 4000)
            + 2 * _tri_amp(index, 32, 2000))


def _hash_span(index, span):
    return ((index * 2654435761) & 0xFFFFFFFF) % (2 * span + 1) - span


def run_sideright_pcm(count):
    """side/right (0b1001) bait: R smooth, L = R + tiny QUANTIZED noise.
    Side = L-R is then cheap AND strictly more expensive to code than R,
    which is what makes libFLAC store side-first (measured 63/63 frames at
    the default level, 3/3 at run size; step 3f part 1's orientation
    witness). Clamp BEFORE quantizing: _sum_tri peaks beyond 16-bit."""
    samples = []
    for i in range(count):
        r = _clamp16(_sum_tri(i))
        l = _clamp16(r + ((_hash_span(i, 60) >> 4) << 4))
        samples += [l, r]
    return samples


def run_leftside_pcm(count):
    """left/side (0b1000) bait: the mirror -- L smooth (quantized-cheap),
    R = L + tiny noise, so side is cheap and L strictly more compressible
    (measured 2/2 frames at run size)."""
    samples = []
    for i in range(count):
        l = _clamp16(_sum_tri(i))
        r = _clamp16(l + ((_hash_span(i, 60) >> 4) << 4))
        samples += [l, r]
    return samples


def run_indep_pcm(count):
    """Independent stereo (0b0001) that COMPRESSES: two uncorrelated smooth
    channels (disjoint partial sets + small disjoint noise), so the encoder
    keeps 0b0001 and codes both with predictors instead of collapsing to
    verbatim (measured: fixed1 subframes, 2/2 frames at run size). The
    independent-2f vector needs a 0b0001 run, and hash-noise stereo would
    have given verbatim-only frames."""
    samples = []
    for i in range(count):
        l = _clamp16(_tri_amp(i, 512, 9000) + _tri_amp(i, 131, 4000)
                     + ((i * 104729) % 601) - 300)
        r = _clamp16(_tri_amp(i, 307, 8000) + _tri_amp(i, 71, 5000)
                     + ((i * 15485863) % 601) - 300)
        samples += [l, r]
    return samples


def synth(dst):
    write_wav(os.path.join(dst, "stereo.wav"), 32000, 2,
              synth_pcm(10.0, 32000, PARTIALS_32K, 1000, 3, 700, 400))
    write_wav(os.path.join(dst, "mono.wav"), 32000, 1,
              synth_pcm(10.0, 32000, PARTIALS_32K, 1000, 3, 700, 400, mono=True))
    write_wav(os.path.join(dst, "r65k.wav"), 65536, 2,
              synth_pcm(10.0, 65536, PARTIALS_64K, 1024, 3, 700, 400))
    write_wav(os.path.join(dst, "silence.wav"), 32000, 2, [0] * (32000 * 2 * 2))
    # Wasted-bits witness stream: a coarse square wave has zero LSBs by
    # construction, so libFLAC signals wasted bits in the subframe header
    # (measured, 1.5.0: FIXED order 1 + wasted 5 on every frame). Pure integer
    # synthesis like everything else here, so the bytes are platform-stable.
    write_wav(os.path.join(dst, "wasted_square.wav"), 32000, 1,
              [20000 if (i // 300) % 2 == 0 else -20000
               for i in range(32000)])
    # Short-tail streams: mono, same synthesis, lengths chosen so the final
    # frame forces each uncommon blocksize form.
    for name, count in (("tail16.wav", TAIL16_SAMPLES), ("tail8.wav", TAIL8_SAMPLES)):
        write_wav(os.path.join(dst, name), 32000, 1,
                  synth_pcm(0, 32000, PARTIALS_32K, 1000, 3, 700, 400,
                            mono=True, exact=count))
    for name, rate, seconds in RATE_STREAMS:
        write_wav(os.path.join(dst, name), rate, 1,
                  synth_pcm(seconds, rate, PARTIALS_32K, 1000, 3, 700, 400,
                            mono=True))
    # VERBATIM witness source (header-table vector): 1 s of incompressible
    # hash material, encoded `-l 0` so every frame is VERBATIM (census-checked
    # fail-closed in emit_body_streams / the l4-vs-l0 census note in emit).
    write_wav(os.path.join(dst, "verbatim.wav"), 32000, 1, hash_pcm(32000))
    # Subframe-body vector sources (step 3e): one 256-sample mono stream per
    # subframe type. One frame each, so the type census of the stream IS the
    # frame's type; 256 keeps verbatim bodies (256 x 16 bits = 512 B) small
    # enough to commit as hex in the table.
    write_wav(os.path.join(dst, "verbatim256.wav"), 32000, 1, hash_pcm(256))
    write_wav(os.path.join(dst, "constant256.wav"), 32000, 1, [0] * 256)
    # Period 30, NOT the 300 of the 10s wasted_square stream: with 300 only
    # one flip would fit in 256 samples and the stream would be *constant*
    # (a first draft of this line was exactly that bug). 30 flips 8 times
    # per frame; ±20000 = 2**5 x 625 keeps the zero-LSB structure that makes
    # libFLAC signal wasted bits.
    write_wav(os.path.join(dst, "square256.wav"), 32000, 1,
              [20000 if (i // 30) % 2 == 0 else -20000 for i in range(256)])
    write_wav(os.path.join(dst, "tonal256.wav"), 32000, 1,
              synth_pcm(0, 32000, PARTIALS_32K, 1000, 3, 700, 400,
                        mono=True, exact=256))
    # LPC + wasted-bits witness (step 3e scale discriminator): tonal256
    # quantized to multiples of 32 (zero low 5 bits by construction, so
    # libFLAC signals wasted bits) while keeping the broadband content that
    # keeps LPC winning at -l 4. Measured libFLAC 1.5.0: LPC order 3 +
    # wasted 5, shift 7 -- the committed stream that makes padding ORDER
    # load-bearing (predict-then-pad matches flac -d; padding before the
    # LPC recurrence mismatches at 252/256 samples; see FLAC.md step 3e).
    write_wav(os.path.join(dst, "lpcw256.wav"), 32000, 1,
              [(s >> 5) << 5 for s in synth_pcm(0, 32000, PARTIALS_32K,
                                                1000, 3, 700, 400,
                                                mono=True, exact=256)])
    # Smooth triangle (no noise floor, no square): the fixed-order lottery.
    # Whichever FIXED order libFLAC picks here (measured: order 2), the vector
    # is emitted optionally with a uniform-census gate.
    tri_period = 128
    write_wav(os.path.join(dst, "smooth256.wav"), 32000, 1,
              [tri(i, tri_period) for i in range(256)])
    # Frame-RUN sources (step 3f part 2): five 356-sample streams -> exactly
    # 2 frames at -b 256 (256 + 100; the 100 tail forces the uncommon 8-bit
    # blocksize form, and is short enough to make the decorrelate tail
    # contract -- samples beyond blocksize must survive -- load-bearing).
    # One stream per channel assignment: mono, independent, and each of the
    # three decorrelation modes, uniform per construction (fail-closed census
    # in emit_runs). Encoded at the DEFAULT compression level: the fast
    # stereo heuristic at -1 collapses every construction to mid/side; the
    # exhaustive per-frame search (default level) is what actually compares
    # modes (measured, libFLAC 1.5.0; the run-census drafts recorded all
    # three modes winning their baits on every frame).
    for name, samples in (("run_mono.wav",
                           synth_pcm(0, 32000, PARTIALS_32K, 1000, 3, 700,
                                     400, mono=True, exact=RUN_SAMPLES)),
                          ("run_indep.wav", run_indep_pcm(RUN_SAMPLES)),
                          ("run_leftside.wav", run_leftside_pcm(RUN_SAMPLES)),
                          ("run_sideright.wav", run_sideright_pcm(RUN_SAMPLES)),
                          ("run_midside.wav",
                           synth_pcm(0, 32000, PARTIALS_32K, 1000, 3, 700,
                                     400, exact=RUN_SAMPLES))):
        write_wav(os.path.join(dst, name), 32000, 2 if name != "run_mono.wav" else 1,
                  samples)
    print("   synthesized stereo/mono/r65k/silence/tails/rates/verbatim/body/run WAVs in %s" % dst)


# ---------------------------------------------------------------- parsing

def streaminfo(path):
    out = subprocess.run(["metaflac", "--list", "--block-type=STREAMINFO", path],
                         capture_output=True, text=True, check=True).stdout
    info = {}
    for line in out.splitlines():
        key, _, value = line.partition(":")
        key = key.strip()
        if key in ("maximum blocksize", "sample_rate", "channels",
                   "bits-per-sample", "total samples"):
            info[key] = int(value.strip().split()[0])
    return info


def first_frame_offset(data):
    assert data[:4] == b"fLaC", "not a FLAC stream"
    off = 4
    while True:
        is_last = data[off] & 0x80
        length = int.from_bytes(data[off + 1:off + 4], "big")
        off += 4 + length
        if is_last:
            return off


def coded_number_octets(lead):
    """RFC 9639 9.1.5 lead byte -> total octets; None when not a legal lead."""
    if lead < 0x80:
        return 1
    if lead < 0xC0:
        return None                 # 0b10xxxxxx: stray continuation, not a lead
    if lead < 0xE0:
        return 2
    if lead < 0xF0:
        return 3
    if lead < 0xF8:
        return 4
    if lead < 0xFC:
        return 5
    if lead < 0xFE:
        return 6
    if lead == 0xFE:
        return 7                    # legal 7-byte lead (only 0xFF is invalid)
    return None


def parse_frame_header(data, pos):
    """Strict RFC 9639 9.1 frame header. Returns a dict, or None when the bytes
    at `pos` are not a structurally valid header.

    Field widths, in order: sync 14, reserved 1, blocking 1, blocksize 4,
    sample rate 4, channels 4, sample size 3, reserved 1 = **32 bits**. Every
    field after that is whole octets (coded number, optional uncommon
    blocksize/rate, CRC-8), which is why the CRC-8 is always byte aligned.
    """
    if pos + 8 > len(data):
        return None
    bits = "".join(format(x, "08b") for x in data[pos:pos + 4])
    sync = int(bits[0:14], 2)
    if sync != 0x3FFE:                  # 14 ones ending in 0 = fixed blocksize
        return None
    reserved1 = int(bits[14], 2)
    blocking = int(bits[15], 2)
    bs_code = int(bits[16:20], 2)
    rate_code = int(bits[20:24], 2)
    chan_code = int(bits[24:28], 2)
    size_code = int(bits[28:31], 2)
    reserved2 = int(bits[31], 2)
    if reserved1 or reserved2 or blocking or rate_code == 0b1111:
        return None
    if BLOCKSIZE[bs_code] is None or size_code == 0b011:
        return None
    if CHANNELS[chan_code][0] == 0:
        return None
    octets = coded_number_octets(data[pos + 4])
    if octets is None:
        return None
    raw = data[pos + 4:pos + 4 + octets]
    if len(raw) < octets or any(b & 0xC0 != 0x80 for b in raw[1:]):
        return None
    value = raw[0] & (0x7F if octets == 1 else (0xFF >> (octets + 1)))
    for byte in raw[1:]:
        value = (value << 6) | (byte & 0x3F)
    # Uncommon block size / sample rate follow the coded number, big-endian,
    # block size first (9.1.6, 9.1.7). Block size is stored minus one; the rate
    # carries its own unit per code (kHz 8-bit, Hz 16-bit, Hz/10 16-bit) --
    # never 24-bit. Walking the tail in field order is what keeps the CRC-8
    # position right when both uncommon fields are present at once.
    tail = pos + 4 + octets
    extra = 0
    blocksize_resolved = BLOCKSIZE[bs_code]
    if bs_code == 0b0110:
        blocksize_resolved = data[tail] + 1
        tail += 1
        extra += 1
    elif bs_code == 0b0111:
        blocksize_resolved = int.from_bytes(data[tail:tail + 2], "big") + 1
        tail += 2
        extra += 2
    rate_resolved = SAMPLERATE[rate_code]
    if rate_code == 0b1100:
        rate_resolved = data[tail] * 1000
        tail += 1
        extra += 1
    elif rate_code in (0b1101, 0b1110):
        unit = 1 if rate_code == 0b1101 else 10
        rate_resolved = int.from_bytes(data[tail:tail + 2], "big") * unit
        tail += 2
        extra += 2
    crc_at = 4 + octets + extra
    if pos + crc_at + 2 > len(data):
        return None
    crc_byte = data[pos + crc_at]
    sub0_byte = data[pos + crc_at + 1]
    subframe0 = ("invalid" if (sub0_byte & 0x80)
                 else SUBFRAME[(sub0_byte >> 1) & 0x3F])
    # 9.2.1/9.2.2 subframe-0 header witness: the type field is pad(1) + code(6)
    # = 7 bits, so the wasted-bits flag is bit 7 (LSB of the byte after the
    # CRC-8) and its unary run continues from the MSB of the NEXT byte. Read
    # exactly that: kind/order derive from the code; wasted is flag + zeros.
    if subframe0 in ("invalid", "reserved"):
        sf_kind, sf_order, sf_wasted = subframe0, None, None
    else:
        if subframe0 == "constant":
            sf_kind, sf_order = "constant", 0
        elif subframe0 == "verbatim":
            sf_kind, sf_order = "verbatim", 0
        elif subframe0.startswith("fixed"):
            sf_kind, sf_order = "fixed", int(subframe0[5:])
        else:
            sf_kind, sf_order = "lpc", int(subframe0[3:])
        if sub0_byte & 1 == 0:
            sf_wasted = 0
        else:
            k, bi = 0, pos + crc_at + 2
            while True:
                byte = data[bi]
                j = 0
                while j < 8 and (byte >> (7 - j)) & 1 == 0:
                    k += 1
                    j += 1
                if j < 8:
                    sf_wasted = k + 1  # terminated by a one at bit j
                    break
                bi += 1  # an all-zeros byte just extends the unary run
                assert bi < len(data), "%s: unterminated wasted-bits unary" % pos
    return dict(
        offset=pos,
        header=data[pos:pos + crc_at + 1],
        blocksize_code=bs_code,
        blocksize=blocksize_resolved,
        rate_code=rate_code,
        rate_hz=rate_resolved,
        chan_code=chan_code,
        subframes=CHANNELS[chan_code][0],
        decorrelation=CHANNELS[chan_code][1],
        size_code=size_code,
        bps=SAMPLESIZE[size_code],
        number=value,
        octets=octets,
        extra_bytes=extra,
        crc_pos=crc_at,
        crc_bit=8 * crc_at,
        crc_byte=crc_byte,
        crc_ok=crc_byte == crc8(data[pos:pos + crc_at]),
        header_bits=8 * (crc_at + 1),
        subframe0=subframe0,
        subframe0_kind=sf_kind,
        subframe0_order=sf_order,
        subframe0_wasted=sf_wasted,
        subframe0_bytes=data[pos + crc_at + 1:pos + crc_at + 3],
    )


def parse_lenient(data, pos):
    """A deliberately SLOPPY header reader: sync shape + a coded number length,
    then a CRC-8 check and nothing else -- no reserved-bit, channel-code,
    sample-size-code or blocking-strategy validation.

    Exists only to measure what a CRC alone buys (`measure` below). It is the
    shape of "scan for sync, trust the CRC" that the Phase 1 spike plan
    originally implied, and it is wrong in a specific, easy-to-miss way: it
    accepts payload bytes that happen to look like a valid header and happen to
    CRC, while `parse_frame_header` refuses them on field validity alone.
    """
    if pos + 8 > len(data):
        return None
    if data[pos] != 0xFF or (data[pos + 1] & 0xF8) != 0xF8:
        return None
    octets = coded_number_octets(data[pos + 4])
    if octets is None or octets > 3:
        return None
    crc_at = 4 + octets
    if pos + crc_at + 1 > len(data):
        return None
    if data[pos + crc_at] != crc8(data[pos:pos + crc_at]):
        return None
    return {"offset": pos}


def find_frames(data):
    """Locate the real frames of a FLAC stream.

    Method: sync shape (0xFF, next byte & 0xF8 == 0xF8) narrowed by STRICT
    structural validation and a verifying header CRC-8. Measured on our profile
    (`scripts/frame_vectors.py measure`, and `check_stream` re-proves it for
    every encode here) that combination is exact: correct count AND coded frame
    numbers exactly 0..N-1 in stream order, across 979 frames.

    Two things this measurement also proved, both of which contradict
    assumptions this harness started from -- see FLAC.md,
    "Frame sync scanning: what actually filters".

      * Sync shape alone over-matches ~1.3-12x (204 candidates for 157 frames
        up to 1568 for 313), and CRC-8 *without* field validation still admits
        a handful of false positives. The strict field checks are load-bearing,
        not belt-and-braces.
      * Do NOT narrow candidates by requiring STREAMINFO's maximum blocksize.
        The LAST frame of a track legitimately carries a shorter block size
        than STREAMINFO's max (320000 samples at 2048 -> 156 full frames + one
        512-sample frame). Filtering on max blocksize silently drops the final
        frame of every track whose length is not a whole multiple of the block.

    What this is NOT: not a decoder, not a shipped seek strategy. The decoder
    never scans -- the GAFP manifest hands it exact offsets (Phase 2 step 6).
    Scanning lives here only because this harness has no manifest.
    """
    start = first_frame_offset(data)
    found = []
    for i in range(start, len(data) - 9):
        if data[i] != 0xFF or (data[i + 1] & 0xF8) != 0xF8:
            continue
        header = parse_frame_header(data, i)
        if header and header["crc_ok"]:
            found.append(header)
    return found


# ---------------------------------------------------------------- emit

# (label, stream file, selector, required?, why)
#
# `required` vectors pin coverage the decoder must have regardless of encoder
# version. Optional vectors record whatever that version happened to choose —
# they document observed behaviour, and regeneration is expected to move them.
VECTOR_SPECS = [
    ("stereo-6byte-first", "l4_stereo.flac", lambda f: f[0], True,
     "6-byte header: 1-octet coded frame number, CRC-8 at bit 40"),
    ("stereo-7byte-num128", "l4_stereo.flac",
     lambda f: next(h for h in f if h["number"] == 128), True,
     "7-byte header: 2-octet coded number (>=128 forces the wider form)"),
    ("stereo-last", "l4_stereo.flac", lambda f: f[-1], True,
     "final frame: short blocksize (the tail is 512, not the track's 2048) AND "
     "a 7-byte coded number -- the frame most decoders get wrong"),
    ("mono-b1024-first", "l4_mono.flac", lambda f: f[0], True,
     "mono: channels code 0b0000, one subframe per frame"),
    ("r65k-rate-from-stream", "r65k.flac", lambda f: f[0], True,
     "sample-rate code 0b0000: 65536 Hz has no 4-bit code, so it lives only in "
     "STREAMINFO -> the FromStreamDefault path the GBA 65kHz goal depends on"),
    ("fixed-only-first", "l0_stereo.flac", lambda f: f[0], True,
     "`-l 0` encode: FIXED-only subframes, the perf gate's FIXED arm"),
    ("subframe-wasted-bits", "wasted_square.flac", lambda f: f[0], False,
     "subframe header WITH wasted bits: coarse square wave encodes FIXED-1 + "
     "wasted 5 (measured, libFLAC 1.5.0). Witnesses that SubframeType::parse "
     "consumes pad+type (7 bits) and stops exactly before the wasted flag"),
    ("silence-constant", "l4_silence.flac", lambda f: f[0], False,
     "CONSTANT subframes (digitally silent input)"),
    ("stereo-midside", "l4_stereo.flac",
     lambda f: next(h for h in f if h["decorrelation"] == "mid-side"), False,
     "mid/side decorrelation: channels code 0b1010"),
    # --- uncommon blocksize forms (9.1.6): the header is only parseable if the
    # appended value is consumed at the right cursor position. Both are final
    # frames, which is exactly where they occur in practice.
    ("tail-uncommon16", "tail16.flac", lambda f: f[-1], True,
     "final frame of 1512 samples: no table size fits, so blocksize code "
     "0b0111 + 16-bit (blocksize minus 1) after the coded number"),
    ("tail-uncommon8", "tail8.flac", lambda f: f[-1], True,
     "final frame of 100 samples: blocksize code 0b0110 + 8-bit value"),
    # --- uncommon sample rates (9.1.7): each code carries its own unit, and
    # the value sits after the uncommon blocksize when both are present.
    ("rate-khz-8bit", "rate_khz.flac", lambda f: f[0], True,
     "56000 Hz: no table code, so 0b1100 + rate in kHz as an 8-bit number"),
    ("rate-hz-16bit", "rate_hz.flac", lambda f: f[0], True,
     "48001 Hz: 0b1101 + rate in Hz as a 16-bit number"),
    ("rate-hz10-16bit", "rate_hz10.flac", lambda f: f[0], True,
     "10010 Hz: 0b1110 + rate/10 as a 16-bit number (units are per-code, and "
     "never 24-bit -- the scaffold's '8 | 16 | 24' note had no witness)"),
    ("stereo-leftside", "l4_stereo.flac",
     lambda f: next(h for h in f if h["decorrelation"] == "left-side"), False,
     "left/side decorrelation: channels code 0b1000"),
    ("stereo-sideright", "l4_stereo.flac",
     lambda f: next(h for h in f if h["decorrelation"] == "side-right"), False,
     "side/right decorrelation: channels code 0b1001"),
    ("verbatim-first", "verbatim.flac", lambda f: f[0], True,
     "VERBATIM subframes from an incompressible source (Knuth hash PCM) at "
     "`-l 0`: closes the golden-vector gap step 3e's plan pinned. Required: "
     "the fail-closed census in emit_body_streams refuses to write the table "
     "if any frame of this stream stops being verbatim"),
]


def load_streams(src):
    streams = {}
    for _, filename, _, _, _ in VECTOR_SPECS:
        if filename not in streams:
            path = os.path.join(src, filename)
            data = open(path, "rb").read()
            streams[filename] = (data, streaminfo(path), find_frames(data))
    return streams


def check_stream(filename, frames, info):
    """Structural invariants. A parser that misreads field widths fails here,
    not silently inside the emitted table."""
    expected = -(-info["total samples"] // info["maximum blocksize"])
    assert len(frames) == expected, (
        "%s: found %d frames, STREAMINFO implies %d"
        % (filename, len(frames), expected))
    assert all(h["crc_ok"] for h in frames), "%s: header CRC-8 mismatch" % filename
    numbers = [h["number"] for h in frames]
    assert numbers == list(range(len(frames))), (
        "%s: coded frame numbers are not 0..N-1 in stream order" % filename)
    offsets = [h["offset"] for h in frames]
    assert offsets == sorted(set(offsets)), "%s: offsets not ascending" % filename
    # The final frame is allowed to be short; every other frame is full.
    blocks = [h["blocksize"] for h in frames]
    assert all(b == info["maximum blocksize"] for b in blocks[:-1]), (
        "%s: a non-final frame departs from STREAMINFO max blocksize" % filename)
    return expected


def emit(src, out_path):
    streams = load_streams(src)
    # Fail-closed VERBATIM census (step 3e): the required `verbatim-first`
    # header vector only witnesses verbatim while the encoder still chooses
    # it. Measured on libFLAC 1.5.0: hash PCM at `-l 0` -> every frame
    # VERBATIM with wasted 0 (the same source at `-l 12`: zero verbatim --
    # `-l 0` is the switch). If an upgrade flips that, refuse to write.
    # subframe0_wasted is derived by the header parser for subframe 0, and
    # verbatim.flac is mono, so this covers every subframe in the stream.
    _, _, verbatim_frames = streams["verbatim.flac"]
    assert verbatim_frames and all(
        h["subframe0"] == "verbatim" and h["subframe0_wasted"] == 0
        for h in verbatim_frames), \
        "verbatim.flac: not every frame is VERBATIM with wasted 0 -- the " \
        "required verbatim-first vector would silently stop witnessing " \
        "verbatim. Re-measure before regenerating."
    version = subprocess.run(["flac", "--version"], capture_output=True,
                             text=True).stdout.strip().splitlines()[0]
    lines = []

    def w(text=""):
        lines.append(text)

    w("# flac-lite frame-header golden vectors -- GENERATED")
    w("# DO NOT EDIT BY HAND.  Regenerate with:  scripts/gen_frame_vectors.sh")
    w("# Generator + expected-value derivation: scripts/frame_vectors.py")
    w("# Source streams: scripts/frame_vectors.py synth (integer-exact, no RNG)")
    w("# Encoder: %s" % version)
    w("#")
    w("# These are REAL frame headers captured from libFLAC, with expected field")
    w("# values derived by an independent RFC 9639 9.1 bit parser -- never by")
    w("# asking libFLAC, never hand-packed. Hand-packing is how the 31-vs-32-bit")
    w("# fixed-field error entered the scaffold AND how a unit test came to assert")
    w("# it as truth (FLAC.md 'Frame header: measured byte layout').")
    w("#")
    w("# Consumed by crates/flac-lite/tests/frame_header_layout.rs, which feeds")
    w("# `bytes` through bits::BitReader and asserts every field width, cursor")
    w("# position, the coded number, and the CRC-8 byte. Phase 1 step 2 replaces")
    w("# that test's hand-written expectations with FrameHeader::parse over the")
    w("# same table, so the vectors outlive the scratch test.")
    w("#")
    w("# Two facts every vector here proves, and neither was true in the scaffold:")
    w("#   * fixed fields are 32 bits (sync 14 + reserved 1 + blocking 1 +")
    w("#     blocksize 4 + rate 4 + CHANNELS 4 + size 3 + reserved 1)")
    w("#   * the coded number is whole octets, so the CRC-8 ALWAYS lands on a byte")
    w("#     boundary -> byte_align() before the CRC-8 is a no-op there anyway, and")
    w("#     writing it 'because the header CRC is unaligned' encodes a lie.")
    w("")

    for label, filename, selector, required, why in VECTOR_SPECS:
        _, info, frames = streams[filename]
        check_stream(filename, frames, info)
        try:
            header = selector(frames)
        except StopIteration:
            if required:
                raise
            continue
        w("vector %s" % label)
        w("source_stream %s" % filename)
        w("note %s" % why)
        w("required %s" % ("true" if required else "false"))
        w("bytes %s" % " ".join(format(b, "02X") for b in header["header"]))
        w("frame_offset %d" % header["offset"])
        w("stream_frame_count %d" % len(frames))
        w("header_bits %d" % header["header_bits"])
        w("fixed_fields_bits 32")
        w("sync 0x3FFE")
        w("blocksize_code 0x%X" % header["blocksize_code"])
        w("blocksize %d" % header["blocksize"])
        w("samplerate_code 0x%X" % header["rate_code"])
        w("samplerate_hz %s" % (header["rate_hz"]
                                if isinstance(header["rate_hz"], int)
                                else "from-stream"))
        w("stream_samplerate_hz %d" % info["sample_rate"])
        w("channels_code 0x%X" % header["chan_code"])
        w("subframes %d" % header["subframes"])
        w("decorrelation %s" % header["decorrelation"])
        w("samplesize_code 0x%X" % header["size_code"])
        w("bits_per_sample %s" % (header["bps"] or "from-stream"))
        w("stream_bits_per_sample %d" % info["bits-per-sample"])
        w("coded_number %d" % header["number"])
        w("coded_number_octets %d" % header["octets"])
        w("extra_field_bytes %d" % header["extra_bytes"])
        w("crc8_byte 0x%X" % header["crc_byte"])
        w("crc8_bit_position %d" % header["crc_bit"])
        w("crc8_byte_aligned %s" % ("true" if header["crc_bit"] % 8 == 0 else "false"))
        w("subframe0_type %s" % header["subframe0"])
        w("subframe0_kind %s" % header["subframe0_kind"])
        w("subframe0_order %s" % header["subframe0_order"])
        w("subframe0_wasted %s" % header["subframe0_wasted"])
        w("subframe0_bytes %s" % " ".join(
            format(b, "02X") for b in header["subframe0_bytes"]))
        w("")

    w("# --- measured encoder behaviour (cite these instead of guessing) ---------")
    w("# First-subframe type across a whole 10s encode of the same source:")
    for filename in ("l0_stereo.flac", "l4_stereo.flac"):
        _, info, frames = streams[filename]
        tally = {}
        for header in frames:
            tally[header["subframe0"]] = tally.get(header["subframe0"], 0) + 1
        w("#   %-16s %s" % (filename,
                            ", ".join("%s x%d" % kv for kv in sorted(tally.items()))))
        dec = {}
        for header in frames:
            dec[header["decorrelation"]] = dec.get(header["decorrelation"], 0) + 1
        w("#   %-16s decorrelation chosen: %s"
          % ("", ", ".join("%s x%d" % kv for kv in sorted(dec.items()))))
    w("# => `-l N` is MAX LPC ORDER, not a FIXED-predictor cap (flac --help:")
    w("#    '-l, --max-lpc-order=#   Max LPC order; 0 => only fixed predictors').")
    w("#    `-l 0` is FIXED-only; `-l 4` emits real LPC subframes of order <= 4.")
    w("")

    with open(out_path, "w") as handle:
        handle.write("\n".join(lines))
    written = sum(1 for line in lines if line.startswith("vector "))
    print("   wrote %s (%d vectors, all stream invariants green)"
          % (out_path, written))


STREAMS = ("l4_stereo.flac", "l0_stereo.flac", "l4_mono.flac",
           "l4_silence.flac", "r65k.flac", "tail16.flac", "tail8.flac",
           "rate_khz.flac", "rate_hz.flac", "rate_hz10.flac")


# ------------------------------------------------ subframe-body walking (3e)

class Bits:
    """MSB-first bit cursor — the independent reader below the frame header.
    Deliberately naive (bit loop, no accumulator): a second mechanism from
    crates/flac-lite's `peek_at`, so an accumulator bug cannot agree with a
    cursor bug."""

    def __init__(self, data, pos_bits=0):
        self.data = data
        self.pos = pos_bits

    def read(self, n):
        assert 0 < n and self.pos + n <= len(self.data) * 8, \
            "EOF reading %d bits at %d" % (n, self.pos)
        v = 0
        for _ in range(n):
            byte = self.data[self.pos >> 3]
            v = (v << 1) | ((byte >> (7 - (self.pos & 7))) & 1)
            self.pos += 1
        return v

    def read_signed(self, n):
        v = self.read(n)
        return v - (1 << n) if v >= (1 << (n - 1)) else v

    def skip(self, n):
        assert self.pos + n <= len(self.data) * 8, \
            "EOF skipping %d bits at %d" % (n, self.pos)
        self.pos += n


def walk_subframe(b, blocksize, bps):
    """Walk one mono subframe from its first bit, reading every field the
    spec says is there, in field order. Returns a dict with the per-subframe
    ground truth `decode_subframe` is tested against.

    Field order below the type code is §9.2.2/§9.2.5/§9.2.6 Table 22, all
    measured on libFLAC 1.5.0 bytes by the draft probe (`flac3e_probe.py`,
    drafts/) and re-proven here on every generation: wasted run -> warm-up
    (order x (bps - wasted), stream order oldest-first) -> LPC u(4)
    precision-minus-1 (0b1111 forbidden) -> s(5) shift (MUST NOT be
    negative) -> order x s(precision) coefficients -> residual.

    The walk's *exit position* is load-bearing: the caller checks every
    mono frame's exit + footer gap lands exactly on the next frame offset
    (or end of stream), so a mis-sized field anywhere -- precision read as
    5 bits, warm-up skipped, a phantom residual header -- breaks the
    arithmetic on every frame instead of passing quietly.
    """
    assert b.read(1) == 0, "subframe pad bit set"
    code = b.read(6)
    if code == 0:
        kind, order = "constant", 0
    elif code == 1:
        kind, order = "verbatim", 0
    elif 8 <= code <= 12:
        kind, order = "fixed", code - 8
    elif code >= 32:
        kind, order = "lpc", code - 31
    else:
        raise AssertionError("reserved subframe code 0b%06b" % code)
    wasted = 0
    if b.read(1):
        wasted = 1
        while b.read(1) == 0:
            wasted += 1
    sub_bps = bps - wasted
    assert sub_bps > 0, "§9.2.2 violated: bps=%d wasted=%d" % (bps, wasted)
    warm = [b.read_signed(sub_bps) for _ in range(order)]  # stream order
    precision = shift = coeffs = body = None
    if kind == "constant":
        body = b.read_signed(sub_bps)
    elif kind == "verbatim":
        body = [b.read_signed(sub_bps) for _ in range(blocksize)]
    else:
        if kind == "lpc":
            precision = b.read(4)
            assert precision != 0b1111, "forbidden precision code"
            shift = b.read_signed(5)
            assert shift >= 0, "§9.2.6: shift MUST NOT be negative (%d)" % shift
            coeffs = [b.read_signed(precision + 1) for _ in range(order)]
        method = b.read(2)
        assert method in (0, 1), "reserved residual method %d" % method
        plen, escape = (4, 15) if method == 0 else (5, 31)
        porder = b.read(4)
        partitions = 1 << porder
        part = blocksize >> porder
        seen = 0
        for p in range(partitions):
            count = part - (order if p == 0 else 0)
            param = b.read(plen)
            if param == escape:
                raw = b.read(5)
                b.skip(count * raw)
            else:
                for _ in range(count):
                    q = 0
                    while b.read(1) == 0:
                        q += 1
                        assert q < 600, "runaway unary"
                    b.skip(param)
            seen += count
        assert seen == blocksize - order, \
            "residual sample count %d != %d" % (seen, blocksize - order)
    return dict(kind=kind, order=order, wasted=wasted, sub_bps=sub_bps,
                warm=warm, precision=precision, shift=shift, coeffs=coeffs,
                body=body, exit_bits=b.pos)


def reference_pcm(path):
    """Ground-truth PCM from the reference DECODER (libFLAC `flac -d`).
    Independent of this harness's parser and of crates/flac-lite: two
    decoders meeting on the encoder's bytes is the strongest available
    witness for subframe reconstruction (the oracle witnesses layout; the
    reference decoder witnesses the math's outcome)."""
    wav = path + ".pcm.wav"
    subprocess.run(["flac", "-d", "-f", "-s", "-o", wav, path], check=True)
    with wave.open(wav) as w:
        assert w.getsampwidth() == 2, "expected 16-bit reference decode"
        n = w.getnframes() * w.getnchannels()
        data = w.readframes(w.getnframes())
    os.remove(wav)
    return list(struct.unpack("<%dh" % n, data))


def walk_stream_bodies(filename, path):
    """Walk every frame of a MONO stream; return (frames, walks, info, pcm).

    Stream invariants, all fail-closed:
      * one subframe per frame (mono)
      * exit + footer gap == next frame's offset (or end of stream).
        MEASURED footer rule (probe, libFLAC 1.5.0 -- it corrected the
        draft's '8 pad bits + CRC-16 = 32 bits always' belief): the frame
        pads to the byte boundary and writes ONLY the 16-bit CRC, so an
        aligned subframe exit leaves a 16-bit gap, an unaligned one
        (8 - exit%8) + 16.
      * warm-up samples == the frame's own first reference-PCM samples >>
        wasted (the 3a finding, restated through the body walk)
      * verbatim bodies == reference PCM exactly; constant bodies == one
        value << wasted everywhere
    """
    data = open(path, "rb").read()
    info = streaminfo(path)
    assert info["channels"] == 1, "%s: body walk is mono-only" % filename
    bps = info["bits-per-sample"]
    frames = find_frames(data)
    check_stream(filename, frames, info)
    pcm = reference_pcm(path)
    walks = []
    for i, h in enumerate(frames):
        b = Bits(data, h["offset"] * 8 + h["header_bits"])
        w = walk_subframe(b, h["blocksize"], bps)
        next_bit = (frames[i + 1]["offset"] * 8 if i + 1 < len(frames)
                    else len(data) * 8)
        gap = next_bit - w["exit_bits"]
        expect_gap = (16 if w["exit_bits"] % 8 == 0
                      else (8 - w["exit_bits"] % 8) + 16)
        assert gap == expect_gap, \
            "%s frame %d: footer gap %d != %d (exit %% 8 = %d) -- a body "\
            "field is mis-sized" % (filename, i, gap, expect_gap,
                                    w["exit_bits"] % 8)
        start = h["number"] * info["maximum blocksize"]
        for j in range(w["order"]):
            assert w["warm"][j] == pcm[start + j] >> w["wasted"], \
                "%s frame %d: warm-up[%d] %d != own PCM>>wasted %d" \
                % (filename, i, j, w["warm"][j], pcm[start + j] >> w["wasted"])
        if w["kind"] == "verbatim":
            assert w["body"] == list(pcm[start:start + h["blocksize"]]), \
                "%s frame %d: verbatim body != reference PCM" % (filename, i)
        if w["kind"] == "constant":
            assert all(s == (w["body"] << w["wasted"])
                       for s in pcm[start:start + h["blocksize"]]), \
                "%s frame %d: constant body != reference PCM" % (filename, i)
        w["frame"] = h
        walks.append(w)
    return frames, walks, info, pcm


# (label, stream, expected kind, required?, why)
#
# One 256-sample mono stream per subframe type: blocksize 256 -> exactly one
# frame per stream, so the stream census IS the frame's type. The expected
# kind is a fail-closed gate: if a libFLAC upgrade stops choosing it, the
# generator refuses to write the table rather than emitting a vector that
# silently tests something else.
BODY_SPECS = [
    ("verbatim-256", "verbatim256.flac", "verbatim", True,
     "VERBATIM body from incompressible hash PCM at `-l 0`: exit cursor at "
     "exactly start + blocksize x subframe bps, samples == reference PCM"),
    ("constant-256", "constant256.flac", "constant", True,
     "CONSTANT body from digital silence: one s(subframe bps) value, exit "
     "right after it"),
    ("fixed1-wasted5-256", "square256.flac", "fixed", True,
     "FIXED-1 + wasted 5 (coarse square wave, zero LSBs by construction): "
     "wasted run feeding subframe bps, warm-up, and the << wasted store"),
    ("lpc-256", "tonal256.flac", "lpc", True,
     "LPC body from a `-l 4` encode (order is per-frame, measured order 3 "
     "here -- `-l` is a cap, never a mandate): u(4) precision-1, s(5) shift, "
     "coefficients, residual -- the Table 22 field order end to end"),
    ("lpc-wasted-256", "lpcw256.flac", "lpc", True,
     "LPC + wasted bits (tonal quantized to multiples of 32; measured LPC-3 "
     "+ wasted 5): the scale discriminator -- prediction runs at the coded "
     "(stripped) scale and the whole decoded block is multiplied by 2^wasted "
     "last (RFC 9639 9.2.2, libFLAC stream_decoder.c:3028). Padding before "
     "the LPC recurrence mismatches this vector at 252/256 samples"),
    ("fixed-lottery-256", "smooth256.flac", "fixed", False,
     "smooth triangle: whichever FIXED order libFLAC picks (emitted only "
     "if the census is uniformly FIXED; order 2 measured)")
]


def emit_body_streams(src):
    """Walk + census body streams; return {(label): (frames, walks, info,
    pcm)} for BODY_SPECS, and fail closed on every expected-kind census.

    (The header table's required `verbatim-first` vector is gated separately,
    in emit() -- its stream is the 2048-block multi-frame one.)"""
    out = {}
    for label, name, expected, required, _why in BODY_SPECS:
        path = os.path.join(src, name)
        frames, walks, info, pcm = walk_stream_bodies(name, path)
        assert len(frames) == 1, \
            "%s: expected exactly one frame (256 samples @ blocksize 256), "\
            "got %d" % (name, len(frames))
        kind = walks[0]["kind"]
        if kind != expected:
            if required:
                raise AssertionError(
                    "%s: encoder chose %s, expected %s -- refusing to emit a "
                    "vector that tests something else" % (name, kind, expected))
            print("   note: %s: encoder chose %s (expected %s) -- vector "
                  "skipped" % (name, kind, expected))
            continue
        out[label] = (frames, walks, info, pcm)
    return out


def emit_bodies(src, out_path):
    emitted = emit_body_streams(src)
    version = subprocess.run(["flac", "--version"], capture_output=True,
                             text=True).stdout.strip().splitlines()[0]
    lines = []

    def w(text=""):
        lines.append(text)

    w("# flac-lite subframe-BODY golden vectors -- GENERATED")
    w("# DO NOT EDIT BY HAND.  Regenerate with:  scripts/gen_frame_vectors.sh")
    w("# Generator + independent walk: scripts/frame_vectors.py (emit_bodies)")
    w("# Source streams: scripts/frame_vectors.py synth (integer-exact, no RNG)")
    w("# Encoder: %s" % version)
    w("#")
    w("# Ground truth for step 3e's `decode_subframe`, in two independent")
    w("# layers: `frame_hex` is raw libFLAC bytes; the field values come from")
    w("# the independent Bits walk (a naive bit-loop reader -- a second")
    w("# mechanism from crates/flac-lite's accumulator); `pcm_samples` come")
    w("# from the reference DECODER (flac -d), so reconstruction is witnessed")
    w("# by a second decoder, not by the harness re-deriving the same sums.")
    w("# Every stream additionally passed the footer-gap, warm-up == own PCM,")
    w("# and body == PCM invariants at generation time (walk_stream_bodies),")
    w("# and the per-stream kind census is fail-closed: a libFLAC upgrade that")
    w("# changes any expected kind refuses to write this file.")
    w("#")
    w("# exit_bits is relative to the START of frame_hex (the frame's first")
    w("# byte), and is where the reader must land after the subframe body --")
    w("# before the frame footer (pad-to-byte + CRC-16), which is 3f's.")
    w("")

    for label, name, _expected, _required, why in BODY_SPECS:
        if label not in emitted:
            continue
        frames, walks, info, pcm = emitted[label]
        h, wk = frames[0], walks[0]
        data = open(os.path.join(src, name), "rb").read()
        frame_start = h["offset"]
        frame_end = (frames[1]["offset"] if len(frames) > 1 else len(data))
        frame = data[frame_start:frame_end]
        w("body_vector %s" % label)
        w("source_stream %s" % name)
        w("note %s" % why)
        w("frame_hex %s" % " ".join(format(x, "02X") for x in frame))
        w("stream_samplerate_hz %d" % info["sample_rate"])
        w("stream_bits_per_sample %d" % info["bits-per-sample"])
        w("blocksize %d" % h["blocksize"])
        w("subframe_kind %s" % wk["kind"])
        w("subframe_order %d" % wk["order"])
        w("subframe_wasted %d" % wk["wasted"])
        w("subframe_bits %d" % wk["sub_bps"])
        w("warm_up_stream_order %s" % " ".join(str(v) for v in wk["warm"]))
        if wk["precision"] is not None:
            w("lpc_precision_minus_one %d" % wk["precision"])
            w("lpc_shift %d" % wk["shift"])
            w("lpc_coeffs %s" % " ".join(str(c) for c in wk["coeffs"]))
        w("exit_bits %d" % (wk["exit_bits"] - frame_start * 8))
        w("pcm_samples %s" % " ".join(str(s) for s in pcm))
        w("")

    with open(out_path, "w") as handle:
        handle.write("\n".join(lines))
    n = sum(1 for line in lines if line.startswith("body_vector "))
    print("   wrote %s (%d body vectors, censuses + invariants green)"
          % (out_path, n))


# ------------------------------------------------ frame-run vectors (3f pt 2)

# Expected mode -> (channel code, subframe count, side slot, stored-slot
# formulas). side_slot is the subframe coded at bps + 1: slot 0 for
# side/right, slot 1 for left/side and mid/side (measured on encoder bytes +
# libFLAC read_subframe_'s bps++ on the side slot -- step 3f part 1; the
# anti-phase encode whose side warm-ups are values a bps-wide field cannot
# hold). stored[i] turns (L, R) into what slot i actually holds -- the
# ENCODER-side reading of the spec, independent of the decoder-side recovery
# the crate implements (the same two-readings discipline as the decorrelate
# oracle: a wrong shared formula fails the warm-up/body vs stored-pcm
# invariants here, at generation time, not just the crate's decode in the
# Rust suite). The `>> 1` for mid is Python's arithmetic shift -- floor on
# negatives, matching libFLAC's C >> and the crate's i64 >>; the stored-slot
# warm-up/body asserts below would catch a divergence from the encoder's
# actual stored bits on negative-odd (L+R).
RUN_MODES = {
    # NOTE the trailing comma on mono's stored tuple: without it the
    # parenthesised lambda is a bare function, not a 1-tuple (this exact
    # bug fired on the first emit_runs run).
    "mono":        {"code": 0b0000, "subframes": 1, "side_slot": None,
                    "stored": (lambda l, r: l,)},
    "independent": {"code": 0b0001, "subframes": 2, "side_slot": None,
                    "stored": (lambda l, r: l, lambda l, r: r)},
    "left-side":   {"code": 0b1000, "subframes": 2, "side_slot": 1,
                    "stored": (lambda l, r: l, lambda l, r: l - r)},
    "side-right":  {"code": 0b1001, "subframes": 2, "side_slot": 0,
                    "stored": (lambda l, r: l - r, lambda l, r: r)},
    "mid-side":    {"code": 0b1010, "subframes": 2, "side_slot": 1,
                    "stored": (lambda l, r: (l + r) >> 1, lambda l, r: l - r)},
}


def walk_stream_runs(filename, path, expected):
    """Walk every frame of a run stream (mono or stereo), slot by slot, and
    return (frames, per-frame slot walks, info, pcm_interleaved, stored_pcm).

    `expected` is the RUN_MODES label and a PARAMETER, never derived from
    the header: the harness names mono and independent-stereo both "none",
    so the channel code alone cannot tell them apart -- the expected label
    is the fail-closed source, same rule as BODY_SPECS' expected kinds.

    Fail-closed invariants beyond what walk_stream_bodies does for mono:
      * exactly 2 frames with blocksizes [256, 100] (RUN_SAMPLES geometry),
        the tail frame at the UNCOMMON 8-bit blocksize code (0b0110) -- the
        whole point of the 356-sample size
      * channel assignment UNIFORM across frames and equal to the expected
        mode's code, subframe count equal to STREAMINFO's channel count
        (a per-frame switch would make the table's single `decorrelation`
        label a lie)
      * the side slot walked at bps + 1, and its warm-up / verbatim body /
        constant value equal the stored-slot reference (side = L-R at a
        width a bps-wide field cannot hold, when the content says so)
      * per frame: last slot's exit + footer gap lands exactly on the next
        frame's offset (or end of stream) -- the measured footer rule
      * per slot: warm-up == own stored first samples >> wasted, verbatim
        body == stored block, constant body == one value << wasted
    """
    data = open(path, "rb").read()
    info = streaminfo(path)
    bps = info["bits-per-sample"]
    frames = find_frames(data)
    check_stream(filename, frames, info)
    assert [h["blocksize"] for h in frames] == [256, RUN_SAMPLES - 256], \
        "%s: run geometry wants blocksizes [256, %d] (RUN_SAMPLES %d @ " \
        "blocksize %d), got %s" % (filename, RUN_SAMPLES - 256, RUN_SAMPLES,
                                   info["maximum blocksize"],
                                   [h["blocksize"] for h in frames])
    assert frames[1]["blocksize_code"] == 0b0110, \
        "%s: tail frame blocksize code 0x%X, expected the uncommon 8-bit " \
        "0b0110 -- the tail form is why RUN_SAMPLES is 356" % (
            filename, frames[1]["blocksize_code"])
    mode = RUN_MODES[expected]
    assert all(h["chan_code"] == mode["code"] for h in frames), \
        "%s: channel assignment switched mid-stream or is not %s (%s) -- the " \
        "table's single decorrelation label would be false" % (
            filename, expected, [hex(h["chan_code"]) for h in frames])
    assert info["channels"] == mode["subframes"], \
        "%s: mode %s wants %d subframes, STREAMINFO says %d channels" % (
            filename, expected, mode["subframes"], info["channels"])
    pcm = reference_pcm(path)
    if info["channels"] == 2:
        interleaved = list(zip(pcm[0::2], pcm[1::2]))
        stored_pcm = [[mode["stored"][s](l, r) for (l, r) in interleaved]
                      for s in range(2)]
    else:
        interleaved = [(v,) for v in pcm]
        stored_pcm = [[mode["stored"][0](v, 0) for (v,) in interleaved]]
    total = sum(h["blocksize"] for h in frames)
    assert len(interleaved) == total, \
        "%s: reference PCM length %d != sum of frame blocksizes %d" % (
            filename, len(interleaved), total)
    walks = []
    pos_samples = 0
    for i, h in enumerate(frames):
        frame_walks = []
        b = Bits(data, h["offset"] * 8 + h["header_bits"])
        for s in range(mode["subframes"]):
            slot_bps = bps + (1 if mode["side_slot"] == s else 0)
            w = walk_subframe(b, h["blocksize"], slot_bps)
            start = pos_samples
            block = stored_pcm[s][start:start + h["blocksize"]]
            for j in range(w["order"]):
                assert w["warm"][j] == block[j] >> w["wasted"], \
                    "%s frame %d slot %d: warm-up %d != stored>>wasted %d" % (
                        filename, i, s, w["warm"][j], block[j] >> w["wasted"])
            if w["kind"] == "verbatim":
                assert w["body"] == block, \
                    "%s frame %d slot %d: verbatim body != stored PCM" % (
                        filename, i, s)
            if w["kind"] == "constant":
                assert all(v == (w["body"] << w["wasted"]) for v in block), \
                    "%s frame %d slot %d: constant body != stored PCM" % (
                        filename, i, s)
            frame_walks.append(w)
        next_bit = (frames[i + 1]["offset"] * 8 if i + 1 < len(frames)
                    else len(data) * 8)
        exit_bits = frame_walks[-1]["exit_bits"]
        gap = next_bit - exit_bits
        expect_gap = (16 if exit_bits % 8 == 0 else (8 - exit_bits % 8) + 16)
        assert gap == expect_gap, \
            "%s frame %d: footer gap %d != %d (exit %% 8 = %d) -- a body " \
            "field is mis-sized" % (filename, i, gap, expect_gap, exit_bits % 8)
        walks.append(frame_walks)
        pos_samples += h["blocksize"]
    return frames, walks, info, pcm, stored_pcm


# (label, stream, expected mode, why)
RUN_SPECS = [
    ("mono-2f", "run_mono.flac", "mono",
     "Mono run: single subframe per frame, tail frame at the uncommon 8-bit "
     "blocksize (100 samples) -- chaining + short-tail decode"),
    ("independent-2f", "run_indep.flac", "independent",
     "Independent stereo (0b0001) that COMPRESSES (two uncorrelated smooth "
     "channels): 2 subframes at frame bps each, no decorrelation; the arm "
     "the decorrelated baits cannot witness (their frames all pick side slots)"),
    ("leftside-2f", "run_leftside.flac", "left-side",
     "Left/side (0b1000) run: L primary at bps, side at bps+1; uniform mode "
     "census over both frames"),
    ("sideright-2f", "run_sideright.flac", "side-right",
     "Side/right (0b1001) run: SIDE primary at bps+1 (subframe 0 holds the "
     "side -- step 3f part 1's orientation), R at bps"),
    ("midside-2f", "run_midside.flac", "mid-side",
     "Mid/side (0b1010) run: mid primary at bps (parity LSB stolen into the "
     "side), side at bps+1; the mode libFLAC picks on correlated material"),
]


def emit_runs(src, out_path):
    """Emit the frame-run golden table for step 3f part 2. What the table
    COMMITS is what the Rust side needs to drive and diff a frame decode:
    contiguous per-frame raw bytes, per-frame blocksize, the stream facts,
    the side-slot seam label, and both ground-truth layers -- `slot_stored_N`
    (what each SUBFRAME holds, encoder-side formulas over the reference PCM)
    and `pcm_left`/`pcm_right` (true L/R from `flac -d`). The per-slot field
    ground truth (warm-up, kinds, exits) is asserted at GENERATION time
    (walk_stream_runs); it stays in the harness, keeping the file small."""
    version = subprocess.run(["flac", "--version"], capture_output=True,
                             text=True).stdout.strip().splitlines()[0]
    lines = []

    def w(text=""):
        lines.append(text)

    w("# flac-lite frame-RUN golden vectors -- GENERATED")
    w("# DO NOT EDIT BY HAND.  Regenerate with:  scripts/gen_frame_vectors.sh")
    w("# Generator + independent walk: scripts/frame_vectors.py (emit_runs)")
    w("# Source streams: scripts/frame_vectors.py synth (integer-exact, no RNG)")
    w("# Encoder: %s" % version)
    w("#")
    w("# Ground truth for step 3f part 2's `decode_frame`: whole 2-frame runs")
    w("# (256 + 100 samples @ -b 256; the 100 tail forces the uncommon 8-bit")
    w("# blocksize form). `frame_hex` rows are raw libFLAC frames IN STREAM")
    w("# ORDER, each covering its frame THROUGH the footer (padding +")
    w("# CRC-16) to the next frame's first byte -- concatenating the rows in")
    w("# order reproduces the contiguous frame region byte-exactly, so a")
    w("# single BitReader can chain frame N -> frame N+1 exactly as a real")
    w("# decode loop does; `frame_blocksize` rows pair with them by position.")
    w("# `side_slot` names the decorrelated slot coded at bps + 1 (none/0/1)")
    w("# -- step 3f part 1's measured seam, table-driven so the Rust side")
    w("# reads it, never re-derives it. `side_width_sensitive` says whether")
    w("# that seam is OBSERVABLE on this run's side slot (none/true/false),")
    w("# computed from the walk's own kind/order: FIXED-0 subframes never")
    w("# consult frame bps (no warm-up reads, width-independent Rice codewords,")
    w("# pad by the wasted field), so left/side and side/right -- whose tiny")
    w("# quantized-noise sides measure as FIXED-0 wasted 4 -- carry an inert")
    w("# seam (false); mid/side's side is LPC-3 on frame 0, so its warm-up")
    w("# reads make the seam load-bearing (true). The Rust suite's negative")
    w("# control demands divergence exactly where the flag says true, and")
    w("# demands width-INvariance where it says false -- the flag is checked,")
    w("# not trusted.")
    w("# Two ground-truth layers, both whole-stream: `slot_stored_N` is slot")
    w("# N's DECODED subframe samples (on decorrelated runs the side slot")
    w("# holds L-R, the mid slot (L+R)>>1) -- derived from pcm_left/pcm_right")
    w("# by the encoder-side formulas and re-checked at generation time")
    w("# against each subframe's own warm-up/verbatim/constant fields by the")
    w("# independent Bits walk (an encoder-side reading of the spec,")
    w("# independent of the crate's decoder-side recovery). pcm_left/")
    w("# pcm_right are the reference DECODER (flac -d) whole-stream PCM")
    w("# de-interleaved -- always the TRUE L/R, even where the subframe")
    w("# holds a stored value; a full frame decode (decorrelate included)")
    w("# must match them on every vector.")
    w("# Generation-time invariants (fail-closed, walk_stream_runs): blocksizes")
    w("# [256, 100] with the tail at the uncommon 8-bit code; uniform channel")
    w("# census == the label; side slot walked at bps+1; per-frame footer gap")
    w("# landing exactly on the next frame offset; warm-up == stored >>")
    w("# wasted; verbatim body == stored block; constant body == one value")
    w("# << wasted. A libFLAC upgrade that changes any mode choice refuses to")
    w("# write this file.")
    w("")
    for label, name, expected, why in RUN_SPECS:
        path = os.path.join(src, name)
        frames, walks, info, pcm, stored_pcm = walk_stream_runs(name, path,
                                                                expected)
        mode = RUN_MODES[expected]
        data = open(path, "rb").read()
        w("run_vector %s" % label)
        w("source_stream %s" % name)
        w("note %s" % why)
        w("stream_samplerate_hz %d" % info["sample_rate"])
        w("stream_bits_per_sample %d" % info["bits-per-sample"])
        w("channels %d" % info["channels"])
        w("decorrelation %s" % expected)
        w("side_slot %s" % ("none" if mode["side_slot"] is None
                            else mode["side_slot"]))
        # Is the bps+1 seam OBSERVABLE on this run's side slot? Measured
        # (libFLAC 1.5.0, the run streams): left/side and side/right encode
        # their side as FIXED order 0 (tiny quantized noise is
        # uncompressible -- no predictor helps), and a FIXED-0 subframe
        # never consults frame bps: no warm-up reads (order x bits = 0),
        # Rice codewords are width-independent, pad_block shifts by the
        # wasted field. Reading the side at bps+0 then decodes bit-identical
        # -- the seam is un-witnessable there, by grammar, not by table bug
        # (discovered by the Rust suite's negative control: leftside at
        # bps+0 still matched ground truth). mid/side's side carries an
        # LPC-3 warm-up, so the seam is observable on that run. Computed
        # from the walk's own kind/order -- never hand-labeled.
        side = mode["side_slot"]
        if side is None:
            sens = "none"
        else:
            sens = "true" if any(
                fw[side]["order"] > 0
                or fw[side]["kind"] in ("verbatim", "constant")
                for fw in walks) else "false"
        w("side_width_sensitive %s" % sens)
        w("frame_count %d" % len(frames))
        for h in frames:
            end = frames[h["number"] + 1]["offset"] if h["number"] + 1 < len(frames) \
                else len(data)
            frame = data[h["offset"]:end]
            w("frame_blocksize %d" % h["blocksize"])
            w("frame_hex %s" % " ".join(format(x, "02X") for x in frame))
        for s, block in enumerate(stored_pcm):
            w("slot_stored_%d %s" % (s, " ".join(str(v) for v in block)))
        if info["channels"] == 2:
            left = pcm[0::2]
            right = pcm[1::2]
        else:
            left, right = pcm, []
        w("pcm_left %s" % " ".join(str(v) for v in left))
        if right:
            w("pcm_right %s" % " ".join(str(v) for v in right))
        w("")

    with open(out_path, "w") as handle:
        handle.write("\n".join(lines))
    n = sum(1 for line in lines if line.startswith("run_vector "))
    print("   wrote %s (%d run vectors, uniform-mode censuses + stored-slot "
          "invariants + footer gaps green)" % (out_path, n))


def measure(src):
    """Print the candidate-filtering measurement quoted by FLAC.md and the
    generated vector header. Numbers here are the only sync-scan figures this
    repo cites; everything else is folklore from a different source stream.

    Per stream, four counts against the frame count implied by STREAMINFO:
      sync     -- bytes shaped like a frame start (0xFF, next & 0xF8 == 0xF8)
      lenient  -- sync + coded-number shape + CRC-8, NO field validation
      strict   -- sync + strict RFC 9639 field validation + CRC-8 (what
                  find_frames uses)
    and whether each set's coded frame numbers are exactly 0..N-1 in order,
    which is the property that actually matters (a matching count with a wrong
    membership is a silent corruption, not a near miss).
    """
    print("stream             expect    sync  crc-only  strict   "
          "strict numbers")
    totals = [0, 0, 0, 0]
    for name in STREAMS:
        path = os.path.join(src, name)
        if not os.path.exists(path):
            print("%s MISSING (run gen_frame_vectors.sh)" % name)
            continue
        data = open(path, "rb").read()
        info = streaminfo(path)
        start = first_frame_offset(data)
        expected = -(-info["total samples"] // info["maximum blocksize"])
        sync = crc_only = 0
        strict_headers = []
        for i in range(start, len(data) - 9):
            if data[i] != 0xFF or (data[i + 1] & 0xF8) != 0xF8:
                continue
            sync += 1
            if parse_lenient(data, i) is not None:
                crc_only += 1
            header = parse_frame_header(data, i)
            if header and header["crc_ok"]:
                strict_headers.append(header)
        numbers = [h["number"] for h in strict_headers]
        strict = len(strict_headers)
        ok_strict = numbers == list(range(expected))
        false_pos = crc_only - strict
        print("%-17s %7d %8d %9d %8d   %s%s"
              % (name, expected, sync, crc_only, strict,
                 "exact 0..N-1" if ok_strict else "WRONG",
                 "" if false_pos == 0 else
                 "   [crc-only admits %d non-frame%s]"
                 % (false_pos, "" if false_pos == 1 else "s")))
        totals[0] += expected
        totals[1] += sync
        totals[2] += crc_only
        totals[3] += strict
        assert ok_strict, "%s: strict filter is not exact -- harness is wrong" % name
        assert strict == expected, "%s: strict filter count mismatch" % name
    print("%-17s %7d %8d %9d %8d" %
          ("TOTAL", totals[0], totals[1], totals[2], totals[3]))


# ------------------------------------------------ perf-gate spike assets (step 4)

def fnv1a64(data):
    """FNV-1a 64-bit over bytes. Mirrored in Rust in the spike crate (the host
    witness recomputes it, PR 2's ROM recomputes it on hardware): two
    independent implementations of one simple hash, meeting on committed
    bytes. Python's unbounded ints are masked to 64 bits per step, which is
    exactly Rust's wrapping arithmetic."""
    h = 0xCBF43CE95DE684B7
    for byte in data:
        h ^= byte
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def spike_census(filename, frames, data, info):
    """Walk EVERY subframe slot of EVERY frame of a stereo spike stream;
    return a census string like 'slot 0: fixed0 x157 | slot 1: fixed0 x157'.

    Fail-closed on the two measured facts the perf gate's comparison stands on
    (FLAC.md Correction 3): `-l 0` is FIXED-only, `-l 4` is majority real LPC.
    If a libFLAC upgrade ever breaks an arm, the two arms stop being a
    comparison — the generator must refuse, loudly, not emit a silently
    degenerate gate.

    The walk doubles as the region-slicing witness: every frame's subframe
    exit + the measured footer gap (pad-to-byte, then the 16-bit CRC) must
    land exactly on the next frame's offset (EOF for the last frame). A
    mis-sized field anywhere breaks that arithmetic on every frame.

    Side-slot width follows the *measured* seam (FLAC.md step 3f part 1): a
    decorrelated frame codes its side subframe at bps + 1.
    """
    tally = []
    for i, header in enumerate(frames):
        bps = header["bps"] or info["bits-per-sample"]
        assert header["subframes"] == 2, \
            "%s frame %d: expected stereo, got %d subframes" % (
                filename, i, header["subframes"])
        bits = Bits(data, header["offset"] * 8 + header["header_bits"])
        # Side slot per the measured orientation (FLAC.md step 3f): left-side
        # stores the side at slot 1, side-right at slot 0, mid-side at slot 1
        # — the same mapping crate-internal decode_frame uses.
        side_slot = {0b1000: 1, 0b1001: 0, 0b1010: 1}.get(header["chan_code"])
        kinds = []
        for slot in range(2):
            slot_bps = bps + 1 if slot == side_slot else bps
            walk = walk_subframe(bits, header["blocksize"], slot_bps)
            kinds.append("%s%d" % (walk["kind"], walk["order"]))
        tally.append(kinds)
        exit_bit = bits.pos
        padded = exit_bit + ((8 - exit_bit % 8) % 8)
        end_bit = (frames[i + 1]["offset"] * 8 if i + 1 < len(frames)
                   else len(data) * 8)
        assert padded + 16 == end_bit, \
            ("%s frame %d: subframe exit %d + footer gap does not land on the "
             "next frame (expected end bit %d) — census walk or region "
             "slicing is wrong" % (filename, i, exit_bit, end_bit))
    per_slot = []
    for slot in range(2):
        slot_tally = {}
        for kinds in tally:
            slot_tally[kinds[slot]] = slot_tally.get(kinds[slot], 0) + 1
        per_slot.append(", ".join("%s x%d" % kv
                                  for kv in sorted(slot_tally.items())))
    census = "slot 0: %s | slot 1: %s" % (per_slot[0], per_slot[1])

    # Arm censuses, fail closed (measured libFLAC 1.5.0 facts).
    flat = [k for kinds in tally for k in kinds]
    if filename == "l0_stereo.flac":
        assert all(k.startswith("fixed") for k in flat), \
            "%s: the -l 0 arm is no longer FIXED-only (%s) — the gate's FIXED "\
            "arm would silently measure something else. Re-measure first." % (
                filename, census)
    elif filename == "l4_stereo.flac":
        assert all(k.startswith(("fixed", "lpc")) for k in flat), \
            "%s: the -l 4 arm carries kinds outside fixed/lpc (%s)" % (
                filename, census)
        lpc = sum(1 for k in flat if k.startswith("lpc"))
        assert lpc * 2 >= len(flat), \
            "%s: the -l 4 arm is no longer majority LPC (%d/%d, %s) — the "\
            "FIXED-vs-LPC comparison would measure one arm twice. "\
            "Re-measure first." % (filename, lpc, len(flat), census)
    return census


def spike_assets(src, crate_dir):
    """Stage the perf gate spike's embedded clips (FLAC.md, "Perf gate spike —
    concrete plan", PR 1). Writes under the spike crate:

      assets/spike_l0_frames.bin    raw frame region, the -l 0 encode
      assets/spike_l0_pcm.bin       reference PCM (flac -d), 16-bit LE interleaved
      assets/spike_l4_frames.bin    raw frame region, the -l 4 encode
      assets/spike_l4_pcm.bin       reference PCM likewise
      src/assets.rs                 generated manifest: offset table, sizes,
                                    FNV-1a pins, censuses, include_bytes! pins

    Offsets are RELATIVE TO THE REGION (the region alone is what gets
    embedded: no fLaC magic, no STREAMINFO). This is the plan's hand-computed
    offset array — the spike's deliberate stand-in for the GAFP manifest's
    seek table. Everything derives from the strict frame finder whose
    exactness check_stream proves per stream; nothing is hand-packed.

    The PCM .bin files are host-test ground truth only; src/assets.rs
    embeds the frame regions and pins the PCM by hash, so the ROM image
    never carries a byte of reference data."""
    import hashlib
    import json
    version = subprocess.run(["flac", "--version"], capture_output=True,
                             text=True).stdout.strip().splitlines()[0]

    def qstr(text):
        # Rust-safe double-quoted string literal. ensure_ascii=False is
        # load-bearing: json's default escapes non-ASCII to \\uXXXX, which is
        # valid JSON but NOT valid Rust (Rust wants \\u{XXXX}). With literal
        # UTF-8, the only escapes are \", \\, and control chars — where JSON
        # and Rust agree exactly. (Caught by the compiler on the em dash in
        # the LPC arm's `why` — a generated-file bug, found by the build, not
        # by review: the gates earning their keep.)
        return json.dumps(text, ensure_ascii=False)

    arms = (
        # (stream, const, blob prefix, why this arm exists)
        ("l0_stereo.flac", "L0_FIXED", "spike_l0",
         "FIXED-only arm (flac -1 -l 0 -b 2048 -m): the gate's conservative "
         "predictor cost"),
        ("l4_stereo.flac", "L4_LPC", "spike_l4",
         "LPC arm (flac -1 -l 4 -b 2048 -m): the gate's full-profile cost "
         "(majority LPC — FLAC.md Correction 3)"),
    )
    assets_dir = os.path.join(crate_dir, "assets")
    os.makedirs(assets_dir, exist_ok=True)
    clips = []
    for filename, const, prefix, why in arms:
        path = os.path.join(src, filename)
        with open(path, "rb") as handle:
            data = handle.read()
        info = streaminfo(path)
        frames = find_frames(data)
        check_stream(filename, frames, info)
        assert (info["channels"], info["bits-per-sample"], info["sample_rate"]) \
            == (2, 16, 32000), "%s: expected 16-bit stereo 32 kHz, got %s" % (
                filename, info)
        census = spike_census(filename, frames, data, info)
        start = first_frame_offset(data)
        assert frames[0]["offset"] == start, \
            "%s: first found frame is not the region start" % filename
        region = data[start:]
        offsets = [h["offset"] - start for h in frames]
        assert offsets[0] == 0, "%s: first frame not at region start" % filename
        assert offsets == sorted(set(offsets)), \
            "%s: frame offsets not strictly ascending" % filename
        ends = offsets[1:] + [len(region)]
        assert all(e > o for e, o in zip(ends, offsets)), \
            "%s: degenerate frame span" % filename
        assert sum(e - o for e, o in zip(ends, offsets)) == len(region), \
            "%s: frame table does not tile the region exactly" % filename
        assert sum(h["blocksize"] for h in frames) == info["total samples"], \
            "%s: per-frame blocksizes do not cover total samples" % filename
        pcm = reference_pcm(path)
        pcm_bytes = struct.pack("<%dh" % len(pcm), *pcm)
        assert len(pcm_bytes) == info["total samples"] * 2 * 2, \
            "%s: reference PCM size mismatch" % filename
        region_path = os.path.join(assets_dir, prefix + "_frames.bin")
        pcm_path = os.path.join(assets_dir, prefix + "_pcm.bin")
        with open(region_path, "wb") as handle:
            handle.write(region)
        with open(pcm_path, "wb") as handle:
            handle.write(pcm_bytes)
        clips.append(dict(
            filename=filename, const=const, prefix=prefix, why=why,
            region=region, region_path=region_path, frames=frames,
            offsets=offsets, census=census, info=info,
            fnv_frames=fnv1a64(region), fnv_pcm=fnv1a64(pcm_bytes),
            sha_region=hashlib.sha256(region).hexdigest(),
            sha_pcm=hashlib.sha256(pcm_bytes).hexdigest(),
        ))

    lines = []
    w = lines.append
    w("//! flac_spike embedded clips — GENERATED")
    w("//! DO NOT EDIT BY HAND.  Regenerate:  scripts/gen_spike_assets.sh")
    w("//! Generator: scripts/frame_vectors.py spike_assets (strict frame finder,")
    w("//! fail-closed arm censuses; FLAC.md 'Perf gate spike' validation rules).")
    w("//! Encoder: %s" % version)
    w("//!")
    w("//! sha256 pins (regeneration tripwires; the witness tests carry the")
    w("//! FNV-1a pins in code, these are for humans reviewing a regen diff):")
    for clip in clips:
        w("//!   %-24s region %s" % (os.path.basename(clip["region_path"]),
                                     clip["sha_region"]))
        w("//!   %-24s pcm    %s" % (clip["prefix"] + "_pcm.bin", clip["sha_pcm"]))
        w("//!   census(%s): %s" % (clip["filename"], clip["census"]))
    w("//!")
    w("//! Offsets are BYTE OFFSETS WITHIN THE EMBEDDED REGION (frame 0 at 0),")
    w("//! the spike's deliberate stand-in for the GAFP manifest's seek table.")
    w("//!")
    w("//! fnv_* are FNV-1a 64-bit: fnv_frames over the region bytes, fnv_pcm")
    w("//! over the reference PCM bytes (16-bit LE interleaved L,R). Witness")
    w("//! layers: the host test recomputes both in Rust and decodes the region")
    w("//! bit-exactly into the PCM; PR 2's ROM recomputes fnv_pcm on hardware.")
    w("")
    w("/// One frame's placement in the region: byte offset and sample count")
    w("/// (the final frame of a track is legitimately short — FLAC.md).")
    w("/// `Clone + Copy`: two plain ints; the witness tests (and PR 4's")
    w("/// buffer rotation) build mutated copies of seek-table slices.")
    w("#[derive(Clone, Copy)]")
    w("pub struct FrameMeta {")
    w("    /// Byte offset of the frame header within the embedded region.")
    w("    pub offset: u32,")
    w("    /// Samples per subframe for THIS frame. Per-frame truth, not a")
    w("    /// track-wide constant: the tail frame is short.")
    w("    pub blocksize: u16,")
    w("}")
    w("")
    w("/// One embedded arm: metadata, the seek table, the `include_bytes!`")
    w("/// region, and the hash pins. The reference PCM lives only in the")
    w("/// `assets/*_pcm.bin` files (host-test ground truth) — the ROM image")
    w("/// never embeds reference data, it pins it by hash.")
    w("pub struct SpikeClip {")
    w("    /// Arm label for logs.")
    w("    pub name: &'static str,")
    w("    /// What this arm measures (generated; logged at ROM boot).")
    w("    pub why: &'static str,")
    w("    /// Measured subframe census (generator refuses a broken census).")
    w("    pub census: &'static str,")
    w("    /// Stream sample rate in Hz (the profile pins one rate per clip).")
    w("    pub sample_rate_hz: u32,")
    w("    /// Stream bit depth (the profile's 16-bit).")
    w("    pub bits_per_sample: u8,")
    w("    /// Channel count (2 for both arms).")
    w("    pub channels: u8,")
    w("    /// Total samples per channel across all frames.")
    w("    pub total_samples: u32,")
    w("    /// Largest per-frame blocksize: the playback buffer size per channel.")
    w("    pub max_blocksize: u16,")
    w("    /// The seek table: frame 0 at offset 0, ascending, tiling exactly.")
    w("    pub frames: &'static [FrameMeta],")
    w("    /// The raw frame region — what `include_bytes!` embeds in the ROM.")
    w("    pub region: &'static [u8],")
    w("    /// File name (under `assets/`) of the reference PCM blob —")
    w("    /// host-test ground truth only, never embedded by the ROM.")
    w("    pub pcm_file: &'static str,")
    w("    /// FNV-1a 64 of `region`.")
    w("    pub fnv_frames: u64,")
    w("    /// FNV-1a 64 of the reference PCM bytes (NOT embedded).")
    w("    pub fnv_pcm: u64,")
    w("}")
    for clip in clips:
        w("")
        w("/// %s" % clip["why"])
        w("pub const %s: SpikeClip = SpikeClip {" % clip["const"])
        w("    name: %s," % qstr(os.path.splitext(clip["filename"])[0]))
        w("    why: %s," % qstr(clip["why"]))
        w("    census: %s," % qstr(clip["census"]))
        w("    sample_rate_hz: %d," % clip["info"]["sample_rate"])
        w("    bits_per_sample: %d," % clip["info"]["bits-per-sample"])
        w("    channels: %d," % clip["info"]["channels"])
        w("    total_samples: %d," % clip["info"]["total samples"])
        w("    max_blocksize: %d," % max(h["blocksize"] for h in clip["frames"]))
        w("    frames: &[")
        for h, off in zip(clip["frames"], clip["offsets"]):
            w("        FrameMeta { offset: %d, blocksize: %d }," % (off, h["blocksize"]))
        w("    ],")
        w("    region: include_bytes!(%s)," % qstr("../assets/" + os.path.basename(clip["region_path"])))
        w("    pcm_file: %s," % qstr(clip["prefix"] + "_pcm.bin"))
        w("    fnv_frames: 0x%016X," % clip["fnv_frames"])
        w("    fnv_pcm: 0x%016X," % clip["fnv_pcm"])
        # A const item's struct-literal initializer must end `};` — `}` alone
        # parses as an expression statement, and the NEXT item's doc comment
        # becomes "unexpected token". (A lexer error earlier in the file
        # masked this on the first build: fix one generated-file bug, the
        # parser advances just far enough to reveal the next. Compile gates.)
        w("};")
        print("   %-16s %9d frame bytes, %6d frames, census: %s"
              % (clip["filename"], len(clip["region"]), len(clip["frames"]),
                 clip["census"]))
    w("")
    w("/// Both arms, in gate order (FIXED first — the arm a decision defaults to).")
    w("pub const CLIPS: [&SpikeClip; 2] = [&L0_FIXED, &L4_LPC];")
    out_path = os.path.join(crate_dir, "src", "assets.rs")
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w") as handle:
        handle.write("\n".join(lines) + "\n")
    print("   wrote %s (%d clips)" % (out_path, len(clips)))


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    what = sys.argv[1]
    if what == "synth":
        synth(sys.argv[2])
    elif what == "emit":
        emit(sys.argv[2], sys.argv[3])
    elif what == "emit_bodies":
        emit_bodies(sys.argv[2], sys.argv[3])
    elif what == "emit_runs":
        emit_runs(sys.argv[2], sys.argv[3])
    elif what == "spike_assets":
        spike_assets(sys.argv[2], sys.argv[3])
    elif what == "measure":
        measure(sys.argv[2])
    else:
        raise SystemExit("usage: frame_vectors.py synth DIR | emit DIR OUT "
                         "| emit_bodies DIR OUT | emit_runs DIR OUT "
                         "| spike_assets DIR CRATE_DIR | measure DIR")


if __name__ == "__main__":
    main()
