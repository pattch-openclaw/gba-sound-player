//! flac_spike embedded clips — GENERATED
//! DO NOT EDIT BY HAND.  Regenerate:  scripts/gen_spike_assets.sh
//! Generator: scripts/frame_vectors.py spike_assets (strict frame finder,
//! fail-closed arm censuses; FLAC.md 'Perf gate spike' validation rules).
//! Encoder: flac 1.5.0
//!
//! sha256 pins (regeneration tripwires; the witness tests carry the
//! FNV-1a pins in code, these are for humans reviewing a regen diff):
//!   spike_l0_frames.bin      region 3bcf47a6dca1c53867d017f27dc97b4e6bf71a70c9e9d5407f5f987a484c7f21
//!   spike_l0_pcm.bin         pcm    5011e75804b18aaad79b0242d5c879535ee7e069c95a58f16eba61e8ce7d8b78
//!   census(l0_stereo.flac): slot 0: fixed0 x157 | slot 1: fixed0 x157
//!   spike_l4_frames.bin      region 6609602ba18860e6562c86594b6b56c4e434749623f731114e8a602444367cc5
//!   spike_l4_pcm.bin         pcm    5011e75804b18aaad79b0242d5c879535ee7e069c95a58f16eba61e8ce7d8b78
//!   census(l4_stereo.flac): slot 0: lpc3 x1, lpc4 x156 | slot 1: lpc3 x1, lpc4 x156
//!
//! Offsets are BYTE OFFSETS WITHIN THE EMBEDDED REGION (frame 0 at 0),
//! the spike's deliberate stand-in for the GAFP manifest's seek table.
//!
//! fnv_* are FNV-1a 64-bit: fnv_frames over the region bytes, fnv_pcm
//! over the reference PCM bytes (16-bit LE interleaved L,R). Witness
//! layers: the host test recomputes both in Rust and decodes the region
//! bit-exactly into the PCM; PR 2's ROM recomputes fnv_pcm on hardware.

/// One frame's placement in the region: byte offset and sample count
/// (the final frame of a track is legitimately short — FLAC.md).
/// `Clone + Copy`: two plain ints; the witness tests (and PR 4's
/// buffer rotation) build mutated copies of seek-table slices.
#[derive(Clone, Copy)]
pub struct FrameMeta {
    /// Byte offset of the frame header within the embedded region.
    pub offset: u32,
    /// Samples per subframe for THIS frame. Per-frame truth, not a
    /// track-wide constant: the tail frame is short.
    pub blocksize: u16,
}

/// One embedded arm: metadata, the seek table, the `include_bytes!`
/// region, and the hash pins. The reference PCM lives only in the
/// `assets/*_pcm.bin` files (host-test ground truth) — the ROM image
/// never embeds reference data, it pins it by hash.
pub struct SpikeClip {
    /// Arm label for logs.
    pub name: &'static str,
    /// What this arm measures (generated; logged at ROM boot).
    pub why: &'static str,
    /// Measured subframe census (generator refuses a broken census).
    pub census: &'static str,
    /// Stream sample rate in Hz (the profile pins one rate per clip).
    pub sample_rate_hz: u32,
    /// Stream bit depth (the profile's 16-bit).
    pub bits_per_sample: u8,
    /// Channel count (2 for both arms).
    pub channels: u8,
    /// Total samples per channel across all frames.
    pub total_samples: u32,
    /// Largest per-frame blocksize: the playback buffer size per channel.
    pub max_blocksize: u16,
    /// The seek table: frame 0 at offset 0, ascending, tiling exactly.
    pub frames: &'static [FrameMeta],
    /// The raw frame region — what `include_bytes!` embeds in the ROM.
    pub region: &'static [u8],
    /// File name (under `assets/`) of the reference PCM blob —
    /// host-test ground truth only, never embedded by the ROM.
    pub pcm_file: &'static str,
    /// FNV-1a 64 of `region`.
    pub fnv_frames: u64,
    /// FNV-1a 64 of the reference PCM bytes (NOT embedded).
    pub fnv_pcm: u64,
}

/// FIXED-only arm (flac -1 -l 0 -b 2048 -m): the gate's conservative predictor cost
pub const L0_FIXED: SpikeClip = SpikeClip {
    name: "l0_stereo",
    why: "FIXED-only arm (flac -1 -l 0 -b 2048 -m): the gate's conservative predictor cost",
    census: "slot 0: fixed0 x157 | slot 1: fixed0 x157",
    sample_rate_hz: 32000,
    bits_per_sample: 16,
    channels: 2,
    total_samples: 320000,
    max_blocksize: 2048,
    frames: &[
        FrameMeta {
            offset: 0,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 5538,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 11074,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 16613,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 22148,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 27689,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 33225,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 38765,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 44301,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 49840,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 55380,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 60917,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 66456,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 71991,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 77531,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 83066,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 88606,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 94142,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 99680,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 105217,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 110754,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 116293,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 121829,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 127368,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 132904,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 138443,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 143980,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 149518,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 155055,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 160592,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 166130,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 171667,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 177206,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 182742,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 188281,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 193818,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 199356,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 204894,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 210432,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 215970,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 221508,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 227047,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 232584,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 238123,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 243661,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 249200,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 254740,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 260278,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 265816,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 271354,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 276893,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 282431,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 287969,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 293508,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 299045,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 304584,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 310122,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 315660,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 321198,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 326736,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 332274,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 337813,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 343351,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 348888,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 354427,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 359964,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 365502,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 371039,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 376576,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 382114,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 387652,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 393190,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 398727,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 404266,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 409803,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 415342,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 420881,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 426420,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 431957,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 437496,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 443034,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 448571,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 454111,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 459648,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 465186,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 470724,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 476263,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 481801,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 487339,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 492878,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 498416,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 503955,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 509492,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 515031,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 520569,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 526108,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 531645,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 537184,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 542722,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 548260,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 553797,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 559335,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 564873,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 570411,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 575949,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 581486,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 587024,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 592562,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 598101,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 603637,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 609174,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 614713,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 620251,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 625789,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 631326,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 636866,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 642403,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 647942,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 653478,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 659017,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 664553,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 670093,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 675631,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 681168,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 686705,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 692242,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 697781,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 703319,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 708858,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 714395,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 719935,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 725473,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 731012,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 736551,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 742090,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 747630,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 753169,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 758710,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 764247,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 769787,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 775325,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 780865,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 786404,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 791943,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 797481,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 803018,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 808558,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 814096,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 819637,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 825175,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 830715,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 836253,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 841793,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 847332,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 852869,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 858408,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 863944,
            blocksize: 512,
        },
    ],
    region: include_bytes!("../assets/spike_l0_frames.bin"),
    pcm_file: "spike_l0_pcm.bin",
    fnv_frames: 0x5A0096689BE6EDCE,
    fnv_pcm: 0x54C7B356621B6E15,
};

/// LPC arm (flac -1 -l 4 -b 2048 -m): the gate's full-profile cost (majority LPC — FLAC.md Correction 3)
pub const L4_LPC: SpikeClip = SpikeClip {
    name: "l4_stereo",
    why: "LPC arm (flac -1 -l 4 -b 2048 -m): the gate's full-profile cost (majority LPC — FLAC.md Correction 3)",
    census: "slot 0: lpc3 x1, lpc4 x156 | slot 1: lpc3 x1, lpc4 x156",
    sample_rate_hz: 32000,
    bits_per_sample: 16,
    channels: 2,
    total_samples: 320000,
    max_blocksize: 2048,
    frames: &[
        FrameMeta {
            offset: 0,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 5345,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 10691,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 16041,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 21387,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 26739,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 32087,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 37438,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 42791,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 48142,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 53486,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 58832,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 64182,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 69531,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 74882,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 80228,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 85579,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 90929,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 96281,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 101630,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 106981,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 112331,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 117679,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 123029,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 128376,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 133726,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 139076,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 144427,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 149774,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 155125,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 160475,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 165824,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 171176,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 176523,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 181871,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 187219,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 192571,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 197919,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 203270,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 208615,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 213964,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 219315,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 224662,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 230010,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 235360,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 240709,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 246058,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 251408,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 256755,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 262106,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 267456,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 272806,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 278155,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 283503,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 288854,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 294203,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 299554,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 304904,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 310258,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 315606,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 320953,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 326303,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 331654,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 337001,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 342354,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 347701,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 353050,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 358400,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 363753,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 369100,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 374448,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 379798,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 385146,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 390499,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 395845,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 401195,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 406544,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 411896,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 417245,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 422593,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 427941,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 433290,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 438643,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 443988,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 449340,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 454688,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 460039,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 465388,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 470738,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 476087,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 481432,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 486785,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 492131,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 497482,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 502830,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 508183,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 513532,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 518882,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 524231,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 529577,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 534928,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 540275,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 545626,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 550972,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 556326,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 561677,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 567031,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 572380,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 577730,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 583079,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 588424,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 593773,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 599122,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 604474,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 609820,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 615174,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 620522,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 625872,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 631222,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 636573,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 641922,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 647270,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 652622,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 657971,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 663321,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 668668,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 674020,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 679368,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 684717,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 690066,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 695416,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 700765,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 706115,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 711466,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 716814,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 722166,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 727516,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 732868,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 738216,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 743567,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 748919,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 754269,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 759618,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 764969,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 770320,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 775666,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 781023,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 786372,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 791723,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 797074,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 802425,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 807771,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 813122,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 818472,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 823821,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 829174,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 834522,
            blocksize: 512,
        },
    ],
    region: include_bytes!("../assets/spike_l4_frames.bin"),
    pcm_file: "spike_l4_pcm.bin",
    fnv_frames: 0x5EE2546D35776FD6,
    fnv_pcm: 0x54C7B356621B6E15,
};

/// Both arms, in gate order (FIXED first — the arm a decision defaults to).
pub const CLIPS: [&SpikeClip; 2] = [&L0_FIXED, &L4_LPC];
