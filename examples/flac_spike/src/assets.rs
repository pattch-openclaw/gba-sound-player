//! flac_spike embedded clips — GENERATED
//! DO NOT EDIT BY HAND.  Regenerate:  scripts/gen_spike_assets.sh
//! Generator: scripts/frame_vectors.py spike_assets (strict frame finder,
//! fail-closed arm censuses; FLAC.md 'Perf gate spike' validation rules).
//! Encoder: flac 1.5.0
//!
//! sha256 pins (regeneration tripwires; the witness tests carry the
//! FNV-1a pins in code, these are for humans reviewing a regen diff):
//!   spike_l0_frames.bin      region 9304b41846d09f0cbed8acd04ab1583d04bb2aafdd16d861689d7c3e308f99ec
//!   spike_l0_pcm.bin         pcm    2a1226434f09b2de4e51db5953b039a0f3250e190c143cee877676edbbaaf8c1
//!   census(l0_stereo.flac): slot 0: fixed0 x160 | slot 1: fixed0 x160
//!   spike_l4_frames.bin      region 3a9cab983f43893ce5a42e19712ff5df1d286ac1668aa53cddd157a80f7cce7e
//!   spike_l4_pcm.bin         pcm    2a1226434f09b2de4e51db5953b039a0f3250e190c143cee877676edbbaaf8c1
//!   census(l4_stereo.flac): slot 0: lpc3 x1, lpc4 x159 | slot 1: lpc3 x1, lpc4 x159
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
    census: "slot 0: fixed0 x160 | slot 1: fixed0 x160",
    sample_rate_hz: 32768,
    bits_per_sample: 16,
    channels: 2,
    total_samples: 326144,
    max_blocksize: 2048,
    frames: &[
        FrameMeta {
            offset: 0,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 5541,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 11080,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 16622,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 22159,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 27701,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 33239,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 38781,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 44319,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 49860,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 55401,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 60940,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 66482,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 72020,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 77562,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 83099,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 88641,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 94178,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 99720,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 105260,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 110800,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 116340,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 121878,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 127419,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 132956,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 138497,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 144036,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 149577,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 155116,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 160656,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 166196,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 171735,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 177276,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 182814,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 188355,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 193894,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 199433,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 204973,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 210513,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 216053,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 221592,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 227133,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 232672,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 238214,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 243754,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 249294,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 254835,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 260373,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 265914,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 271454,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 276995,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 282533,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 288074,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 293614,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 299154,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 304694,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 310233,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 315773,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 321312,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 326852,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 332391,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 337932,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 343471,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 349010,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 354552,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 360091,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 365632,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 371172,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 376713,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 382252,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 387791,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 393331,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 398870,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 404410,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 409949,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 415490,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 421030,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 426570,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 432110,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 437651,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 443191,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 448730,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 454270,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 459810,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 465352,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 470892,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 476434,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 481973,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 487513,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 493054,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 498593,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 504134,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 509673,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 515213,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 520753,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 526294,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 531834,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 537375,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 542915,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 548454,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 553993,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 559532,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 565073,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 570613,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 576154,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 581693,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 587233,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 592772,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 598312,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 603851,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 609391,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 614930,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 620470,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 626012,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 631553,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 637094,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 642632,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 648172,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 653710,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 659251,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 664791,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 670332,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 675872,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 681412,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 686952,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 692492,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 698033,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 703573,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 709114,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 714653,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 720195,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 725735,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 731275,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 736816,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 742357,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 747898,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 753438,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 758980,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 764521,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 770064,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 775605,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 781147,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 786687,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 792229,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 797771,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 803311,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 808853,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 814392,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 819935,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 825474,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 831017,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 836557,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 842098,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 847639,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 853179,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 858720,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 864259,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 869802,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 875341,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 880884,
            blocksize: 512,
        },
    ],
    region: include_bytes!("../assets/spike_l0_frames.bin"),
    pcm_file: "spike_l0_pcm.bin",
    fnv_frames: 0x840C8013B5C4FED2,
    fnv_pcm: 0x054DD8B24062C27D,
};

/// LPC arm (flac -1 -l 4 -b 2048 -m): the gate's full-profile cost (majority LPC — FLAC.md Correction 3)
pub const L4_LPC: SpikeClip = SpikeClip {
    name: "l4_stereo",
    why: "LPC arm (flac -1 -l 4 -b 2048 -m): the gate's full-profile cost (majority LPC — FLAC.md Correction 3)",
    census: "slot 0: lpc3 x1, lpc4 x159 | slot 1: lpc3 x1, lpc4 x159",
    sample_rate_hz: 32768,
    bits_per_sample: 16,
    channels: 2,
    total_samples: 326144,
    max_blocksize: 2048,
    frames: &[
        FrameMeta {
            offset: 0,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 5348,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 10696,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 16048,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 21395,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 26750,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 32100,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 37453,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 42805,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 48158,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 53505,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 58854,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 64207,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 69557,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 74908,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 80257,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 85611,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 90964,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 96316,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 101665,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 107017,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 112368,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 117720,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 123072,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 128422,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 133772,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 139125,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 144479,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 149827,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 155180,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 160532,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 165885,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 171237,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 176586,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 181940,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 187292,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 192645,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 197997,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 203349,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 208698,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 214053,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 219406,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 224754,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 230105,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 235456,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 240808,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 246159,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 251512,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 256862,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 262213,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 267566,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 272917,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 278268,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 283619,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 288971,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 294322,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 299671,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 305022,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 310376,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 315728,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 321077,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 326427,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 331779,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 337128,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 342484,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 347835,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 353185,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 358537,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 363888,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 369236,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 374587,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 379940,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 385292,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 390646,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 395993,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 401346,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 406698,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 412049,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 417399,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 422749,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 428098,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 433448,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 438804,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 444152,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 449505,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 454855,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 460209,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 465560,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 470914,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 476268,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 481617,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 486971,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 492321,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 497672,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 503023,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 508378,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 513729,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 519081,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 524432,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 529782,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 535134,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 540483,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 545835,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 551184,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 556538,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 561891,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 567246,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 572597,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 577949,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 583303,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 588651,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 594002,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 599354,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 604708,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 610055,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 615410,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 620762,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 626114,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 631465,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 636816,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 642166,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 647516,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 652870,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 658220,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 663570,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 668920,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 674275,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 679623,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 684975,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 690327,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 695681,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 701032,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 706383,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 711736,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 717085,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 722438,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 727790,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 733144,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 738492,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 743847,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 749201,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 754552,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 759904,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 765257,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 770609,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 775957,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 781315,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 786665,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 792016,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 797370,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 802724,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 808072,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 813427,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 818782,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 824133,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 829486,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 834836,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 840188,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 845539,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 850891,
            blocksize: 512,
        },
    ],
    region: include_bytes!("../assets/spike_l4_frames.bin"),
    pcm_file: "spike_l4_pcm.bin",
    fnv_frames: 0xB19C800E7FCAA5BB,
    fnv_pcm: 0x054DD8B24062C27D,
};

/// Both arms, in gate order (FIXED first — the arm a decision defaults to).
pub const CLIPS: [&SpikeClip; 2] = [&L0_FIXED, &L4_LPC];
