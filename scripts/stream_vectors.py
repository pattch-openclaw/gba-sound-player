#!/usr/bin/env python3
"""stream_vectors.py — generate the stream-layer (file-level metadata) fixtures
and their pinned expectations: real FLAC files, committed verbatim, plus a text
table of what their metadata says.

Subcommands:

  synth SRC_DIR                        write the deterministic source WAVs
  emit SRC_DIR FIXTURE_DIR OUT_FILE    validate SRC_DIR/*.flac, copy the real
                                       files into FIXTURE_DIR, write the table

Why a separate generator from frame_vectors.py: the frame vectors pin *bits
inside frame headers*, which need no container at all. This one pins the
container — the `fLaC` signature, the metadata-block walk (§8.1), and the
streaminfo block (§8.2) — because that is the layer split-ladder item 4a adds
(`crates/flac-lite/src/stream.rs`), and because every value in it is something
a real tool already reports: `metaflac --list` prints the block list and the
streaminfo fields, so those numbers are pins, not guesses.

Why the streams are small: the whole point of item 4a is the container, not the
audio. Each encode is a fraction of a second. The wide-header vector (the one
that matters for the frame layer's header-width bound) needs only TWO frames:
an uncommon block-size code and an uncommon sample-rate code sit *after* the
coded frame number, so a first frame's header is already 80 bits wide — no
2^31-frame stream required to witness it (which, incidentally, no fixture could
carry).

Three readings of the same spec, and the table is where they meet:

  * `metaflac --list` — the reference tool's own block list and streaminfo
    field values. These become the pinned rows.
  * the block/streaminfo readers in THIS file — an independent RFC reading,
    same role as frame_vectors.py's header parser. A disagreement refuses to
    write the table.
  * `crates/flac-lite/src/stream.rs` + `tests/stream_metadata_layout.rs` — the
    crate's own reading, asserted against the pins.

Generation-time invariants (fail-closed, `check_stream`):

  * `flac -t` accepts every committed fixture (so the fixture is a file the
    reference decoder accepts, not just bytes that happen to parse).
  * this file's block walk agrees with `metaflac --list` on every block: type,
    last-flag, length, and the frame-data offset that follows the last one.
  * this file's streaminfo reader agrees with `metaflac --list` on all nine
    fields, and the streaminfo block is the first block with length 34.
  * frames are found by frame_vectors.py's proven strict filter (sync shape +
    full field validation + verifying header CRC-8), their count equals
    ceil(total_samples / max_blocksize) wherever total_samples is known, and
    their coded frame numbers are exactly 0..N-1.
  * the header-width census is pinned per fixture: `wide` MUST have a first
    header of exactly 80 bits and none below it, `plain` MUST stay inside the
    48..=72-bit profile our packer produces. A libFLAC upgrade that changes an
    encoder choice refuses to write the table.
  * the unseekable fixture MUST carry the three "unknown" sentinels
    (total_samples 0, both framesizes 0, MD5 all-zero) — that fixture is the
    only witness in this repo that a real stream can declare nothing.
  * every frame number libFLAC wrote MUST be in its MINIMAL coded form
    (Table 18's ranges are disjoint, so "minimal" = "the only form the value
    fits in"), and the frame region is scanned twice by a second, independent
    reading of section 9.1 (`header_at`) to count the byte offsets where a
    header parse starts WITH and WITHOUT that rule (`scan_frame_starts_*`
    rows). The strict count MUST equal the frame count — the independent
    reader has to land on exactly the frames the proven filter finds.
  * the frame footer's CRC-16 covers the FRAME HEADER TOO, not the body alone:
    both are computed over the real frames and compared to the stored footer.
    Measured per fixture rather than recalled, because the number-form question
    below turns on it.
  * every frame's BYTE LENGTH is derived twice, independently, and the two must
    agree on every frame of every fixture: once by walking the frame's own
    subfields bit by bit (`walk_frame_bytes`, which never looks at the next
    frame) and once as the distance to the next frame start / end of file. The
    sum of the per-frame lengths MUST equal the frame region's byte length, so
    the frames tile the region with no gap and no overlap. That is the property
    split-ladder item 4b's frame walk is built on — a frame ends itself — and
    the numbers become the `frame_bytes` / `frame_footer_gaps_bits` rows.
  * the sum of the frames' block sample counts MUST equal the declared
    total_samples when there is one; when there is not (the unseekable
    fixture), it is still what the reference decoder returns, and the two must
    agree.
  * the reference decoder's PCM is MD5'd and, wherever the stream declares an
    MD5, MUST match it — that is the format's own definition of the streaminfo
    MD5 field (the MD5 of the *unencoded* PCM), measured here at two bit depths
    rather than asserted from the spec. The same bytes are FNV-1a-64'd into
    `pcm_fnv1a64` so a whole-file decode has a compact pin.
  * the frame-start filter is measured as a LADDER, per fixture, over the whole
    frame region: sync shape alone (`scan_frame_starts_sync_shape_only`), field
    validation without the CRC (`scan_frame_starts_fields_only`), full strict
    validation (`scan_frame_starts_strict`), and strict validation without the
    minimal-form rule (`scan_frame_starts_all_forms_allowed`). Each count MUST
    contain the one after it and the strict count MUST equal the frame count:
    that is what tells a reader which legs of the filter are load-bearing on
    these bytes, and what a player gives up if it decides to "just scan for
    0xFF".
  * a frame-number FORM mutation on the first fixture: one real frame header
    rewritten with its number in each non-minimal form 2/3/4 octets wide, with
    the header CRC-8 AND the frame CRC-16 recomputed over the widened bytes.
    `flac -d` (decoding, MD5-verifying by default) is run on each mutated
    stream, and it MUST accept it — measured, because this repo's comments
    previously recorded the opposite ("the reference decoder refuses with
    BAD_HEADER"), which was an artifact of leaving the frame footer covering the
    original header bytes. The reference decoder accepts non-minimal forms;
    libFLAC never writes one; the crate refuses them anyway, as a policy
    choice of the format's canonical reading. That divergence is what the
    `mutation*` rows pin.

Determinism: source PCM is synthesized with integer arithmetic only (no float,
no RNG — the same helpers as frame_vectors.py), so the WAVs are byte-identical
everywhere. The *encodes* depend on libFLAC's version; the generator records it
in the table header, exactly as gen_frame_vectors.sh does.

Stdlib only. `flac`/`metaflac` must be on PATH (run via
scripts/gen_stream_vectors.sh).
"""

import hashlib
import os
import struct
import subprocess
import sys
import wave

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import frame_vectors as fv  # noqa: E402  (sibling module: synthesis + frame walk)

# ---------------------------------------------------------------- fixtures

# (name, why this stream exists, what must come out true)
#
# The names are what lands in tests/fixtures/ and what the Rust suite asks for
# by name, so they are part of the test, not a filename.
FIXTURES = [
    dict(
        name="s1_plain_8k_mono",
        wav="s1_plain_8k_mono.wav",
        encode=["flac", "-1", "-f", "-s", "--no-padding", "--no-seektable",
                "-b", "128", "{wav}"],
        blocks=[("STREAMINFO", False), ("VORBIS_COMMENT", True)],
        mutate=True,
        note="two-block container: streaminfo first, a real (empty) Vorbis "
             "comment carrying the last-metadata-block flag. No padding, no "
             "seektable: --no-padding/--no-seektable are encoder options, so "
             "this is libFLAC's own output, not a hand-trimmed file. Also the "
             "host of the frame-number FORM mutation (`mutation "
             "number-form-overlong`): a frame header of this real stream "
             "rewritten with its number in each longer form, both CRCs "
             "recomputed — the only witness here that pins what the reference "
             "decoder does with a non-minimal form, as opposed to what "
             "libFLAC writes.",
    ),
    dict(
        name="s2_seekable_96k_stereo",
        wav="s2_seekable_96k_stereo.wav",
        encode=["flac", "-1", "-f", "-s", "-P", "64", "-b", "4608", "-m",
                "{wav}"],
        blocks=[("STREAMINFO", False), ("SEEKTABLE", False),
                ("VORBIS_COMMENT", False), ("PADDING", True)],
        note="four-block container: every real block type this encoder emits, "
             "three of them skipped by length. A walk that only knows "
             "streaminfo cannot reach the frames; this is the walk.",
    ),
    dict(
        name="s3_wide_48001hz_b3000",
        wav="s3_wide_48001hz_b3000.wav",
        encode=["flac", "-1", "-f", "-s", "--no-padding", "--no-seektable",
                "-b", "3000", "{wav}"],
        blocks=[("STREAMINFO", False), ("VORBIS_COMMENT", True)],
        note="header width OUTSIDE our packer's 48..=72-bit profile at frame "
             "0: 48001 Hz has no 4-bit sample-rate code (0b1101 appends a "
             "16-bit Hz value, section 9.1.7) and 3000 samples has no 4-bit "
             "block-size code (0b0111 appends a 16-bit size-1, section 9.1.6). "
             "Both appended forms sit AFTER the coded frame number (Table 13's "
             "column order — the same order frame_vectors.py's parser follows), "
             "so the FIRST header is 32+8+16+16+8 = 80 bits and both frames "
             "here carry 3000/1801, i.e. the 16-bit form twice. Two frames are "
             "enough: an appended value form is not tied to frame count, and "
             "this is the widest real header this repo commits. Anything wider "
             "also needs the frame NUMBER to reach two octets (128+ frames) — "
             "measured on a 2.1M-frame synthetic stream (-b 257, 44100 Hz) a "
             "single stream contained widths of 80, 88, 96 and 104 bits; that "
             "file is not committed (multi-megabyte for a width rule). The "
             "format's own ceiling, 128 bits — a 6-octet coded number (the most "
             "the 31-bit frame-number bound allows) plus both appended forms "
             "plus the CRC — is therefore witnessed by a computed synthetic "
             "header in the Rust suite, not by a fixture. Measured: this encode "
             "needs no --lax (same as the uncommon-rate streams "
             "gen_frame_vectors.sh already encodes) — a wide header is not the "
             "same thing as an unstreamable stream. If a future libFLAC stops "
             "using the 16-bit forms here, the census below refuses to write.",
    ),
    dict(
        name="s4_24bit_8k_mono",
        wav="s4_24bit_8k_mono.wav",
        encode=["flac", "-1", "-f", "-s", "--no-padding", "--no-seektable",
                "-b", "128", "{wav}"],
        blocks=[("STREAMINFO", False), ("VORBIS_COMMENT", True)],
        note="a valid 24-bit stream: the container is ordinary and the "
             "reference decoder verifies it, while this crate's decoder "
             "carries only 8/16-bit samples (frame.rs bits_per_sample). The "
             "stream layer refuses it as UnsupportedSampleSize, so the "
             "capability statement has a witness on real bytes rather than a "
             "sentence.",
    ),
    dict(
        name="s5_unseekable_total_unknown",
        raw="s5_unseekable_total_unknown.raw",   # the same 1200 samples as s1
        encode=["flac", "-1", "-f", "-s", "--no-padding", "--no-seektable",
                "--force-raw-format", "--endian=little", "--sign=signed",
                "--sample-rate=8000", "--channels=1", "--bps=16", "-b", "128",
                "-"],
        raw_pipe=True,
        blocks=[("STREAMINFO", False), ("VORBIS_COMMENT", True)],
        note="the same audio as s1, fed as raw s16le PCM through a pipe and "
             "encoded to stdout: total samples 0 (unknown), min/max frame size "
             "0 (unknown), MD5 all-zero (unset). `flac -t` still verifies it. "
             "This is the fixture that says a decoder may not count frames from "
             "total_samples — and the measured reason it exists: piping a WAV is "
             "NOT enough, because the WAV header states the sample count before "
             "a byte of audio is read, so a piped WAV still declares "
             "total_samples (measured, libFLAC 1.5.0). Only raw-in-pipe/pipe-out "
             "produces a stream that genuinely declares nothing.",
    ),
]

# Low-rate partials: periods in samples chosen for an 8 kHz source (100-400 Hz
# content) with the same broadband intent as frame_vectors.py's PARTIALS_32K.
PARTIALS_8K = [(40, 5), (61, 4), (100, 3), (25, 2)]

STREAMINFO_FIELDS = [
    ("minimum_blocksize", 16),
    ("maximum_blocksize", 16),
    ("minimum_framesize", 24),
    ("maximum_framesize", 24),
    ("sample_rate_hz", 20),
    ("channels_minus_one", 3),
    ("bits_per_sample_minus_one", 5),
    ("total_samples", 36),
]
STREAMINFO_LEN = 34  # 272 bits including the 128-bit MD5 (Table 3)

# How metaflac labels each field in --list output, in Table 3's order.
METAFLAC_FIELDS = [
    "minimum blocksize", "maximum blocksize", "minimum framesize",
    "maximum framesize", "sample_rate", "channels", "bits-per-sample",
    "total samples", "MD5 signature",
]

BLOCK_TYPE_NAMES = {
    0: "STREAMINFO", 1: "PADDING", 2: "APPLICATION", 3: "SEEKTABLE",
    4: "VORBIS_COMMENT", 5: "CUESHEET", 6: "PICTURE",
}

# RFC 9639 Table 18, left column: the value ranges the coded-number forms
# cover. They are DISJOINT and in ascending order, which is what "each value
# has exactly one minimal form" means. Written here independently of
# `fv.coded_number_octets` (which maps a lead byte to a width — the other
# direction) and of `CODED_NUMBER_FORM_MIN` in the crate.
NUMBER_FORM_MAX = [0x7F, 0x7FF, 0xFFFF, 0x1FFFFF, 0x3FFFFFF, 0x7FFFFFF,
                   0xFFFFFFFFF]


def min_number_octets(value):
    """The width of the minimal form of a coded number (Table 18)."""
    for octets, high in enumerate(NUMBER_FORM_MAX, start=1):
        if value <= high:
            return octets
    raise AssertionError("%d exceeds Table 18's 36-bit maximum" % value)


# --------------------------------------------------- an independent 9.1 reader

def header_at(data, pos, allow_nonminimal, require_crc=True):
    """A second, independent reading of the RFC 9639 9.1 frame header, written
    for one question: how many byte offsets in a frame region does a header
    parse accept, with the minimal-form rule (`allow_nonminimal=False`, the
    crate's rule) and without it.

    `require_crc=False` drops the header CRC-8 check while keeping every field
    validation: that is the second leg of the filter ladder the
    `scan_frame_starts_*` rows measure (sync shape, then field validation,
    then CRC-8, then the minimal-form rule). It is a measurement switch, not a
    decoding rule — the crate always verifies the CRC.

    It is NOT the production reader and not the vector source — `fv.parse_frame_header`
    is, and `frame_vectors.py` is the module the Rust frame suite is written
    against. What this function adds is the `allow_nonminimal` switch, which
    `fv`'s reader does not have. It is cross-checked against `fv` below: on
    every real frame of every fixture the two must agree, and the
    minimal-form count must equal the frame count. If they ever disagree this
    generator refuses to write.
    """
    if pos + 6 > len(data):
        return None
    bits = "".join(format(x, "08b") for x in data[pos:pos + 4])
    if bits[:14] != "11111111111110":      # 14-bit sync, fixed-blocksize
        return None
    if bits[14] != "0" or bits[15] != "0":   # reserved, blocking strategy
        return None
    bs_code = bits[16:20]
    rate_code = bits[20:24]
    chan_code = int(bits[24:28], 2)
    size_code = int(bits[28:31], 2)
    if bits[31] != "0":                      # reserved bit
        return None
    # The code→value tables come from frame_vectors.py — one source of truth
    # for those tables; what is independent here is the ORDER of the fields and
    # the width arithmetic that follows them (and the minimal-form rule).
    bs_kind = fv.BLOCKSIZE.get(int(bs_code, 2))
    if bs_kind is None or fv.SAMPLESIZE.get(size_code) is None or \
            fv.CHANNELS.get(chan_code, (0,))[0] == 0:
        return None

    lead = data[pos + 4]
    octets = fv.coded_number_octets(lead)
    if octets is None:
        return None
    raw = data[pos + 4:pos + 4 + octets]
    if len(raw) < octets or any(b & 0xC0 != 0x80 for b in raw[1:]):
        return None
    value = raw[0] & (0x7F if octets == 1 else (0xFF >> (octets + 1)))
    for byte in raw[1:]:
        value = (value << 6) | (byte & 0x3F)
    if not allow_nonminimal and octets != min_number_octets(value):
        return None

    at = pos + 4 + octets
    # The appended value forms come after the coded number, block size first
    # then sample rate (Table 13's column order — the order the crate's parser
    # and fv's follow too, and the reason the two can't be summed separately).
    if bs_kind == "u8":
        at += 1                              # 8-bit block size, stored minus 1
    elif bs_kind == "u16":
        at += 2                              # 16-bit block size, stored minus 1
    if rate_code == "1101":
        at += 2                              # 16-bit sample rate
    elif rate_code == "1110":
        at += 3                              # 24-bit sample rate
    if at >= len(data):
        return None
    crc_ok = fv.crc8(data[pos:at]) == data[at]
    if require_crc and not crc_ok:
        return None
    return dict(octets=octets, number=value, width=at + 1 - pos, crc_ok=crc_ok)


def count_frame_starts(data, start, allow_nonminimal, require_crc=True):
    """Every byte offset in the frame region that a header parse accepts."""
    found = []
    for pos in range(start, len(data)):
        header = header_at(data, pos, allow_nonminimal, require_crc)
        if header is not None:
            found.append((pos, header))
    return found


def count_sync_shape(data, start):
    """Every offset whose next two bytes have a fixed-blocksize frame SYNC
    SHAPE: 0xFF, then a byte whose top five bits are 111110 — the two-byte
    test `fv.find_frames` uses as its pre-filter, and the one a player is
    tempted to call "find the next frame". Counted here because whether the
    stricter legs of the filter matter is a measurement, not a mannerism."""
    return sum(1 for pos in range(start, len(data) - 1)
               if data[pos] == 0xFF and (data[pos + 1] & 0xF8) == 0xF8)


def sync_shape_at(data, pos):
    """The two-byte sync-shape test at ONE offset (see count_sync_shape)."""
    return (pos + 1 < len(data) and data[pos] == 0xFF
            and (data[pos + 1] & 0xF8) == 0xF8)


def bits_at(data, bit, n):
    """The next `n` bits at a BIT offset, MSB-first, as an integer."""
    out = 0
    for i in range(n):
        byte, off = divmod(bit + i, 8)
        out = (out << 1) | ((data[byte] >> (7 - off)) & 1)
    return out


def sync_shape_at_bits(data, bit):
    """`sync_shape_at` at a bit offset (see bits_at)."""
    return (bit + 16 <= len(data) * 8
            and (bits_at(data, bit, 16) & 0xFFF8) == 0xFFF8)


def strict_header_at_bit(data, bit):
    """The SAME strict frame-start filter used for `scan_frame_starts_strict`
    (§9.1 fields, minimal coded forms, CRC-8), asked at a BIT offset by
    re-aligning the bytes to that bit.

    A reader that mis-counted a frame's footer lands between two byte
    boundaries, so the honest question about such a position cannot be asked of
    a byte-rounded view of it — that would be answering a different question and
    reporting it as this one. Returns the header dict or None, exactly as
    `header_at` does."""
    byte, off = divmod(bit, 8)
    if byte + 17 > len(data):
        return None
    text = "".join(format(x, "08b") for x in data[byte:byte + 17])
    text = text[off:]
    buf = bytes(int(text[i:i + 8], 2) for i in range(0, len(text) - 7, 8))
    return header_at(buf, 0, False)


# Which subframe slot carries the SIDE signal, and so is one bit wider than the
# stream's declared depth (section 9.1.3's three decorrelation codes; there is
# no "swapped" variant). A bit-level frame walk needs this to size the slots.
SIDE_SLOT = {"left-side": 1, "mid-side": 1, "side-right": 0}


def walk_frame_bytes(data, frame, stream_bps):
    """A frame's total byte length, from its header's last bit through every
    subframe body to the footer — WITHOUT looking at where the next frame
    starts.

    This is the property the frame walk in crates/flac-lite/scan.rs depends on:
    a frame ENDS ITSELF. The walk returns the frame's length as the number of
    bytes its own fields occupy (plus the padding bit(s) and the 16-bit CRC-16
    that close it), so a reader can advance by that many bytes without an index
    and without a byte count. `check_stream` then cross-checks it against the
    framing (next frame's offset, or end of file): two derivations of the same
    number, one from the bits, one from the neighbour."""
    bps = frame["bps"] or stream_bps
    side = SIDE_SLOT.get(frame["decorrelation"])
    # Bits is absolute over the file: the first subframe field starts at the
    # frame's offset plus its header width, both in bits.
    bits = fv.Bits(data, frame["offset"] * 8 + frame["header_bits"])
    for slot in range(frame["subframes"]):
        # A side slot is stored one bit wider than the frame's declared depth
        # (section 9.1: the extra bit carries the range the decorrelation
        # adds). subframe.rs' slot rule, read independently here.
        width = bps + (1 if slot == side else 0)
        fv.walk_subframe(bits, frame["blocksize"], width)
    exit_bits = bits.pos
    # The footer: zero-padding to the byte boundary, then CRC-16. Measured with
    # the frame CRC-16 over header+body (frame_crc_covers_the_header), which is
    # what makes the padding length determinable rather than inferred.
    gap = 16 if exit_bits % 8 == 0 else (8 - exit_bits % 8) + 16
    # §12's "zeros ... until the next byte boundary" bounds the padding at 7
    # bits, so this tail can only be 16..23. Asserted, not assumed: the row is
    # only a witness about padding if generation refuses a frame whose tail is
    # longer than the format allows (a longer one would mean my walk stopped
    # short of the real body end, and `frame_bytes` with it).
    assert 16 <= gap <= 23, "frame at %d has a %d-bit footer tail" % (
        frame["offset"], gap)
    return (exit_bits + gap + 7) // 8 - frame["offset"], gap


# ----------------------------------------------------------- the form mutation

def crc16(data, init=0):
    """The frame footer's CRC-16 (poly 0x8005, init 0, no reflection) —
    reimplemented here from the format, independently of the crate.

    The coverage question is settled by measurement, not memory: on every frame
    of every fixture this, fed header+body, equals the stored footer, and
    fed body alone it does not (see `frame_crc_covers_the_header`)."""
    crc = init
    for byte in data:
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x8005) & 0xFFFF if crc & 0x8000 else crc << 1
    return crc & 0xFFFF


def frame_crc_covers_the_header(data, frames):
    """Measure which bytes the frame footer's CRC-16 covers, on real bytes.

    It covers the FRAME HEADER TOO, not just the body — that is why changing a
    header's bytes at all (even rewriting a block size in an equivalent form)
    makes the reference decoder report FRAME_CRC_MISMATCH. Recorded because a
    whole discussion about overlong frame numbers rested on it.
    """
    for a, b in zip(frames, frames[1:] + [None]):
        end = len(data) if b is None else b["offset"]
        frame = data[a["offset"]:end]
        stored = int.from_bytes(frame[-2:], "big")
        head = a["header_bits"] // 8
        assert crc16(frame[:-2]) == stored, \
            "frame CRC-16 does not cover header+body at %d" % a["offset"]
        assert crc16(frame[head:-2]) != stored, \
            "frame CRC-16 covers the body alone at %d" % a["offset"]


def mutate_number_form(data, frame, frame_end, forms):
    """Rewrite one real frame's number in each non-minimal form that is `forms`
    octets wide, recomputing BOTH CRCs — the header's CRC-8 over the widened
    header, and the frame's CRC-16 over the widened header including that
    CRC-8 byte plus the untouched body.

    The second one is the whole point. Recomputing only the header CRC leaves
    the frame footer covering the ORIGINAL header bytes (the frame CRC-16 covers
    the header too — measured, see `frame_crc_covers_the_header`), so the
    reference decoder fails the frame: FRAME_CRC_MISMATCH, or BAD_HEADER if the
    header CRC was left stale as well. Neither says anything about the number
    form. With both recomputed the frame is sound and the only variable left is
    how the number is spelled — the question this witness exists to answer.

    Returns [(octets, mutated_header_hex, mutated_stream)].
    """
    pos = frame["offset"]
    head_fixed = data[pos:pos + 4]
    extra = data[pos + 4 + frame["octets"]:pos + frame["crc_pos"]]
    body = data[pos + frame["crc_pos"] + 1:frame_end - 2]
    out = []
    for octets in forms:
        lead = {2: 0xC0, 3: 0xE0, 4: 0xF0}[octets]
        number = bytes([lead] + [0x80] * (octets - 1))
        hdr = head_fixed + number + extra
        crc = fv.crc8(hdr)
        # The frame CRC-16 covers the header INCLUDING its CRC-8 byte, which is
        # why the coverage below is hdr+crc+body and not hdr+body — the fact
        # `frame_crc_covers_the_header` measures on the real frames above.
        new_frame = (hdr + bytes([crc]) + body +
                     crc16(hdr + bytes([crc]) + body).to_bytes(2, "big"))
        out.append((octets,
                    " ".join("%02X" % b for b in hdr + bytes([crc])),
                    data[:pos] + new_frame + data[frame_end:]))
    return out


def reference_decode(path):
    """Run the reference decoder over `path`, verifying as it decodes (flac's
    default: decode, then MD5-compare against streaminfo). Returns
    (exit code, first stderr line or '')."""
    proc = subprocess.run(["flac", "-d", "-f", "-s", "-o", os.devnull, path],
                          capture_output=True, text=True)
    lines = (proc.stderr or "").strip().splitlines()
    return proc.returncode, (lines[0].strip() if lines else "")


# ---------------------------------------------------------------- synthesis

def write_wav16(path, rate, channels, samples):
    fv.write_wav(path, rate, channels, samples)  # same 16-bit writer


def write_wav24(path, rate, samples):
    """24-bit mono WAV. The samples are the 16-bit synthesis shifted up 8 bits,
    so the low byte is zero: the point is the declared bit depth, not extra
    precision, and libFLAC writes a 24-bit streaminfo from it either way."""
    with wave.open(path, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(3)
        wav.setframerate(rate)
        packed = bytearray()
        for v in samples:
            packed += struct.pack("<i", int(v) * 256)[:3]  # low 3 bytes, LE
        wav.writeframes(bytes(packed))


def synth(dst):
    """Deterministic sources, all of them a fraction of a second long."""
    # 1200 samples at 8 kHz = 0.15 s: ten 128-sample frames, nothing else.
    mono_8k = fv.synth_pcm(0.15, 8000, PARTIALS_8K, 61, 3, 30, 0,
                           mono=True, exact=1200)
    write_wav16(os.path.join(dst, "s1_plain_8k_mono.wav"), 8000, 1, mono_8k)
    # 9600 sample-frames stereo at 96 kHz: three 4608-sample frames (last short).
    stereo_96k = fv.synth_pcm(0.1, 96000, fv.PARTIALS_64K, 257, 3, 30, 30,
                              exact=9600)
    write_wav16(os.path.join(dst, "s2_seekable_96k_stereo.wav"), 96000, 2,
                stereo_96k)
    # 4801 samples at 48001 Hz with -b 3000 -> two frames (3000 + 1801); both
    # sizes are outside the 4-bit table AND outside the 8-bit uncommon form
    # (>255), so BOTH headers take the 16-bit block-size form.
    mono_48k = fv.synth_pcm(0.1, 48001, fv.PARTIALS_64K, 257, 3, 30, 0,
                            mono=True, exact=4801)
    write_wav16(os.path.join(dst, "s3_wide_48001hz_b3000.wav"), 48001, 1,
                mono_48k)
    write_wav24(os.path.join(dst, "s4_24bit_8k_mono.wav"), 8000, mono_8k)
    # s5's source is the SAME 1200 samples, as bare samples: the container is
    # what differs, and that is the whole point of the fixture.
    with open(os.path.join(dst, "s5_unseekable_total_unknown.raw"), "wb") as raw:
        raw.write(struct.pack("<%dh" % len(mono_8k), *mono_8k))
    print("   synthesized 4 short WAVs + 1 raw sample stream (8k mono, 96k "
          "stereo, 48001Hz, 24-bit, and s1's PCM as bare s16le) in %s" % dst)


# ---------------------------------------------------------------- independent
# readers of RFC 9639 section 8 (container). These are the oracle's second
# opinion: metaflac reports the same numbers, and a disagreement stops the
# generator.

def walk_blocks(data):
    """Section 8.1 block walk. Returns (blocks, frame_data_offset) where blocks
    are dicts with offset/type/length/is_last, in stream order."""
    assert data[:4] == b"fLaC", "not a FLAC stream: %r" % data[:4]
    blocks, off = [], 4
    while True:
        assert off + 4 <= len(data), "truncated metadata block header at %d" % off
        first = data[off]
        is_last = bool(first & 0x80)
        kind = first & 0x7F
        length = int.from_bytes(data[off + 1:off + 4], "big")
        assert off + 4 + length <= len(data), \
            "metadata block %d (%s) claims %d bytes, only %d remain" % (
                len(blocks), BLOCK_TYPE_NAMES.get(kind, kind), length,
                len(data) - off - 4)
        blocks.append(dict(offset=off, type=kind, length=length, is_last=is_last,
                           payload=data[off + 4:off + 4 + length]))
        off += 4 + length
        if is_last:
            return blocks, off


def read_streaminfo(payload):
    """Section 8.2 Table 3, read as bits — an independent reading, not
    metaflac's and not the crate's."""
    assert len(payload) == STREAMINFO_LEN, \
        "streaminfo payload is %d bytes, Table 3 says %d" % (
            len(payload), STREAMINFO_LEN)
    pos, info = 0, {}

    def take(name, n):
        nonlocal pos
        v = 0
        for i in range(pos, pos + n):
            v = (v << 1) | ((payload[i // 8] >> (7 - i % 8)) & 1)
        pos += n
        info[name] = v

    for name, width in STREAMINFO_FIELDS:
        take(name, width)
    assert pos == 144, "Table 3 fields before the MD5 should end at bit 144"
    info["md5"] = payload[18:34].hex()
    assert pos + 128 == STREAMINFO_LEN * 8
    return info


def metaflac_list(path):
    """The reference tool's own reading, as pins."""
    out = subprocess.run(["metaflac", "--list", path], capture_output=True,
                        text=True, check=True).stdout
    blocks, info, current = [], {}, None
    for line in out.splitlines():
        line = line.strip()
        if line.startswith("METADATA block #"):
            current = {}
            blocks.append(current)
            continue
        if current is None:
            continue
        key, _, value = line.partition(":")
        key, value = key.strip(), value.strip()
        if key == "type":
            num, _, label = value.partition("(")
            current["type"] = int(num)
            current["type_name"] = label.rstrip(")")
        elif key == "is last":
            current["is_last"] = value == "true"
        elif key == "length":
            current["length"] = int(value.split()[0])
        elif key == "vendor string":
            info["vendor"] = value
        elif key in METAFLAC_FIELDS:
            info[key] = value
    return blocks, info


# ---------------------------------------------------------------- validation

def encode_all(src):
    """Run the encodes the table describes. They live HERE, next to the rows
    that report them, so an `encode_command` row cannot drift away from the
    command that actually produced the bytes."""
    for spec in FIXTURES:
        flac = os.path.join(src, spec["name"] + ".flac")
        argv = [a.format(raw=spec.get("raw", ""), wav=spec.get("wav", ""))
                for a in spec["encode"]]
        with open(flac, "wb") as out:
            if spec.get("raw_pipe"):
                with open(os.path.join(src, spec["raw"]), "rb") as stdin:
                    subprocess.run(argv, cwd=src, stdin=stdin, stdout=out,
                                   check=True)
            else:
                subprocess.run(argv + ["-o", os.path.basename(flac)], cwd=src,
                               check=True)
        print("   encoded %s" % os.path.basename(flac))


def check_stream(spec, path, data):
    """All the fail-closed invariants for one fixture. Returns the rows to pin."""
    name = spec["name"]
    subprocess.run(["flac", "-t", "-s", path], check=True)  # reference-verifies

    blocks, off = walk_blocks(data)
    mblocks, minfo = metaflac_list(path)
    assert len(mblocks) == len(blocks), \
        "%s: walk sees %d blocks, metaflac lists %d" % (name, len(blocks),
                                                        len(mblocks))
    for mine, theirs in zip(blocks, mblocks):
        assert mine["type"] == theirs["type"], "%s: block type %s vs %s" % (
            name, mine, theirs)
        assert BLOCK_TYPE_NAMES.get(mine["type"], str(mine["type"])) == \
            theirs["type_name"], "%s: block type name disagrees" % name
        assert mine["is_last"] == theirs["is_last"], "%s: last flag" % name
        assert mine["length"] == theirs["length"], "%s: length" % name
    census = [(BLOCK_TYPE_NAMES.get(b["type"], b["type"]), b["is_last"])
              for b in blocks]
    assert census == spec["blocks"], "%s: block census is %s, expected %s" % (
        name, census, spec["blocks"])
    assert blocks[0]["type"] == 0 and blocks[0]["length"] == STREAMINFO_LEN, \
        "%s: first block must be a 34-byte streaminfo" % name

    # Table 3, two readings: ours (bits) and metaflac's (its own parser).
    info = read_streaminfo(blocks[0]["payload"])
    values = [minfo[label] for label in METAFLAC_FIELDS]
    assert info["minimum_blocksize"] == int(values[0].split()[0])
    assert info["maximum_blocksize"] == int(values[1].split()[0])
    assert info["minimum_framesize"] == int(values[2].split()[0])
    assert info["maximum_framesize"] == int(values[3].split()[0])
    assert info["sample_rate_hz"] == int(values[4].split()[0])
    assert info["channels_minus_one"] == int(values[5]) - 1
    assert info["bits_per_sample_minus_one"] == int(values[6]) - 1
    assert info["total_samples"] == int(values[7])
    assert info["md5"] == values[8].replace(" ", "")
    assert off == fv.first_frame_offset(data), "frame-data offset disagreement"

    # The frames, with frame_vectors.py's proven strict filter.
    frames = fv.find_frames(data)
    numbers = [f["number"] for f in frames]
    assert numbers == list(range(len(frames))), "%s: frame numbers %r" % (
        name, numbers[:8])
    assert all(f["crc_ok"] for f in frames), "%s: a header CRC fails" % name
    widths = sorted({f["header_bits"] for f in frames})
    if info["total_samples"]:
        want = -(-info["total_samples"] // info["maximum_blocksize"])
        assert len(frames) == want, "%s: %d frames, total_samples says %d" % (
            name, len(frames), want)
        assert info["minimum_framesize"] and info["maximum_framesize"], \
            "%s: a seekable encode should know its frame sizes" % name
        assert info["md5"] != "0" * 32, "%s: a seekable encode should know MD5"
    else:
        assert info["minimum_framesize"] == 0 == info["maximum_framesize"], \
            "%s: unknown total samples should travel with unknown framesizes" % name
        assert info["md5"] == "0" * 32, "%s: unseekable MD5 must be unset" % name
    # The unknown-value fixture is the only one allowed to be unknown; if any
    # other encode ever starts declaring 0, this is where it gets loud.
    assert (info["total_samples"] == 0) == bool(spec.get("raw_pipe")), \
        "%s: total_samples unknown-ness changed" % name

    # Coded-number FORM census (Table 18's left column). Everything libFLAC
    # writes is minimal — which is what makes refusing the other forms free.
    nonminimal = [(f["number"], f["octets"], min_number_octets(f["number"]))
                  for f in frames
                  if f["octets"] != min_number_octets(f["number"])]
    assert not nonminimal, \
        "%s: libFLAC wrote a non-minimal frame number: %r" % (name, nonminimal[:4])

    # What that strictness costs, measured on these same bytes: count the
    # offsets in the frame region where a header parse starts, with the
    # minimal-form rule and without it. `header_at` is a second independent
    # reading of 9.1 (see its docstring); with the rule it must find exactly
    # the frames `fv.find_frames` found — same offsets, same count — and then
    # the difference between the two counts IS the frame-start surface the
    # rule removes. No estimate: if it is zero on a fixture, that goes in the
    # table too.
    strict = count_frame_starts(data, off, False)
    lax = count_frame_starts(data, off, True)
    assert [p for p, _ in strict] == [f["offset"] for f in frames], \
        "%s: the independent 9.1 reader disagrees with the frame filter: %r" % (
            name, [p for p, _ in strict][:8])
    assert {p for p, _ in strict} <= {p for p, _ in lax}, \
        "%s: the minimal-form rule removed a real frame" % name

    # The rest of the filter ladder, same region, same second reading of 9.1:
    # what a scanner sees with the CRC-8 leg removed, and with only the sync
    # SHAPE (no field validation at all). Each set must contain the one after
    # it; the strict count is the frame count. If a leg stops mattering on
    # these bytes the table says so — and if a leg starts admitting false
    # starts, the table says that too, which is the measurement that decides
    # whether a player may treat "scan for 0xFF" as a frame finder.
    fields = count_frame_starts(data, off, False, require_crc=False)
    shape = count_sync_shape(data, off)
    assert {p for p, _ in strict} <= {p for p, _ in fields} <= \
        {p for p in range(off, len(data))}, \
        "%s: removing the CRC-8 leg removed a real frame" % name
    assert shape >= len(fields), \
        "%s: the sync-shape count (%d) is below the field-validated count " \
        "(%d) — a validated start must have the shape" % (name, shape, len(fields))

    # Which bytes the frame footer's CRC-16 covers — measured on these frames
    # rather than recalled, because the number-form question below turns on it.
    frame_crc_covers_the_header(data, frames)

    # A frame ENDS ITSELF: every frame's byte length derived by walking its own
    # subfields, and cross-checked against the framing distance to the next
    # frame. See walk_frame_bytes(). This is the property item 4b's frame walk
    # uses instead of an index, a byte count or an offset table.
    stream_bps = info["bits_per_sample_minus_one"] + 1
    walked, gaps, header_bits, size_codes = [], [], [], []
    for i, frame in enumerate(frames):
        length, gap = walk_frame_bytes(data, frame, stream_bps)
        framed = (frames[i + 1]["offset"] if i + 1 < len(frames)
                  else len(data)) - frame["offset"]
        assert length == framed, \
            "%s: frame %d is %d bytes by its own fields, %d by the framing " \
            "(%d %s %d) — a body field is mis-sized or the region does not " \
            "tile" % (name, frame["number"], length, framed, frame["offset"],
                      "->" if i + 1 < len(frames) else "-> end of file",
                      frames[i + 1]["offset"] if i + 1 < len(frames) else len(data))
        assert frame["offset"] + length > frame["offset"] + frame["header_bits"] // 8, \
            "%s: frame %d's body walk ends inside its own header" % (name, i)
        walked.append(length)
        gaps.append(gap)
        # The header's own width, per frame, from the independent parser. With
        # `frame_bytes` and `frame_footer_gaps_bits` this is what lets a reader
        # reconstruct a frame's subframe-body length by subtraction — framing is
        # compositional, and a decoder that disagrees about any one of the three
        # disagrees about where the next frame starts.
        header_bits.append(frame["header_bits"])
        # What the frame SAYS about its sample depth (§9.1.5 Table 17). Code
        # 0b000 means "only the stream states it" and resolves through
        # StreamDefaults in the crate; any other code is the frame's own
        # statement, and a real encoder's statements have to agree with its
        # streaminfo — measured here rather than assumed, because a frame walk's
        # profile check reads the RESOLVED value, and whether that value came
        # from the frame or from the caller changes what a caller can argue with.
        code = frame["size_code"]
        size_codes.append(code)
        stated = fv.SAMPLESIZE.get(code)
        if stated is not None:
            assert stated == stream_bps, \
                "%s: frame %d states %d-bit samples, its streaminfo says %d" % (
                    name, frame["number"], stated, stream_bps)
    assert sum(walked) == len(data) - off, \
        "%s: the frames' own lengths total %d, the frame region is %d bytes" % (
            name, sum(walked), len(data) - off)
    assert frames[-1]["offset"] + walked[-1] == len(data), \
        "%s: the last frame does not end on the file's last byte" % name

    # WHERE A FOOTER MISTAKE WOULD LAND — measured, and recorded as rows.
    # §12 closes a frame with "zero padding to the next byte boundary" and THEN
    # the CRC-16, so a reader that treated the footer as exactly 16 bits ends
    # short of the next frame's first byte on every frame whose padding is
    # non-zero (`footer_padding_frames`: how many of this fixture's frames that
    # is). At those positions this reading is asked two questions: does the
    # STRICT filter accept them, and does the bare sync SHAPE match them. The
    # pair is the whole argument for the field-validation leg: a position whose
    # shape matches and whose fields do not is exactly where a player that
    # trusted its eyes starts a phantom frame. The byte-truncation shape is
    # asked the same two questions — `end - 1`, the position a reader lands on
    # if it rounds a non-byte-aligned end DOWN instead of up (which is also why
    # the crate's walk measures bit ends rather than rounding them).
    # No asserts here: the counts are facts about these fixtures, not logical
    # necessities. They are rows so the prose in the crate and in FLAC.md cites
    # numbers a reader can re-run, and the Rust suite asserts the byte-level
    # ones against its own filter — a fixture that changed an answer would fail
    # a test rather than quietly invalidate a paragraph.
    pad_frames = 0
    slip_shape = slip_headers = 0
    early_shape = early_headers = early_positions = 0
    for i, frame in enumerate(frames):
        end = frame["offset"] + walked[i]
        if gaps[i] != 16:
            pad_frames += 1
            bit = end * 8 - (gaps[i] - 16)
            if sync_shape_at_bits(data, bit):
                slip_shape += 1
            if strict_header_at_bit(data, bit) is not None:
                slip_headers += 1
        if i + 1 < len(frames):
            early_positions += 1
            if sync_shape_at(data, end - 1):
                early_shape += 1
            if header_at(data, end - 1, False) is not None:
                early_headers += 1

    # What the stream's own declarations buy at the END of a frame walk. The
    # last frame's block size is the arithmetic total_samples makes possible
    # and nothing else does: with a fixed block size and a known sample count
    # the tail is total % max_blocksize samples (and a streaminfo with a fixed
    # block size is what makes the frame COUNT countable at all, so an exact
    # count is available for the fixed-blocksize fixtures and not for the
    # unseekable one). Where total_samples is unknown, no declaration tells a
    # reader where the audio ends — the region's end does.
    blocksizes = [f["blocksize"] for f in frames]
    samples = sum(blocksizes)
    derivable = (info["total_samples"] > 0 and
                 info["minimum_blocksize"] == info["maximum_blocksize"])
    derived = (-(-info["total_samples"] // info["maximum_blocksize"])
               if derivable else None)
    if derived is not None:
        assert derived == len(frames), \
            "%s: the stream's own declarations say %d frames, there are %d" % (
                name, derived, len(frames))
        assert samples == info["total_samples"], \
            "%s: frames carry %d interchannel samples, total_samples says %d" % (
                name, samples, info["total_samples"])
        assert blocksizes[-1] == info["total_samples"] % info["maximum_blocksize"] \
            or blocksizes[-1] == info["maximum_blocksize"], \
            "%s: tail frame is %d samples, total_samples says otherwise" % (
                name, blocksizes[-1])

    # The audio itself, from the reference DECODER, and the stream's own MD5
    # checked against it: section 8.2's MD5 field is the MD5 of the UNENCODED
    # PCM, so the fixture's declared value and `flac -d`'s output meeting is a
    # fact about the format, not about this harness. Only the 16-bit fixtures
    # get PCM rows: `reference_pcm` reads a decoded WAV, and the 24-bit fixture
    # is refused by this crate's decoder at the depth gate anyway (its frames
    # are still walked above, because the frame layer's framing rule has
    # nothing to do with sample depth).
    pcm_rows = None
    if stream_bps == 16:
        pcm = fv.reference_pcm(path)
        assert len(pcm) == samples * (info["channels_minus_one"] + 1), \
            "%s: reference decode produced %d values, the frames describe %d" % (
                name, len(pcm), samples * (info["channels_minus_one"] + 1))
        packed = struct.pack("<%dh" % len(pcm), *pcm)
        digest = hashlib.md5(packed).hexdigest()
        if info["md5"] != "0" * 32:
            assert digest == info["md5"], \
                "%s: MD5 of the reference-decoded PCM is %s, the stream " \
                "declares %s" % (name, digest, info["md5"])
        else:
            assert digest != "0" * 32, \
                "%s: the unseekable fixture's MD5 field is unset; the decoded " \
                "PCM hashing to the unset sentinel would be a coincidence worth " \
                "knowing about" % name
        pcm_rows = dict(values=len(pcm), bytes=packed, md5=digest,
                        pcm_fnv=fv.fnv1a64(packed))

    # The number-FORM witness: one real frame, its number spelled in three
    # longer forms, both CRCs recomputed. See mutate_number_form().
    mutation = None
    if spec.get("mutate"):
        end = frames[1]["offset"] if len(frames) > 1 else len(data)
        forms = {}
        for octets, spelled, blob in mutate_number_form(data, frames[0], end,
                                                        (2, 3, 4)):
            accepted = header_at(blob, off, True)
            assert accepted is not None and accepted["number"] == 0 and \
                accepted["width"] == len(spelled.split()), \
                "%s: the lenient reader should accept the %d-octet form" % (
                    name, octets)
            assert header_at(blob, off, False) is None, \
                "%s: the minimal-form rule should reject the %d-octet form" % (
                    name, octets)
            tmp = path + ".form%d" % octets
            with open(tmp, "wb") as handle:
                handle.write(blob)
            code, line = reference_decode(tmp)
            os.remove(tmp)
            assert code == 0, \
                "%s: the reference decoder rejected the %d-octet form: %s" % (
                    name, octets, line)
            forms[octets] = dict(header_hex=spelled, reference_exit=code,
                                 reference_stderr=line or "none")
        mutation = dict(
            at=off,
            before_hex=" ".join("%02X" % b for b in
                                data[off:off + frames[0]["crc_pos"] + 1]),
            forms=forms,
        )

    # Header-width census, pinned per fixture. This is the row the frame
    # layer's supported-width bound is derived from; if a libFLAC upgrade
    # changes what the encoder chooses, this refuses to write rather than
    # silently moving the goalposts under the Rust test.
    if spec["name"] == "s3_wide_48001hz_b3000":
        assert frames and frames[0]["header_bits"] == 80, \
            "%s: first header should be 80 bits, census %s" % (name, widths)
        assert min(widths) >= 80, "%s: header census %s" % (name, widths)
        assert frames[0]["blocksize"] == 3000 and frames[-1]["blocksize"] == 1801, \
            "%s: blocksize expectation (%s)" % (
                name, [f["blocksize"] for f in frames])
        assert frames[0]["rate_hz"] == 48001, \
            "%s: sample rate %s" % (name, frames[0]["rate_hz"])
    else:
        assert all(48 <= f["header_bits"] <= 72 for f in frames), \
            "%s: expected the 48..=72-bit profile, census %s" % (name, widths)

    return dict(
        blocks=blocks, frame_data_offset=off, info=info, frames=frames,
        widths=widths, metaflac=minfo, forms_checked=len(frames),
        starts_strict=len(strict), starts_lax=len(lax),
        starts_fields=len(fields), starts_shape=shape,
        frame_bytes=walked, footer_gaps=gaps, blocksizes=blocksizes,
        header_bits=header_bits, size_codes=size_codes,
        footer_padding_frames=pad_frames,
        footer_slip_positions_sync_shape=slip_shape,
        footer_slip_positions_strict_headers=slip_headers,
        footer_early_positions=early_positions,
        footer_early_positions_sync_shape=early_shape,
        footer_early_positions_strict_headers=early_headers,
        derived_frame_count=derived, exact_frame_count=derivable,
        pcm=pcm_rows, mutation=mutation,
    )


# ---------------------------------------------------------------- emit

def emit(src, fixture_dir, out_path):
    os.makedirs(fixture_dir, exist_ok=True)
    encode_all(src)
    rows = []
    for spec in FIXTURES:
        path = os.path.join(src, spec["name"] + ".flac")
        with open(path, "rb") as handle:
            data = handle.read()
        checked = check_stream(spec, path, data)
        fixture = os.path.join(fixture_dir, spec["name"] + ".flac")
        with open(fixture, "wb") as handle:
            handle.write(data)
        rows.append((spec, data, checked))
        print("   checked + staged %s (%d bytes, blocks %s, headers %s bits)" % (
            spec["name"], len(data),
            ",".join(BLOCK_TYPE_NAMES[b["type"]] for b in checked["blocks"]),
            checked["widths"]))

    info = subprocess.run(["flac", "--version"], capture_output=True, text=True)
    version = (info.stdout.splitlines() or ["unknown"])[0].strip()
    lines = [
        "# flac-lite stream-layer (file-level metadata) golden vectors -- GENERATED",
        "# DO NOT EDIT BY HAND.  Regenerate with:  scripts/gen_stream_vectors.sh",
        "# Generator + independent container walk: scripts/stream_vectors.py",
        "# Frame rows: scripts/frame_vectors.py (find_frames / parse_frame_header)",
        "# Pins: metaflac --list (the reference tool's own block list + field values)",
        "# Encoder: %s" % version,
        "#",
        "# Each `stream_vector` block describes ONE committed fixture in",
        "# tests/fixtures/, byte-for-byte what libFLAC wrote (fixture_len and",
        "# fixture_fnv1a64 pin it -- the same hash the Rust suite reimplements).",
        "# The rows are what tests/stream_metadata_layout.rs asserts",
        "# flac_lite::stream::parse (-> StreamFacts) reports, and none of them comes",
        "# from that code: block types/flags/lengths and every streaminfo field",
        "# are metaflac's, cross-checked at generation time against an",
        "# independent bit-level reading of RFC 9639 section 8 in this file, and",
        "# the frame rows come from the proven strict frame filter (sync shape +",
        "# full field validation + verifying header CRC-8) whose header_bits is",
        "# the same arithmetic the crate's header parser must now reach.",
        "#",
        "# `frame_data_offset` is the byte the frames start at -- the number item",
        "# 4a exists to produce.  `first_frame_hex` is those bytes: enough of the",
        "# real stream to parse its header with the crate's own frame layer, at",
        "# the offset the stream layer reports.  Two layers, one byte offset, no",
        "# hand-built container anywhere (the hand-packed-vector rule, FLAC.md).",
        "#",
        "# `frame_number_forms_*` and `scan_frame_starts_*` are the two",
        "# measurements behind the frame-number strictness rule: every number",
        "# libFLAC wrote here is minimal (so refusing the others is free), and",
        "# the counts say what a scanner sees with and without that rule -- the",
        "# strict count is the frame count, counted by a second independent",
        "# reading of section 9.1 in this file. `mutation*` rows are one real",
        "# frame header of s1 rewritten with its number in each longer form,",
        "# both CRCs recomputed, with the real `flac -d` exit status for each:",
        "# (the frame footer's CRC-16 is measured per frame here to cover the",
        "# frame HEADER as well as the body -- crc16(header+body) equals the",
        "# stored footer, crc16(body) does not -- which is exactly why re-spelling",
        "# a number forces that CRC to be recomputed. An earlier claim in this",
        "# crate that the reference decoder REJECTED the longer form was that",
        "# artifact: a stale CRC, not a structural rule.)",
        "# the reference decoder ACCEPTS those forms (it just never writes",
        "# them), so our refusal is a policy divergence, recorded here rather",
        "# than argued.",
        "#",
        "# `scan_frame_starts_sync_shape_only` and `scan_frame_starts_fields_only`",
        "# complete that pair into a LADDER over the same region, measured with",
        "# the same second reading of section 9.1: sync shape alone (0xFF plus",
        "# the rest of the 14-bit sync — the test a player is tempted to call",
        "# \"find the next frame\"), then field validation without the CRC-8,",
        "# then full strict validation, then strict validation with the",
        "# minimal-form rule lifted. Generation refuses to write unless each",
        "# count contains the next and the strict count equals the frame count.",
        "# On THESE fixtures every leg gives the frame count: the regions are",
        "# 1-16 kB and a false sync-shaped offset costs about one per 8 kB of",
        "# material (0xFF x 1/32), so none is expected or found. The false",
        "# starts live in LONGER material — measured in scripts/frame_vectors.py's",
        "# `measure`, sync shape over-matches by 1.3x to 12x (204 candidates for",
        "# 157 frames, up to 1568 for 313), and it is the FIELD-VALIDATION leg",
        "# that removes nearly all of them (a CRC-8 check without field checks",
        "# still admits false starts — same docstring). That asymmetry is why",
        "# the crate's frame walk never guesses a boundary, and why the",
        "# constructed garbage/truncation cases in src/scan.rs' unit tests are",
        "# hand-cut regions rather than fixtures (the rule in tests/README.md).",
        "#",
        "# The frame-walk rows are item 4b's ground truth: what a reader that",
        "# indexes nothing but advances past frames must see. `frame_bytes` is",
        "# every frame's own length (header + bodies + padding + CRC-16), derived",
        "# by walking each frame's subfields bit by bit WITHOUT looking at the",
        "# next frame, and cross-checked against the distance to the next frame",
        "# start / end of file -- two derivations of the same number. Generation",
        "# refuses to write unless they agree on every frame and the lengths",
        "# total the region's byte length exactly: the frames tile the region",
        "# with no gap and no overlap, which is what makes `frame_bytes` a",
        "# reader's clock instead of an index. `frame_footer_gaps_bits` is the",
        "# measured padding+CRC-16 tail of each frame (16 when the last body bit",
        "# already lands on a byte boundary, otherwise pad-to-byte + 16). The",
        "# padding half is bounded by the format at 7 bits, and generation",
        "# asserts 16 <= gap <= 23 for every frame instead of trusting that.",
        "# `footer_padding_frames` counts how many of this fixture's frames",
        "# carry any padding at all, and the rows after it ask what a reader",
        "# that treated the footer as exactly 16 bits would find where it then",
        "# stands: `footer_slip_positions_*` are the positions its BIT cursor",
        "# would land on (short by the padding), `footer_byte_truncation_*` the",
        "# ones its BYTE cursor would land on if it rounded a non-byte-aligned",
        "# end down instead of up. On these fixtures the strict filter accepts",
        "# none of them - a footer mistake is refused, not silently followed -",
        "# but some of the bit-level positions are sync-SHAPED, which is the",
        "# case worth knowing: shape-matching and field-refusing is exactly",
        "# where a player that trusted the two-byte pattern would invent a",
        "# frame. These are rows rather than prose because the answers are",
        "# fixture facts: a future fixture that changed one would fail the Rust",
        "# test that reads them.",
        "# `frame_header_bits` is the same frame's header width, per frame, from",
        "# the independent parser. The three rows add up: a reader holding any two",
        "# can reconstruct the third part (a frame's subframe-body length is",
        "# bytes*8 - header_bits - gap). Framing is compositional, so a decoder",
        "# that disagrees about one of them disagrees about where the next frame",
        "# begins - and a per-frame width is a cross-implementation check, not a",
        "# restatement of the census rows above.",
        "# `frame_blocksizes` is the per-frame sample count, whose sum is",
        "# total_samples where the stream declares one -- and where it does not",
        "# (the unseekable fixture) is still what the reference decoder returns.",
        "# `frame_exact_count_from_declarations` is the frame count a reader",
        "# can derive from the stream's OWN declarations -- a fixed block size",
        "# plus a known sample total -- and nothing else: ceil(total / block)",
        "# with the short tail included. It equals the frame count on the four",
        "# seekable fixtures and `none` on the unseekable one, which is the",
        "# measured form of the point item 4b exists for: three of the five",
        "# streams here state where their audio ends and one states nothing.",
        "# `pcm_interleaved_values`, `pcm_fnv1a64` and `pcm_md5` come from",
        "# `flac -d`'s output, not from this crate: the whole-file decode the",
        "# frame walk produces has a pin, and `pcm_md5` equaling",
        "# `streaminfo_md5` is the format's own definition of that field (the",
        "# MD5 of the unencoded PCM) measured on real files. No PCM rows for the",
        "# 24-bit fixture: this crate's decoder stops at the depth gate, though",
        "# its frames are walked and cross-checked like the others.",
        "# `frame_sample_size_codes` is each frame's 9.1.5 Table 17 sample-size",
        "# code: 0 = \"the depth is only in STREAMINFO\" (the crate resolves that",
        "# through StreamDefaults), 1 = 8, 2 = 12, 3 = reserved, 4 = 16, 5 = 20,",
        "# 6 = 24, 7 = 32. Every frame of every fixture here STATES its depth,",
        "# and generation refuses to write a frame whose statement contradicts",
        "# its own streaminfo. That is what makes the 24-bit fixture's refusal",
        "# unarguable: no caller-supplied default could talk a decoder into",
        "# reading those frames, because the frames themselves say 24.",
        "",
    ]
    for spec, data, checked in rows:
        info, blocks, frames = checked["info"], checked["blocks"], checked["frames"]
        first = frames[0]
        lines += [
            "stream_vector %s" % spec["name"],
            "note %s" % spec["note"],
            "encode_command %s" % _command(spec),
            "fixture %s.flac" % spec["name"],
            "fixture_len %d" % len(data),
            "fixture_fnv1a64 %s" % fv.fnv1a64(data),
            "encoder_vendor %s" % checked["metaflac"].get("vendor", "unknown"),
            "block_count %d" % len(blocks),
        ]
        for n, b in enumerate(blocks):
            lines.append("block %d type %d(%s) last %s length %d" % (
                n, b["type"], BLOCK_TYPE_NAMES.get(b["type"], "RESERVED"),
                "true" if b["is_last"] else "false", b["length"]))
        lines += [
            "frame_data_offset %d" % checked["frame_data_offset"],
            "streaminfo_minimum_blocksize %d" % info["minimum_blocksize"],
            "streaminfo_maximum_blocksize %d" % info["maximum_blocksize"],
            "streaminfo_minimum_framesize %d" % info["minimum_framesize"],
            "streaminfo_maximum_framesize %d" % info["maximum_framesize"],
            "streaminfo_sample_rate_hz %d" % info["sample_rate_hz"],
            "streaminfo_channels %d" % (info["channels_minus_one"] + 1),
            "streaminfo_bits_per_sample %d" % (info["bits_per_sample_minus_one"] + 1),
            "streaminfo_total_samples %d" % info["total_samples"],
            "streaminfo_md5 %s" % info["md5"],
            "frame_count %d" % len(frames),
            # What the strictness of the frame-number form costs and what it
            # buys, measured on these bytes: `frame_number_forms_*` says every
            # number libFLAC wrote here is minimal (so refusing the rest is
            # free); `scan_frame_starts_*` counts the offsets in the frame
            # region where a header parse starts, with and without the
            # minimal-form rule. The strict count is by construction the frame
            # count (generation asserts the independent 9.1 reader lands on
            # exactly the frames the proven filter finds).
            "frame_number_forms_checked %d" % checked["forms_checked"],
            "frame_number_forms_all_minimal true",
            "scan_frame_starts_strict %d" % checked["starts_strict"],
            "scan_frame_starts_all_forms_allowed %d" % checked["starts_lax"],
            "frame_header_widths_bits %s" % " ".join(str(w) for w in checked["widths"]),
            "max_frame_header_width_bits %d" % max(f["header_bits"] for f in frames),
            "first_frame_hex %s" % " ".join(
                "%02X" % b for b in
                data[checked["frame_data_offset"]:
                     checked["frame_data_offset"] + first["header_bits"] // 8]),
            "first_frame_header_bits %d" % first["header_bits"],
            "first_frame_number %d" % first["number"],
            "first_frame_blocksize %d" % first["blocksize"],
            "first_frame_sample_rate_hz %s" % first["rate_hz"],
            "first_frame_bits_per_sample %d" % first["bps"],
            "first_frame_subframes %d" % first["subframes"],
            "first_frame_decorrelation %s" % first["decorrelation"],
            # The LAST frame too: it is the one that may be shorter than
            # streaminfo's max_blocksize, it is the frame whose end is the
            # region's end, and its number is frame_count-1. A table with only
            # the first frame cannot witness any of that.
            "last_frame_offset %d" % frames[-1]["offset"],
            "last_frame_number %d" % frames[-1]["number"],
            "last_frame_header_bits %d" % frames[-1]["header_bits"],
            "last_frame_blocksize %d" % frames[-1]["blocksize"],
            "last_frame_bytes_to_region_end %d" % (len(data) - frames[-1]["offset"]),
            # ---- item 4b: what a frame walk must see, per frame, in order.
            # The rows above describe what the stream DECLARES; these describe
            # what a reader with no index encounters, and none of them comes
            # from the crate either (walk_frame_bytes walks each frame's own
            # subfields; the strict filter's offsets and the reference decode's
            # PCM are the reference tool's).
            "frame_region_len %d" % (len(data) - checked["frame_data_offset"]),
            "frame_offsets %s" % " ".join(str(f["offset"]) for f in frames),
            "frame_bytes %s" % " ".join(str(n) for n in checked["frame_bytes"]),
            "frame_header_bits %s" % " ".join(str(n)
                                              for n in checked["header_bits"]),
            "frame_blocksizes %s" % " ".join(str(n)
                                             for n in checked["blocksizes"]),
            "frame_footer_gaps_bits %s" % " ".join(str(g)
                                                   for g in checked["footer_gaps"]),
            "footer_padding_frames %d" % checked["footer_padding_frames"],
            "footer_slip_positions_sync_shape %d" % checked[
                "footer_slip_positions_sync_shape"],
            "footer_slip_positions_strict_headers %d" % checked[
                "footer_slip_positions_strict_headers"],
            "footer_byte_truncation_positions %d" % checked[
                "footer_early_positions"],
            "footer_byte_truncation_sync_shape %d" % checked[
                "footer_early_positions_sync_shape"],
            "footer_byte_truncation_strict_headers %d" % checked[
                "footer_early_positions_strict_headers"],
            "frame_numbers %s" % " ".join(str(f["number"]) for f in frames),
            # §9.1.5 Table 17 sample-size codes, per frame: 0b000 = "the depth
            # is only in STREAMINFO", 0b001 = 8, 0b010 = 12, 0b011 = reserved,
            # 0b100 = 16, 0b101 = 20, 0b110 = 24, 0b111 = 32. Generation
            # refuses to write if a frame states a depth its streaminfo
            # contradicts.
            "frame_sample_size_codes %s" % " ".join(str(c) for c in
                                                    checked["size_codes"]),
            "frame_exact_count_from_declarations %s" % (
                "none" if checked["derived_frame_count"] is None
                else checked["derived_frame_count"]),
            "scan_frame_starts_sync_shape_only %d" % checked["starts_shape"],
            "scan_frame_starts_fields_only %d" % checked["starts_fields"],
        ]
        if checked["pcm"]:
            pcm = checked["pcm"]
            lines += [
                "pcm_interleaved_values %d" % pcm["values"],
                "pcm_fnv1a64 %s" % pcm["pcm_fnv"],
                "pcm_md5 %s" % pcm["md5"],
                "pcm_bytes_len %d" % len(pcm["bytes"]),
            ]
        lines.append("")
        if checked["mutation"]:
            m = checked["mutation"]
            lines += [
                # One real frame header of THIS stream, rewritten with its
                # number in each longer form, with the header CRC-8 AND the
                # frame CRC-16 recomputed (see mutate_number_form). Each
                # `..._reference_exit` is the real `flac -d` status on the
                # mutated stream — decoding and MD5-verifying, flac's default.
                "mutation number-form-overlong",
                "mutation_at %d" % m["at"],
                "mutation_before_hex %s" % m["before_hex"],
                "mutation_forms 2 3 4",
            ]
            for octets in sorted(m["forms"]):
                form = m["forms"][octets]
                lines += [
                    "mutation_form%d_header_hex %s" % (octets,
                                                       form["header_hex"]),
                    "mutation_form%d_reference_exit %d" % (octets,
                                                           form["reference_exit"]),
                ]
            lines += [
                "reference_decoder_accepts_nonminimal_forms true",
                "",
            ]
    with open(out_path, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines).rstrip() + "\n")


def _command(spec):
    """The exact command, in a copy-pasteable form. Raw-pipe encodes show the
    pipe because the pipe is what produces the unknown streaminfo values."""
    argv = [a.format(raw=spec.get("raw", ""), wav=spec.get("wav", ""))
            for a in spec["encode"]]
    out = "%s.flac" % spec["name"]
    if spec.get("raw_pipe"):
        return "cat %s | %s > %s" % (spec["raw"], " ".join(argv), out)
    return "%s -o %s" % (" ".join(argv), out)





def main(argv):
    if len(argv) >= 2 and argv[1] == "synth":
        synth(argv[2])
        return
    if len(argv) >= 4 and argv[1] == "emit":
        emit(argv[2], argv[3], argv[4])
        return
    print(__doc__)
    raise SystemExit(2)


if __name__ == "__main__":
    main(sys.argv)
