#!/usr/bin/env python3
"""frame_vectors.py — deterministic source synthesis + independent FLAC frame
header parser, used to generate the frame-header golden vectors.

Subcommands:

  synth SRC_DIR            write the deterministic source WAVs into SRC_DIR
  emit  SRC_DIR OUT_FILE   parse SRC_DIR/*.flac, write the vector table

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
    print("   synthesized stereo/mono/r65k/silence/tails/rates WAVs in %s" % dst)


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


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    what = sys.argv[1]
    if what == "synth":
        synth(sys.argv[2])
    elif what == "emit":
        emit(sys.argv[2], sys.argv[3])
    elif what == "measure":
        measure(sys.argv[2])
    else:
        raise SystemExit("usage: frame_vectors.py synth DIR | emit DIR OUT "
                         "| measure DIR")


if __name__ == "__main__":
    main()
