//! opus_spike embedded clips — GENERATED
//! DO NOT EDIT BY HAND.  Regenerate:  scripts/gen_opus_assets.sh
//! Generator: scripts/opus_assets.py opus_assets (Ogg demux with CRC-32
//! verification, fail-closed TOC censuses; OPUS.md 'Measurement plan').
//! Encoder: ffmpeg version 9.0.1 Copyright (c) 2000-2026 the FFmpeg developers
//!
//! sha256 pins (regeneration tripwires; the witness tests carry the
//! FNV-1a pins in code, these are for humans reviewing a regen diff):
//!   opus_silk_packets.bin        region 76fa0525d6e57141792fecd6b40bb797df9e4dad05d6a622314583aa8c3c4aa5
//!   opus_silk_ref.bin            pcm    bf4adc61f9feb8056b7e42ff9379431698b4e3864eab9c58fa4427b81cc66263
//!   census(opus_silk): config 9 SILK-only WB 20 ms x501 packets (code 0, one frame each)
//!   opus_music_packets.bin       region 7e26c8d68d1593008d9d4e5ecd0c8c132e406e8d3c7ecfd54bb7a0330c96bfd6
//!   opus_music_ref.bin           pcm    4136341af329b928f2271b1a46aaa49c436095b43d9288f7a515c27ca9536872
//!   census(opus_music): config 31 CELT-only FB 20 ms x501 packets (code 0, one frame each)
//!
//! Offsets are BYTE OFFSETS WITHIN THE EMBEDDED PACKET REGION (packet 0
//! at 0) — the product manifest's (offset, length) index, OPUS.md.
//! Unlike the FLAC gate these packets are NOT independently decodable:
//! Opus decode is a state machine, so the walk is sequential per clip.
//!
//! fnv_* are FNV-1a 64-bit: fnv_packets over the region bytes, fnv_ref_pcm
//! over the reference PCM bytes (32-bit float LE mono, ffmpeg/libopus
//! pre-skip applied — host ground truth, never embedded), and
//! fnv_walk_fold over the VENDORED WALK's own folded output (i16 LE,
//! half-up grid, all walk_samples — the ROM decode proof's golden;
//! measured over the dump_walk output, i.e. the production seam).

/// One packet's placement in the region: byte offset and length.
/// `Clone + Copy`: two plain ints.
#[derive(Clone, Copy)]
pub struct PacketMeta {
    /// Byte offset of the packet within the embedded region.
    pub offset: u32,
    /// Packet length in bytes (TOC byte included).
    pub len: u16,
}

/// One embedded arm: metadata, the packet index, the `include_bytes!`
/// region, and the hash pins. The reference PCM lives only in the
/// `assets/*_ref.bin` files (host-test ground truth) — the ROM image
/// never embeds reference data.
/// `Clone + Copy`: every field is a plain int or a `'static` reference —
/// the negative-control tests rebuild a corrupt clip by struct-update.
#[derive(Clone, Copy)]
pub struct OpusClip {
    /// Arm label for logs.
    pub name: &'static str,
    /// What this arm measures (generated; logged at ROM boot).
    pub why: &'static str,
    /// Measured TOC census (generator refuses a broken census).
    pub census: &'static str,
    /// RFC 6716 TOC config the census proved for every packet.
    pub toc_config: u8,
    /// Decoder API sample rate in Hz (the profile pins one per arm).
    pub sample_rate_hz: u32,
    /// Samples per decoded frame at the API rate (20 ms pinned).
    pub frame_samples: u16,
    /// Channel count (1 for both arms — the mono profile).
    pub channels: u8,
    /// OpusHead pre-skip in samples at the API rate: the encoder
    /// delay the reference PCM has ALREADY removed. NOT necessarily
    /// the walk→reference alignment — see `align_shift` (measured
    /// 2026-10-02: the SILK arm aligns 3 samples earlier).
    pub pre_skip: u16,
    /// MEASURED walk→reference alignment in samples: the shift
    /// where the folded vendored walk meets the folded reference
    /// (generator `discover_alignment` over the dump_walk output;
    /// the witness re-derives it from the production walk and the
    /// two implementations must agree).
    pub align_shift: u16,
    /// Fold-grid mismatches at `align_shift` on the round-half-up
    /// grid (the measured equivalence class — NOT the vendored
    /// truncate): the witness asserts exactly this count and every
    /// residual ≤ 1 LSB. Measured 2026-10-09 on the strict
    /// (host-simd OFF) equivalence class: SILK 0; CELT 126 of
    /// 480,000 (soft-float drift crossing grid boundaries; the
    /// pre-patch-5 NEON host measured 127 — see OPUS.md).
    pub fold_mismatch: u32,
    /// Packet count.
    pub packets: usize,
    /// Total samples a full packet walk produces at the API rate
    /// (packets x frame_samples, before pre-skip alignment).
    pub walk_samples: u32,
    /// Reference PCM bytes (host ground truth; 32-bit float LE mono).
    /// The witness folds BOTH sides with the crate's shared
    /// `checksum::fold_f32_to_i16` (round half up) and compares on the
    /// i16 grid from `align_shift` — the measured equivalence class
    /// of float decoders (step-2 findings 2026-10-02: raw f32 is NOT
    /// bit-exact; per-arm `fold_mismatch` counts the residuals).
    pub ref_bytes_len: u32,
    /// FNV-1a 64 of `region`.
    pub fnv_packets: u64,
    /// FNV-1a 64 of the reference PCM bytes (f32-LE; NOT embedded).
    pub fnv_ref_pcm: u64,
    /// FNV-1a 64 of the VENDORED WALK's folded output (every
    /// walk sample folded through the shared round-half-up grid,
    /// i16 LE bytes; no alignment shift). The ROM decode proof's
    /// golden: the port is NOT bit-exact with the reference, so the
    /// ROM pins its own decoder's deterministic output, and the
    /// reference comparison stays the host witness's layer
    /// (OPUS.md 2026-10-03: the FLAC fnv_pcm pattern does not
    /// transfer to a lossy codec).
    pub fnv_walk_fold: u64,
    /// File name (under `assets/`) of the reference PCM blob.
    pub ref_file: &'static str,
    /// The packet index: packet 0 at offset 0, ascending, tiling exactly.
    pub index: &'static [PacketMeta],
    /// The raw packet region — what `include_bytes!` embeds in the ROM.
    pub region: &'static [u8],
}

const OPUS_SILK_INDEX: &[PacketMeta] = &[
    PacketMeta { offset: 0, len: 30 },
    PacketMeta {
        offset: 30,
        len: 32,
    },
    PacketMeta {
        offset: 62,
        len: 27,
    },
    PacketMeta {
        offset: 89,
        len: 27,
    },
    PacketMeta {
        offset: 116,
        len: 30,
    },
    PacketMeta {
        offset: 146,
        len: 28,
    },
    PacketMeta {
        offset: 174,
        len: 23,
    },
    PacketMeta {
        offset: 197,
        len: 23,
    },
    PacketMeta {
        offset: 220,
        len: 16,
    },
    PacketMeta {
        offset: 236,
        len: 22,
    },
    PacketMeta {
        offset: 258,
        len: 21,
    },
    PacketMeta {
        offset: 279,
        len: 27,
    },
    PacketMeta {
        offset: 306,
        len: 26,
    },
    PacketMeta {
        offset: 332,
        len: 25,
    },
    PacketMeta {
        offset: 357,
        len: 24,
    },
    PacketMeta {
        offset: 381,
        len: 26,
    },
    PacketMeta {
        offset: 407,
        len: 26,
    },
    PacketMeta {
        offset: 433,
        len: 25,
    },
    PacketMeta {
        offset: 458,
        len: 25,
    },
    PacketMeta {
        offset: 483,
        len: 26,
    },
    PacketMeta {
        offset: 509,
        len: 27,
    },
    PacketMeta {
        offset: 536,
        len: 26,
    },
    PacketMeta {
        offset: 562,
        len: 25,
    },
    PacketMeta {
        offset: 587,
        len: 26,
    },
    PacketMeta {
        offset: 613,
        len: 22,
    },
    PacketMeta {
        offset: 635,
        len: 26,
    },
    PacketMeta {
        offset: 661,
        len: 25,
    },
    PacketMeta {
        offset: 686,
        len: 26,
    },
    PacketMeta {
        offset: 712,
        len: 27,
    },
    PacketMeta {
        offset: 739,
        len: 28,
    },
    PacketMeta {
        offset: 767,
        len: 24,
    },
    PacketMeta {
        offset: 791,
        len: 24,
    },
    PacketMeta {
        offset: 815,
        len: 26,
    },
    PacketMeta {
        offset: 841,
        len: 25,
    },
    PacketMeta {
        offset: 866,
        len: 26,
    },
    PacketMeta {
        offset: 892,
        len: 27,
    },
    PacketMeta {
        offset: 919,
        len: 24,
    },
    PacketMeta {
        offset: 943,
        len: 24,
    },
    PacketMeta {
        offset: 967,
        len: 25,
    },
    PacketMeta {
        offset: 992,
        len: 24,
    },
    PacketMeta {
        offset: 1016,
        len: 28,
    },
    PacketMeta {
        offset: 1044,
        len: 26,
    },
    PacketMeta {
        offset: 1070,
        len: 26,
    },
    PacketMeta {
        offset: 1096,
        len: 25,
    },
    PacketMeta {
        offset: 1121,
        len: 28,
    },
    PacketMeta {
        offset: 1149,
        len: 27,
    },
    PacketMeta {
        offset: 1176,
        len: 25,
    },
    PacketMeta {
        offset: 1201,
        len: 24,
    },
    PacketMeta {
        offset: 1225,
        len: 28,
    },
    PacketMeta {
        offset: 1253,
        len: 27,
    },
    PacketMeta {
        offset: 1280,
        len: 27,
    },
    PacketMeta {
        offset: 1307,
        len: 25,
    },
    PacketMeta {
        offset: 1332,
        len: 25,
    },
    PacketMeta {
        offset: 1357,
        len: 25,
    },
    PacketMeta {
        offset: 1382,
        len: 25,
    },
    PacketMeta {
        offset: 1407,
        len: 25,
    },
    PacketMeta {
        offset: 1432,
        len: 24,
    },
    PacketMeta {
        offset: 1456,
        len: 25,
    },
    PacketMeta {
        offset: 1481,
        len: 24,
    },
    PacketMeta {
        offset: 1505,
        len: 22,
    },
    PacketMeta {
        offset: 1527,
        len: 25,
    },
    PacketMeta {
        offset: 1552,
        len: 24,
    },
    PacketMeta {
        offset: 1576,
        len: 26,
    },
    PacketMeta {
        offset: 1602,
        len: 25,
    },
    PacketMeta {
        offset: 1627,
        len: 26,
    },
    PacketMeta {
        offset: 1653,
        len: 25,
    },
    PacketMeta {
        offset: 1678,
        len: 25,
    },
    PacketMeta {
        offset: 1703,
        len: 28,
    },
    PacketMeta {
        offset: 1731,
        len: 25,
    },
    PacketMeta {
        offset: 1756,
        len: 25,
    },
    PacketMeta {
        offset: 1781,
        len: 27,
    },
    PacketMeta {
        offset: 1808,
        len: 25,
    },
    PacketMeta {
        offset: 1833,
        len: 26,
    },
    PacketMeta {
        offset: 1859,
        len: 24,
    },
    PacketMeta {
        offset: 1883,
        len: 26,
    },
    PacketMeta {
        offset: 1909,
        len: 27,
    },
    PacketMeta {
        offset: 1936,
        len: 26,
    },
    PacketMeta {
        offset: 1962,
        len: 26,
    },
    PacketMeta {
        offset: 1988,
        len: 25,
    },
    PacketMeta {
        offset: 2013,
        len: 25,
    },
    PacketMeta {
        offset: 2038,
        len: 27,
    },
    PacketMeta {
        offset: 2065,
        len: 25,
    },
    PacketMeta {
        offset: 2090,
        len: 26,
    },
    PacketMeta {
        offset: 2116,
        len: 26,
    },
    PacketMeta {
        offset: 2142,
        len: 27,
    },
    PacketMeta {
        offset: 2169,
        len: 26,
    },
    PacketMeta {
        offset: 2195,
        len: 26,
    },
    PacketMeta {
        offset: 2221,
        len: 24,
    },
    PacketMeta {
        offset: 2245,
        len: 25,
    },
    PacketMeta {
        offset: 2270,
        len: 25,
    },
    PacketMeta {
        offset: 2295,
        len: 25,
    },
    PacketMeta {
        offset: 2320,
        len: 24,
    },
    PacketMeta {
        offset: 2344,
        len: 25,
    },
    PacketMeta {
        offset: 2369,
        len: 25,
    },
    PacketMeta {
        offset: 2394,
        len: 25,
    },
    PacketMeta {
        offset: 2419,
        len: 25,
    },
    PacketMeta {
        offset: 2444,
        len: 24,
    },
    PacketMeta {
        offset: 2468,
        len: 24,
    },
    PacketMeta {
        offset: 2492,
        len: 25,
    },
    PacketMeta {
        offset: 2517,
        len: 27,
    },
    PacketMeta {
        offset: 2544,
        len: 25,
    },
    PacketMeta {
        offset: 2569,
        len: 24,
    },
    PacketMeta {
        offset: 2593,
        len: 25,
    },
    PacketMeta {
        offset: 2618,
        len: 23,
    },
    PacketMeta {
        offset: 2641,
        len: 25,
    },
    PacketMeta {
        offset: 2666,
        len: 27,
    },
    PacketMeta {
        offset: 2693,
        len: 26,
    },
    PacketMeta {
        offset: 2719,
        len: 25,
    },
    PacketMeta {
        offset: 2744,
        len: 24,
    },
    PacketMeta {
        offset: 2768,
        len: 25,
    },
    PacketMeta {
        offset: 2793,
        len: 25,
    },
    PacketMeta {
        offset: 2818,
        len: 25,
    },
    PacketMeta {
        offset: 2843,
        len: 27,
    },
    PacketMeta {
        offset: 2870,
        len: 29,
    },
    PacketMeta {
        offset: 2899,
        len: 26,
    },
    PacketMeta {
        offset: 2925,
        len: 23,
    },
    PacketMeta {
        offset: 2948,
        len: 26,
    },
    PacketMeta {
        offset: 2974,
        len: 23,
    },
    PacketMeta {
        offset: 2997,
        len: 28,
    },
    PacketMeta {
        offset: 3025,
        len: 27,
    },
    PacketMeta {
        offset: 3052,
        len: 24,
    },
    PacketMeta {
        offset: 3076,
        len: 24,
    },
    PacketMeta {
        offset: 3100,
        len: 23,
    },
    PacketMeta {
        offset: 3123,
        len: 27,
    },
    PacketMeta {
        offset: 3150,
        len: 27,
    },
    PacketMeta {
        offset: 3177,
        len: 26,
    },
    PacketMeta {
        offset: 3203,
        len: 26,
    },
    PacketMeta {
        offset: 3229,
        len: 25,
    },
    PacketMeta {
        offset: 3254,
        len: 24,
    },
    PacketMeta {
        offset: 3278,
        len: 27,
    },
    PacketMeta {
        offset: 3305,
        len: 27,
    },
    PacketMeta {
        offset: 3332,
        len: 21,
    },
    PacketMeta {
        offset: 3353,
        len: 26,
    },
    PacketMeta {
        offset: 3379,
        len: 28,
    },
    PacketMeta {
        offset: 3407,
        len: 25,
    },
    PacketMeta {
        offset: 3432,
        len: 24,
    },
    PacketMeta {
        offset: 3456,
        len: 25,
    },
    PacketMeta {
        offset: 3481,
        len: 26,
    },
    PacketMeta {
        offset: 3507,
        len: 25,
    },
    PacketMeta {
        offset: 3532,
        len: 23,
    },
    PacketMeta {
        offset: 3555,
        len: 27,
    },
    PacketMeta {
        offset: 3582,
        len: 26,
    },
    PacketMeta {
        offset: 3608,
        len: 26,
    },
    PacketMeta {
        offset: 3634,
        len: 26,
    },
    PacketMeta {
        offset: 3660,
        len: 27,
    },
    PacketMeta {
        offset: 3687,
        len: 25,
    },
    PacketMeta {
        offset: 3712,
        len: 25,
    },
    PacketMeta {
        offset: 3737,
        len: 22,
    },
    PacketMeta {
        offset: 3759,
        len: 25,
    },
    PacketMeta {
        offset: 3784,
        len: 25,
    },
    PacketMeta {
        offset: 3809,
        len: 24,
    },
    PacketMeta {
        offset: 3833,
        len: 22,
    },
    PacketMeta {
        offset: 3855,
        len: 23,
    },
    PacketMeta {
        offset: 3878,
        len: 26,
    },
    PacketMeta {
        offset: 3904,
        len: 26,
    },
    PacketMeta {
        offset: 3930,
        len: 26,
    },
    PacketMeta {
        offset: 3956,
        len: 26,
    },
    PacketMeta {
        offset: 3982,
        len: 25,
    },
    PacketMeta {
        offset: 4007,
        len: 26,
    },
    PacketMeta {
        offset: 4033,
        len: 26,
    },
    PacketMeta {
        offset: 4059,
        len: 28,
    },
    PacketMeta {
        offset: 4087,
        len: 23,
    },
    PacketMeta {
        offset: 4110,
        len: 25,
    },
    PacketMeta {
        offset: 4135,
        len: 25,
    },
    PacketMeta {
        offset: 4160,
        len: 24,
    },
    PacketMeta {
        offset: 4184,
        len: 25,
    },
    PacketMeta {
        offset: 4209,
        len: 25,
    },
    PacketMeta {
        offset: 4234,
        len: 25,
    },
    PacketMeta {
        offset: 4259,
        len: 26,
    },
    PacketMeta {
        offset: 4285,
        len: 25,
    },
    PacketMeta {
        offset: 4310,
        len: 26,
    },
    PacketMeta {
        offset: 4336,
        len: 26,
    },
    PacketMeta {
        offset: 4362,
        len: 28,
    },
    PacketMeta {
        offset: 4390,
        len: 27,
    },
    PacketMeta {
        offset: 4417,
        len: 26,
    },
    PacketMeta {
        offset: 4443,
        len: 25,
    },
    PacketMeta {
        offset: 4468,
        len: 23,
    },
    PacketMeta {
        offset: 4491,
        len: 24,
    },
    PacketMeta {
        offset: 4515,
        len: 28,
    },
    PacketMeta {
        offset: 4543,
        len: 24,
    },
    PacketMeta {
        offset: 4567,
        len: 22,
    },
    PacketMeta {
        offset: 4589,
        len: 27,
    },
    PacketMeta {
        offset: 4616,
        len: 25,
    },
    PacketMeta {
        offset: 4641,
        len: 25,
    },
    PacketMeta {
        offset: 4666,
        len: 24,
    },
    PacketMeta {
        offset: 4690,
        len: 25,
    },
    PacketMeta {
        offset: 4715,
        len: 24,
    },
    PacketMeta {
        offset: 4739,
        len: 26,
    },
    PacketMeta {
        offset: 4765,
        len: 26,
    },
    PacketMeta {
        offset: 4791,
        len: 23,
    },
    PacketMeta {
        offset: 4814,
        len: 25,
    },
    PacketMeta {
        offset: 4839,
        len: 27,
    },
    PacketMeta {
        offset: 4866,
        len: 22,
    },
    PacketMeta {
        offset: 4888,
        len: 23,
    },
    PacketMeta {
        offset: 4911,
        len: 28,
    },
    PacketMeta {
        offset: 4939,
        len: 25,
    },
    PacketMeta {
        offset: 4964,
        len: 23,
    },
    PacketMeta {
        offset: 4987,
        len: 27,
    },
    PacketMeta {
        offset: 5014,
        len: 25,
    },
    PacketMeta {
        offset: 5039,
        len: 25,
    },
    PacketMeta {
        offset: 5064,
        len: 25,
    },
    PacketMeta {
        offset: 5089,
        len: 24,
    },
    PacketMeta {
        offset: 5113,
        len: 24,
    },
    PacketMeta {
        offset: 5137,
        len: 26,
    },
    PacketMeta {
        offset: 5163,
        len: 25,
    },
    PacketMeta {
        offset: 5188,
        len: 26,
    },
    PacketMeta {
        offset: 5214,
        len: 29,
    },
    PacketMeta {
        offset: 5243,
        len: 24,
    },
    PacketMeta {
        offset: 5267,
        len: 27,
    },
    PacketMeta {
        offset: 5294,
        len: 24,
    },
    PacketMeta {
        offset: 5318,
        len: 26,
    },
    PacketMeta {
        offset: 5344,
        len: 25,
    },
    PacketMeta {
        offset: 5369,
        len: 24,
    },
    PacketMeta {
        offset: 5393,
        len: 26,
    },
    PacketMeta {
        offset: 5419,
        len: 26,
    },
    PacketMeta {
        offset: 5445,
        len: 25,
    },
    PacketMeta {
        offset: 5470,
        len: 27,
    },
    PacketMeta {
        offset: 5497,
        len: 26,
    },
    PacketMeta {
        offset: 5523,
        len: 27,
    },
    PacketMeta {
        offset: 5550,
        len: 22,
    },
    PacketMeta {
        offset: 5572,
        len: 25,
    },
    PacketMeta {
        offset: 5597,
        len: 23,
    },
    PacketMeta {
        offset: 5620,
        len: 26,
    },
    PacketMeta {
        offset: 5646,
        len: 25,
    },
    PacketMeta {
        offset: 5671,
        len: 26,
    },
    PacketMeta {
        offset: 5697,
        len: 25,
    },
    PacketMeta {
        offset: 5722,
        len: 27,
    },
    PacketMeta {
        offset: 5749,
        len: 25,
    },
    PacketMeta {
        offset: 5774,
        len: 27,
    },
    PacketMeta {
        offset: 5801,
        len: 25,
    },
    PacketMeta {
        offset: 5826,
        len: 22,
    },
    PacketMeta {
        offset: 5848,
        len: 27,
    },
    PacketMeta {
        offset: 5875,
        len: 27,
    },
    PacketMeta {
        offset: 5902,
        len: 24,
    },
    PacketMeta {
        offset: 5926,
        len: 26,
    },
    PacketMeta {
        offset: 5952,
        len: 27,
    },
    PacketMeta {
        offset: 5979,
        len: 25,
    },
    PacketMeta {
        offset: 6004,
        len: 25,
    },
    PacketMeta {
        offset: 6029,
        len: 24,
    },
    PacketMeta {
        offset: 6053,
        len: 24,
    },
    PacketMeta {
        offset: 6077,
        len: 24,
    },
    PacketMeta {
        offset: 6101,
        len: 27,
    },
    PacketMeta {
        offset: 6128,
        len: 26,
    },
    PacketMeta {
        offset: 6154,
        len: 24,
    },
    PacketMeta {
        offset: 6178,
        len: 25,
    },
    PacketMeta {
        offset: 6203,
        len: 26,
    },
    PacketMeta {
        offset: 6229,
        len: 25,
    },
    PacketMeta {
        offset: 6254,
        len: 24,
    },
    PacketMeta {
        offset: 6278,
        len: 22,
    },
    PacketMeta {
        offset: 6300,
        len: 26,
    },
    PacketMeta {
        offset: 6326,
        len: 24,
    },
    PacketMeta {
        offset: 6350,
        len: 26,
    },
    PacketMeta {
        offset: 6376,
        len: 26,
    },
    PacketMeta {
        offset: 6402,
        len: 24,
    },
    PacketMeta {
        offset: 6426,
        len: 25,
    },
    PacketMeta {
        offset: 6451,
        len: 26,
    },
    PacketMeta {
        offset: 6477,
        len: 26,
    },
    PacketMeta {
        offset: 6503,
        len: 25,
    },
    PacketMeta {
        offset: 6528,
        len: 25,
    },
    PacketMeta {
        offset: 6553,
        len: 25,
    },
    PacketMeta {
        offset: 6578,
        len: 24,
    },
    PacketMeta {
        offset: 6602,
        len: 27,
    },
    PacketMeta {
        offset: 6629,
        len: 27,
    },
    PacketMeta {
        offset: 6656,
        len: 27,
    },
    PacketMeta {
        offset: 6683,
        len: 26,
    },
    PacketMeta {
        offset: 6709,
        len: 25,
    },
    PacketMeta {
        offset: 6734,
        len: 27,
    },
    PacketMeta {
        offset: 6761,
        len: 25,
    },
    PacketMeta {
        offset: 6786,
        len: 26,
    },
    PacketMeta {
        offset: 6812,
        len: 25,
    },
    PacketMeta {
        offset: 6837,
        len: 25,
    },
    PacketMeta {
        offset: 6862,
        len: 26,
    },
    PacketMeta {
        offset: 6888,
        len: 25,
    },
    PacketMeta {
        offset: 6913,
        len: 25,
    },
    PacketMeta {
        offset: 6938,
        len: 26,
    },
    PacketMeta {
        offset: 6964,
        len: 27,
    },
    PacketMeta {
        offset: 6991,
        len: 22,
    },
    PacketMeta {
        offset: 7013,
        len: 26,
    },
    PacketMeta {
        offset: 7039,
        len: 26,
    },
    PacketMeta {
        offset: 7065,
        len: 26,
    },
    PacketMeta {
        offset: 7091,
        len: 27,
    },
    PacketMeta {
        offset: 7118,
        len: 26,
    },
    PacketMeta {
        offset: 7144,
        len: 25,
    },
    PacketMeta {
        offset: 7169,
        len: 26,
    },
    PacketMeta {
        offset: 7195,
        len: 23,
    },
    PacketMeta {
        offset: 7218,
        len: 24,
    },
    PacketMeta {
        offset: 7242,
        len: 24,
    },
    PacketMeta {
        offset: 7266,
        len: 23,
    },
    PacketMeta {
        offset: 7289,
        len: 26,
    },
    PacketMeta {
        offset: 7315,
        len: 27,
    },
    PacketMeta {
        offset: 7342,
        len: 28,
    },
    PacketMeta {
        offset: 7370,
        len: 24,
    },
    PacketMeta {
        offset: 7394,
        len: 20,
    },
    PacketMeta {
        offset: 7414,
        len: 25,
    },
    PacketMeta {
        offset: 7439,
        len: 25,
    },
    PacketMeta {
        offset: 7464,
        len: 24,
    },
    PacketMeta {
        offset: 7488,
        len: 25,
    },
    PacketMeta {
        offset: 7513,
        len: 27,
    },
    PacketMeta {
        offset: 7540,
        len: 25,
    },
    PacketMeta {
        offset: 7565,
        len: 25,
    },
    PacketMeta {
        offset: 7590,
        len: 24,
    },
    PacketMeta {
        offset: 7614,
        len: 26,
    },
    PacketMeta {
        offset: 7640,
        len: 27,
    },
    PacketMeta {
        offset: 7667,
        len: 25,
    },
    PacketMeta {
        offset: 7692,
        len: 26,
    },
    PacketMeta {
        offset: 7718,
        len: 24,
    },
    PacketMeta {
        offset: 7742,
        len: 25,
    },
    PacketMeta {
        offset: 7767,
        len: 24,
    },
    PacketMeta {
        offset: 7791,
        len: 25,
    },
    PacketMeta {
        offset: 7816,
        len: 24,
    },
    PacketMeta {
        offset: 7840,
        len: 26,
    },
    PacketMeta {
        offset: 7866,
        len: 24,
    },
    PacketMeta {
        offset: 7890,
        len: 25,
    },
    PacketMeta {
        offset: 7915,
        len: 25,
    },
    PacketMeta {
        offset: 7940,
        len: 27,
    },
    PacketMeta {
        offset: 7967,
        len: 27,
    },
    PacketMeta {
        offset: 7994,
        len: 23,
    },
    PacketMeta {
        offset: 8017,
        len: 23,
    },
    PacketMeta {
        offset: 8040,
        len: 26,
    },
    PacketMeta {
        offset: 8066,
        len: 27,
    },
    PacketMeta {
        offset: 8093,
        len: 24,
    },
    PacketMeta {
        offset: 8117,
        len: 26,
    },
    PacketMeta {
        offset: 8143,
        len: 26,
    },
    PacketMeta {
        offset: 8169,
        len: 22,
    },
    PacketMeta {
        offset: 8191,
        len: 27,
    },
    PacketMeta {
        offset: 8218,
        len: 28,
    },
    PacketMeta {
        offset: 8246,
        len: 26,
    },
    PacketMeta {
        offset: 8272,
        len: 27,
    },
    PacketMeta {
        offset: 8299,
        len: 25,
    },
    PacketMeta {
        offset: 8324,
        len: 24,
    },
    PacketMeta {
        offset: 8348,
        len: 26,
    },
    PacketMeta {
        offset: 8374,
        len: 24,
    },
    PacketMeta {
        offset: 8398,
        len: 27,
    },
    PacketMeta {
        offset: 8425,
        len: 28,
    },
    PacketMeta {
        offset: 8453,
        len: 28,
    },
    PacketMeta {
        offset: 8481,
        len: 26,
    },
    PacketMeta {
        offset: 8507,
        len: 27,
    },
    PacketMeta {
        offset: 8534,
        len: 27,
    },
    PacketMeta {
        offset: 8561,
        len: 25,
    },
    PacketMeta {
        offset: 8586,
        len: 27,
    },
    PacketMeta {
        offset: 8613,
        len: 26,
    },
    PacketMeta {
        offset: 8639,
        len: 29,
    },
    PacketMeta {
        offset: 8668,
        len: 24,
    },
    PacketMeta {
        offset: 8692,
        len: 27,
    },
    PacketMeta {
        offset: 8719,
        len: 26,
    },
    PacketMeta {
        offset: 8745,
        len: 26,
    },
    PacketMeta {
        offset: 8771,
        len: 27,
    },
    PacketMeta {
        offset: 8798,
        len: 26,
    },
    PacketMeta {
        offset: 8824,
        len: 26,
    },
    PacketMeta {
        offset: 8850,
        len: 24,
    },
    PacketMeta {
        offset: 8874,
        len: 26,
    },
    PacketMeta {
        offset: 8900,
        len: 28,
    },
    PacketMeta {
        offset: 8928,
        len: 23,
    },
    PacketMeta {
        offset: 8951,
        len: 28,
    },
    PacketMeta {
        offset: 8979,
        len: 26,
    },
    PacketMeta {
        offset: 9005,
        len: 24,
    },
    PacketMeta {
        offset: 9029,
        len: 25,
    },
    PacketMeta {
        offset: 9054,
        len: 24,
    },
    PacketMeta {
        offset: 9078,
        len: 23,
    },
    PacketMeta {
        offset: 9101,
        len: 25,
    },
    PacketMeta {
        offset: 9126,
        len: 25,
    },
    PacketMeta {
        offset: 9151,
        len: 27,
    },
    PacketMeta {
        offset: 9178,
        len: 26,
    },
    PacketMeta {
        offset: 9204,
        len: 27,
    },
    PacketMeta {
        offset: 9231,
        len: 25,
    },
    PacketMeta {
        offset: 9256,
        len: 23,
    },
    PacketMeta {
        offset: 9279,
        len: 25,
    },
    PacketMeta {
        offset: 9304,
        len: 26,
    },
    PacketMeta {
        offset: 9330,
        len: 26,
    },
    PacketMeta {
        offset: 9356,
        len: 25,
    },
    PacketMeta {
        offset: 9381,
        len: 25,
    },
    PacketMeta {
        offset: 9406,
        len: 24,
    },
    PacketMeta {
        offset: 9430,
        len: 27,
    },
    PacketMeta {
        offset: 9457,
        len: 25,
    },
    PacketMeta {
        offset: 9482,
        len: 25,
    },
    PacketMeta {
        offset: 9507,
        len: 26,
    },
    PacketMeta {
        offset: 9533,
        len: 25,
    },
    PacketMeta {
        offset: 9558,
        len: 26,
    },
    PacketMeta {
        offset: 9584,
        len: 27,
    },
    PacketMeta {
        offset: 9611,
        len: 24,
    },
    PacketMeta {
        offset: 9635,
        len: 26,
    },
    PacketMeta {
        offset: 9661,
        len: 27,
    },
    PacketMeta {
        offset: 9688,
        len: 25,
    },
    PacketMeta {
        offset: 9713,
        len: 26,
    },
    PacketMeta {
        offset: 9739,
        len: 24,
    },
    PacketMeta {
        offset: 9763,
        len: 29,
    },
    PacketMeta {
        offset: 9792,
        len: 25,
    },
    PacketMeta {
        offset: 9817,
        len: 25,
    },
    PacketMeta {
        offset: 9842,
        len: 28,
    },
    PacketMeta {
        offset: 9870,
        len: 25,
    },
    PacketMeta {
        offset: 9895,
        len: 25,
    },
    PacketMeta {
        offset: 9920,
        len: 25,
    },
    PacketMeta {
        offset: 9945,
        len: 24,
    },
    PacketMeta {
        offset: 9969,
        len: 25,
    },
    PacketMeta {
        offset: 9994,
        len: 25,
    },
    PacketMeta {
        offset: 10019,
        len: 26,
    },
    PacketMeta {
        offset: 10045,
        len: 24,
    },
    PacketMeta {
        offset: 10069,
        len: 27,
    },
    PacketMeta {
        offset: 10096,
        len: 24,
    },
    PacketMeta {
        offset: 10120,
        len: 27,
    },
    PacketMeta {
        offset: 10147,
        len: 25,
    },
    PacketMeta {
        offset: 10172,
        len: 26,
    },
    PacketMeta {
        offset: 10198,
        len: 27,
    },
    PacketMeta {
        offset: 10225,
        len: 24,
    },
    PacketMeta {
        offset: 10249,
        len: 24,
    },
    PacketMeta {
        offset: 10273,
        len: 27,
    },
    PacketMeta {
        offset: 10300,
        len: 24,
    },
    PacketMeta {
        offset: 10324,
        len: 29,
    },
    PacketMeta {
        offset: 10353,
        len: 26,
    },
    PacketMeta {
        offset: 10379,
        len: 24,
    },
    PacketMeta {
        offset: 10403,
        len: 23,
    },
    PacketMeta {
        offset: 10426,
        len: 25,
    },
    PacketMeta {
        offset: 10451,
        len: 25,
    },
    PacketMeta {
        offset: 10476,
        len: 24,
    },
    PacketMeta {
        offset: 10500,
        len: 26,
    },
    PacketMeta {
        offset: 10526,
        len: 23,
    },
    PacketMeta {
        offset: 10549,
        len: 25,
    },
    PacketMeta {
        offset: 10574,
        len: 27,
    },
    PacketMeta {
        offset: 10601,
        len: 25,
    },
    PacketMeta {
        offset: 10626,
        len: 23,
    },
    PacketMeta {
        offset: 10649,
        len: 27,
    },
    PacketMeta {
        offset: 10676,
        len: 26,
    },
    PacketMeta {
        offset: 10702,
        len: 25,
    },
    PacketMeta {
        offset: 10727,
        len: 25,
    },
    PacketMeta {
        offset: 10752,
        len: 26,
    },
    PacketMeta {
        offset: 10778,
        len: 23,
    },
    PacketMeta {
        offset: 10801,
        len: 27,
    },
    PacketMeta {
        offset: 10828,
        len: 24,
    },
    PacketMeta {
        offset: 10852,
        len: 24,
    },
    PacketMeta {
        offset: 10876,
        len: 24,
    },
    PacketMeta {
        offset: 10900,
        len: 25,
    },
    PacketMeta {
        offset: 10925,
        len: 25,
    },
    PacketMeta {
        offset: 10950,
        len: 26,
    },
    PacketMeta {
        offset: 10976,
        len: 27,
    },
    PacketMeta {
        offset: 11003,
        len: 26,
    },
    PacketMeta {
        offset: 11029,
        len: 25,
    },
    PacketMeta {
        offset: 11054,
        len: 25,
    },
    PacketMeta {
        offset: 11079,
        len: 23,
    },
    PacketMeta {
        offset: 11102,
        len: 25,
    },
    PacketMeta {
        offset: 11127,
        len: 26,
    },
    PacketMeta {
        offset: 11153,
        len: 24,
    },
    PacketMeta {
        offset: 11177,
        len: 24,
    },
    PacketMeta {
        offset: 11201,
        len: 26,
    },
    PacketMeta {
        offset: 11227,
        len: 26,
    },
    PacketMeta {
        offset: 11253,
        len: 24,
    },
    PacketMeta {
        offset: 11277,
        len: 26,
    },
    PacketMeta {
        offset: 11303,
        len: 24,
    },
    PacketMeta {
        offset: 11327,
        len: 24,
    },
    PacketMeta {
        offset: 11351,
        len: 24,
    },
    PacketMeta {
        offset: 11375,
        len: 26,
    },
    PacketMeta {
        offset: 11401,
        len: 23,
    },
    PacketMeta {
        offset: 11424,
        len: 25,
    },
    PacketMeta {
        offset: 11449,
        len: 27,
    },
    PacketMeta {
        offset: 11476,
        len: 23,
    },
    PacketMeta {
        offset: 11499,
        len: 24,
    },
    PacketMeta {
        offset: 11523,
        len: 24,
    },
    PacketMeta {
        offset: 11547,
        len: 24,
    },
    PacketMeta {
        offset: 11571,
        len: 24,
    },
    PacketMeta {
        offset: 11595,
        len: 25,
    },
    PacketMeta {
        offset: 11620,
        len: 26,
    },
    PacketMeta {
        offset: 11646,
        len: 22,
    },
    PacketMeta {
        offset: 11668,
        len: 27,
    },
    PacketMeta {
        offset: 11695,
        len: 23,
    },
    PacketMeta {
        offset: 11718,
        len: 24,
    },
    PacketMeta {
        offset: 11742,
        len: 26,
    },
    PacketMeta {
        offset: 11768,
        len: 29,
    },
    PacketMeta {
        offset: 11797,
        len: 25,
    },
    PacketMeta {
        offset: 11822,
        len: 27,
    },
    PacketMeta {
        offset: 11849,
        len: 28,
    },
    PacketMeta {
        offset: 11877,
        len: 24,
    },
    PacketMeta {
        offset: 11901,
        len: 26,
    },
    PacketMeta {
        offset: 11927,
        len: 26,
    },
    PacketMeta {
        offset: 11953,
        len: 26,
    },
    PacketMeta {
        offset: 11979,
        len: 24,
    },
    PacketMeta {
        offset: 12003,
        len: 26,
    },
    PacketMeta {
        offset: 12029,
        len: 24,
    },
    PacketMeta {
        offset: 12053,
        len: 25,
    },
    PacketMeta {
        offset: 12078,
        len: 25,
    },
    PacketMeta {
        offset: 12103,
        len: 25,
    },
    PacketMeta {
        offset: 12128,
        len: 26,
    },
    PacketMeta {
        offset: 12154,
        len: 26,
    },
    PacketMeta {
        offset: 12180,
        len: 26,
    },
    PacketMeta {
        offset: 12206,
        len: 26,
    },
    PacketMeta {
        offset: 12232,
        len: 23,
    },
    PacketMeta {
        offset: 12255,
        len: 25,
    },
    PacketMeta {
        offset: 12280,
        len: 25,
    },
    PacketMeta {
        offset: 12305,
        len: 28,
    },
    PacketMeta {
        offset: 12333,
        len: 25,
    },
    PacketMeta {
        offset: 12358,
        len: 26,
    },
    PacketMeta {
        offset: 12384,
        len: 27,
    },
    PacketMeta {
        offset: 12411,
        len: 23,
    },
    PacketMeta {
        offset: 12434,
        len: 23,
    },
    PacketMeta {
        offset: 12457,
        len: 24,
    },
    PacketMeta {
        offset: 12481,
        len: 28,
    },
    PacketMeta {
        offset: 12509,
        len: 25,
    },
    PacketMeta {
        offset: 12534,
        len: 26,
    },
    PacketMeta {
        offset: 12560,
        len: 26,
    },
    PacketMeta {
        offset: 12586,
        len: 25,
    },
    PacketMeta {
        offset: 12611,
        len: 25,
    },
    PacketMeta {
        offset: 12636,
        len: 26,
    },
    PacketMeta {
        offset: 12662,
        len: 21,
    },
];

/// opus_silk — config 9 SILK-only WB 20 ms x501 packets (code 0, one frame each).
pub const OPUS_SILK: OpusClip = OpusClip {
    name: "opus_silk",
    why: "SILK-only speech arm: libopus -application voip at the 48 kHz API rate (census records the mode/bandwidth it actually chose); the speech-first profile's decode cost incl. the internal resampler",
    census: "config 9 SILK-only WB 20 ms x501 packets (code 0, one frame each)",
    toc_config: 9,
    sample_rate_hz: 48000,
    frame_samples: 960,
    channels: 1,
    pre_skip: 312,
    align_shift: 309,
    fold_mismatch: 0,
    packets: 501,
    walk_samples: 480960,
    ref_bytes_len: 1920000,
    fnv_packets: 0x3f491c76528c74d2,
    fnv_ref_pcm: 0x71de2d4050d06f5e,
    fnv_walk_fold: 0x79e9dceaba485bee,
    ref_file: "opus_silk_ref.bin",
    index: OPUS_SILK_INDEX,
    region: include_bytes!("../assets/opus_silk_packets.bin"),
};

const OPUS_MUSIC_INDEX: &[PacketMeta] = &[
    PacketMeta {
        offset: 0,
        len: 432,
    },
    PacketMeta {
        offset: 432,
        len: 309,
    },
    PacketMeta {
        offset: 741,
        len: 293,
    },
    PacketMeta {
        offset: 1034,
        len: 276,
    },
    PacketMeta {
        offset: 1310,
        len: 303,
    },
    PacketMeta {
        offset: 1613,
        len: 286,
    },
    PacketMeta {
        offset: 1899,
        len: 298,
    },
    PacketMeta {
        offset: 2197,
        len: 295,
    },
    PacketMeta {
        offset: 2492,
        len: 297,
    },
    PacketMeta {
        offset: 2789,
        len: 305,
    },
    PacketMeta {
        offset: 3094,
        len: 300,
    },
    PacketMeta {
        offset: 3394,
        len: 304,
    },
    PacketMeta {
        offset: 3698,
        len: 295,
    },
    PacketMeta {
        offset: 3993,
        len: 298,
    },
    PacketMeta {
        offset: 4291,
        len: 312,
    },
    PacketMeta {
        offset: 4603,
        len: 309,
    },
    PacketMeta {
        offset: 4912,
        len: 309,
    },
    PacketMeta {
        offset: 5221,
        len: 312,
    },
    PacketMeta {
        offset: 5533,
        len: 312,
    },
    PacketMeta {
        offset: 5845,
        len: 307,
    },
    PacketMeta {
        offset: 6152,
        len: 311,
    },
    PacketMeta {
        offset: 6463,
        len: 306,
    },
    PacketMeta {
        offset: 6769,
        len: 306,
    },
    PacketMeta {
        offset: 7075,
        len: 308,
    },
    PacketMeta {
        offset: 7383,
        len: 306,
    },
    PacketMeta {
        offset: 7689,
        len: 311,
    },
    PacketMeta {
        offset: 8000,
        len: 314,
    },
    PacketMeta {
        offset: 8314,
        len: 310,
    },
    PacketMeta {
        offset: 8624,
        len: 311,
    },
    PacketMeta {
        offset: 8935,
        len: 304,
    },
    PacketMeta {
        offset: 9239,
        len: 307,
    },
    PacketMeta {
        offset: 9546,
        len: 300,
    },
    PacketMeta {
        offset: 9846,
        len: 298,
    },
    PacketMeta {
        offset: 10144,
        len: 320,
    },
    PacketMeta {
        offset: 10464,
        len: 308,
    },
    PacketMeta {
        offset: 10772,
        len: 306,
    },
    PacketMeta {
        offset: 11078,
        len: 310,
    },
    PacketMeta {
        offset: 11388,
        len: 309,
    },
    PacketMeta {
        offset: 11697,
        len: 311,
    },
    PacketMeta {
        offset: 12008,
        len: 304,
    },
    PacketMeta {
        offset: 12312,
        len: 304,
    },
    PacketMeta {
        offset: 12616,
        len: 305,
    },
    PacketMeta {
        offset: 12921,
        len: 305,
    },
    PacketMeta {
        offset: 13226,
        len: 303,
    },
    PacketMeta {
        offset: 13529,
        len: 319,
    },
    PacketMeta {
        offset: 13848,
        len: 313,
    },
    PacketMeta {
        offset: 14161,
        len: 312,
    },
    PacketMeta {
        offset: 14473,
        len: 306,
    },
    PacketMeta {
        offset: 14779,
        len: 311,
    },
    PacketMeta {
        offset: 15090,
        len: 311,
    },
    PacketMeta {
        offset: 15401,
        len: 309,
    },
    PacketMeta {
        offset: 15710,
        len: 303,
    },
    PacketMeta {
        offset: 16013,
        len: 315,
    },
    PacketMeta {
        offset: 16328,
        len: 309,
    },
    PacketMeta {
        offset: 16637,
        len: 309,
    },
    PacketMeta {
        offset: 16946,
        len: 313,
    },
    PacketMeta {
        offset: 17259,
        len: 306,
    },
    PacketMeta {
        offset: 17565,
        len: 307,
    },
    PacketMeta {
        offset: 17872,
        len: 307,
    },
    PacketMeta {
        offset: 18179,
        len: 304,
    },
    PacketMeta {
        offset: 18483,
        len: 310,
    },
    PacketMeta {
        offset: 18793,
        len: 303,
    },
    PacketMeta {
        offset: 19096,
        len: 303,
    },
    PacketMeta {
        offset: 19399,
        len: 310,
    },
    PacketMeta {
        offset: 19709,
        len: 304,
    },
    PacketMeta {
        offset: 20013,
        len: 300,
    },
    PacketMeta {
        offset: 20313,
        len: 299,
    },
    PacketMeta {
        offset: 20612,
        len: 299,
    },
    PacketMeta {
        offset: 20911,
        len: 313,
    },
    PacketMeta {
        offset: 21224,
        len: 307,
    },
    PacketMeta {
        offset: 21531,
        len: 308,
    },
    PacketMeta {
        offset: 21839,
        len: 313,
    },
    PacketMeta {
        offset: 22152,
        len: 313,
    },
    PacketMeta {
        offset: 22465,
        len: 309,
    },
    PacketMeta {
        offset: 22774,
        len: 308,
    },
    PacketMeta {
        offset: 23082,
        len: 304,
    },
    PacketMeta {
        offset: 23386,
        len: 302,
    },
    PacketMeta {
        offset: 23688,
        len: 311,
    },
    PacketMeta {
        offset: 23999,
        len: 303,
    },
    PacketMeta {
        offset: 24302,
        len: 314,
    },
    PacketMeta {
        offset: 24616,
        len: 311,
    },
    PacketMeta {
        offset: 24927,
        len: 311,
    },
    PacketMeta {
        offset: 25238,
        len: 314,
    },
    PacketMeta {
        offset: 25552,
        len: 302,
    },
    PacketMeta {
        offset: 25854,
        len: 307,
    },
    PacketMeta {
        offset: 26161,
        len: 301,
    },
    PacketMeta {
        offset: 26462,
        len: 305,
    },
    PacketMeta {
        offset: 26767,
        len: 313,
    },
    PacketMeta {
        offset: 27080,
        len: 311,
    },
    PacketMeta {
        offset: 27391,
        len: 309,
    },
    PacketMeta {
        offset: 27700,
        len: 313,
    },
    PacketMeta {
        offset: 28013,
        len: 308,
    },
    PacketMeta {
        offset: 28321,
        len: 312,
    },
    PacketMeta {
        offset: 28633,
        len: 307,
    },
    PacketMeta {
        offset: 28940,
        len: 306,
    },
    PacketMeta {
        offset: 29246,
        len: 306,
    },
    PacketMeta {
        offset: 29552,
        len: 306,
    },
    PacketMeta {
        offset: 29858,
        len: 303,
    },
    PacketMeta {
        offset: 30161,
        len: 316,
    },
    PacketMeta {
        offset: 30477,
        len: 315,
    },
    PacketMeta {
        offset: 30792,
        len: 310,
    },
    PacketMeta {
        offset: 31102,
        len: 310,
    },
    PacketMeta {
        offset: 31412,
        len: 305,
    },
    PacketMeta {
        offset: 31717,
        len: 310,
    },
    PacketMeta {
        offset: 32027,
        len: 311,
    },
    PacketMeta {
        offset: 32338,
        len: 304,
    },
    PacketMeta {
        offset: 32642,
        len: 313,
    },
    PacketMeta {
        offset: 32955,
        len: 316,
    },
    PacketMeta {
        offset: 33271,
        len: 310,
    },
    PacketMeta {
        offset: 33581,
        len: 310,
    },
    PacketMeta {
        offset: 33891,
        len: 308,
    },
    PacketMeta {
        offset: 34199,
        len: 308,
    },
    PacketMeta {
        offset: 34507,
        len: 308,
    },
    PacketMeta {
        offset: 34815,
        len: 302,
    },
    PacketMeta {
        offset: 35117,
        len: 304,
    },
    PacketMeta {
        offset: 35421,
        len: 304,
    },
    PacketMeta {
        offset: 35725,
        len: 306,
    },
    PacketMeta {
        offset: 36031,
        len: 307,
    },
    PacketMeta {
        offset: 36338,
        len: 302,
    },
    PacketMeta {
        offset: 36640,
        len: 300,
    },
    PacketMeta {
        offset: 36940,
        len: 296,
    },
    PacketMeta {
        offset: 37236,
        len: 297,
    },
    PacketMeta {
        offset: 37533,
        len: 312,
    },
    PacketMeta {
        offset: 37845,
        len: 308,
    },
    PacketMeta {
        offset: 38153,
        len: 308,
    },
    PacketMeta {
        offset: 38461,
        len: 317,
    },
    PacketMeta {
        offset: 38778,
        len: 316,
    },
    PacketMeta {
        offset: 39094,
        len: 304,
    },
    PacketMeta {
        offset: 39398,
        len: 310,
    },
    PacketMeta {
        offset: 39708,
        len: 304,
    },
    PacketMeta {
        offset: 40012,
        len: 310,
    },
    PacketMeta {
        offset: 40322,
        len: 304,
    },
    PacketMeta {
        offset: 40626,
        len: 306,
    },
    PacketMeta {
        offset: 40932,
        len: 316,
    },
    PacketMeta {
        offset: 41248,
        len: 314,
    },
    PacketMeta {
        offset: 41562,
        len: 310,
    },
    PacketMeta {
        offset: 41872,
        len: 308,
    },
    PacketMeta {
        offset: 42180,
        len: 308,
    },
    PacketMeta {
        offset: 42488,
        len: 309,
    },
    PacketMeta {
        offset: 42797,
        len: 304,
    },
    PacketMeta {
        offset: 43101,
        len: 302,
    },
    PacketMeta {
        offset: 43403,
        len: 313,
    },
    PacketMeta {
        offset: 43716,
        len: 307,
    },
    PacketMeta {
        offset: 44023,
        len: 309,
    },
    PacketMeta {
        offset: 44332,
        len: 310,
    },
    PacketMeta {
        offset: 44642,
        len: 310,
    },
    PacketMeta {
        offset: 44952,
        len: 305,
    },
    PacketMeta {
        offset: 45257,
        len: 307,
    },
    PacketMeta {
        offset: 45564,
        len: 309,
    },
    PacketMeta {
        offset: 45873,
        len: 307,
    },
    PacketMeta {
        offset: 46180,
        len: 302,
    },
    PacketMeta {
        offset: 46482,
        len: 302,
    },
    PacketMeta {
        offset: 46784,
        len: 317,
    },
    PacketMeta {
        offset: 47101,
        len: 310,
    },
    PacketMeta {
        offset: 47411,
        len: 307,
    },
    PacketMeta {
        offset: 47718,
        len: 307,
    },
    PacketMeta {
        offset: 48025,
        len: 309,
    },
    PacketMeta {
        offset: 48334,
        len: 308,
    },
    PacketMeta {
        offset: 48642,
        len: 304,
    },
    PacketMeta {
        offset: 48946,
        len: 305,
    },
    PacketMeta {
        offset: 49251,
        len: 317,
    },
    PacketMeta {
        offset: 49568,
        len: 310,
    },
    PacketMeta {
        offset: 49878,
        len: 310,
    },
    PacketMeta {
        offset: 50188,
        len: 309,
    },
    PacketMeta {
        offset: 50497,
        len: 306,
    },
    PacketMeta {
        offset: 50803,
        len: 306,
    },
    PacketMeta {
        offset: 51109,
        len: 305,
    },
    PacketMeta {
        offset: 51414,
        len: 302,
    },
    PacketMeta {
        offset: 51716,
        len: 307,
    },
    PacketMeta {
        offset: 52023,
        len: 306,
    },
    PacketMeta {
        offset: 52329,
        len: 308,
    },
    PacketMeta {
        offset: 52637,
        len: 305,
    },
    PacketMeta {
        offset: 52942,
        len: 302,
    },
    PacketMeta {
        offset: 53244,
        len: 298,
    },
    PacketMeta {
        offset: 53542,
        len: 303,
    },
    PacketMeta {
        offset: 53845,
        len: 296,
    },
    PacketMeta {
        offset: 54141,
        len: 314,
    },
    PacketMeta {
        offset: 54455,
        len: 309,
    },
    PacketMeta {
        offset: 54764,
        len: 306,
    },
    PacketMeta {
        offset: 55070,
        len: 312,
    },
    PacketMeta {
        offset: 55382,
        len: 309,
    },
    PacketMeta {
        offset: 55691,
        len: 311,
    },
    PacketMeta {
        offset: 56002,
        len: 309,
    },
    PacketMeta {
        offset: 56311,
        len: 306,
    },
    PacketMeta {
        offset: 56617,
        len: 304,
    },
    PacketMeta {
        offset: 56921,
        len: 305,
    },
    PacketMeta {
        offset: 57226,
        len: 302,
    },
    PacketMeta {
        offset: 57528,
        len: 320,
    },
    PacketMeta {
        offset: 57848,
        len: 312,
    },
    PacketMeta {
        offset: 58160,
        len: 315,
    },
    PacketMeta {
        offset: 58475,
        len: 310,
    },
    PacketMeta {
        offset: 58785,
        len: 305,
    },
    PacketMeta {
        offset: 59090,
        len: 314,
    },
    PacketMeta {
        offset: 59404,
        len: 305,
    },
    PacketMeta {
        offset: 59709,
        len: 299,
    },
    PacketMeta {
        offset: 60008,
        len: 313,
    },
    PacketMeta {
        offset: 60321,
        len: 311,
    },
    PacketMeta {
        offset: 60632,
        len: 307,
    },
    PacketMeta {
        offset: 60939,
        len: 312,
    },
    PacketMeta {
        offset: 61251,
        len: 308,
    },
    PacketMeta {
        offset: 61559,
        len: 307,
    },
    PacketMeta {
        offset: 61866,
        len: 307,
    },
    PacketMeta {
        offset: 62173,
        len: 303,
    },
    PacketMeta {
        offset: 62476,
        len: 307,
    },
    PacketMeta {
        offset: 62783,
        len: 307,
    },
    PacketMeta {
        offset: 63090,
        len: 304,
    },
    PacketMeta {
        offset: 63394,
        len: 313,
    },
    PacketMeta {
        offset: 63707,
        len: 309,
    },
    PacketMeta {
        offset: 64016,
        len: 308,
    },
    PacketMeta {
        offset: 64324,
        len: 308,
    },
    PacketMeta {
        offset: 64632,
        len: 303,
    },
    PacketMeta {
        offset: 64935,
        len: 309,
    },
    PacketMeta {
        offset: 65244,
        len: 308,
    },
    PacketMeta {
        offset: 65552,
        len: 305,
    },
    PacketMeta {
        offset: 65857,
        len: 318,
    },
    PacketMeta {
        offset: 66175,
        len: 312,
    },
    PacketMeta {
        offset: 66487,
        len: 310,
    },
    PacketMeta {
        offset: 66797,
        len: 307,
    },
    PacketMeta {
        offset: 67104,
        len: 311,
    },
    PacketMeta {
        offset: 67415,
        len: 303,
    },
    PacketMeta {
        offset: 67718,
        len: 308,
    },
    PacketMeta {
        offset: 68026,
        len: 303,
    },
    PacketMeta {
        offset: 68329,
        len: 306,
    },
    PacketMeta {
        offset: 68635,
        len: 305,
    },
    PacketMeta {
        offset: 68940,
        len: 303,
    },
    PacketMeta {
        offset: 69243,
        len: 306,
    },
    PacketMeta {
        offset: 69549,
        len: 302,
    },
    PacketMeta {
        offset: 69851,
        len: 303,
    },
    PacketMeta {
        offset: 70154,
        len: 298,
    },
    PacketMeta {
        offset: 70452,
        len: 300,
    },
    PacketMeta {
        offset: 70752,
        len: 313,
    },
    PacketMeta {
        offset: 71065,
        len: 312,
    },
    PacketMeta {
        offset: 71377,
        len: 306,
    },
    PacketMeta {
        offset: 71683,
        len: 315,
    },
    PacketMeta {
        offset: 71998,
        len: 308,
    },
    PacketMeta {
        offset: 72306,
        len: 307,
    },
    PacketMeta {
        offset: 72613,
        len: 315,
    },
    PacketMeta {
        offset: 72928,
        len: 305,
    },
    PacketMeta {
        offset: 73233,
        len: 305,
    },
    PacketMeta {
        offset: 73538,
        len: 305,
    },
    PacketMeta {
        offset: 73843,
        len: 305,
    },
    PacketMeta {
        offset: 74148,
        len: 315,
    },
    PacketMeta {
        offset: 74463,
        len: 315,
    },
    PacketMeta {
        offset: 74778,
        len: 310,
    },
    PacketMeta {
        offset: 75088,
        len: 311,
    },
    PacketMeta {
        offset: 75399,
        len: 308,
    },
    PacketMeta {
        offset: 75707,
        len: 307,
    },
    PacketMeta {
        offset: 76014,
        len: 306,
    },
    PacketMeta {
        offset: 76320,
        len: 305,
    },
    PacketMeta {
        offset: 76625,
        len: 315,
    },
    PacketMeta {
        offset: 76940,
        len: 310,
    },
    PacketMeta {
        offset: 77250,
        len: 307,
    },
    PacketMeta {
        offset: 77557,
        len: 312,
    },
    PacketMeta {
        offset: 77869,
        len: 310,
    },
    PacketMeta {
        offset: 78179,
        len: 305,
    },
    PacketMeta {
        offset: 78484,
        len: 305,
    },
    PacketMeta {
        offset: 78789,
        len: 303,
    },
    PacketMeta {
        offset: 79092,
        len: 305,
    },
    PacketMeta {
        offset: 79397,
        len: 307,
    },
    PacketMeta {
        offset: 79704,
        len: 303,
    },
    PacketMeta {
        offset: 80007,
        len: 313,
    },
    PacketMeta {
        offset: 80320,
        len: 305,
    },
    PacketMeta {
        offset: 80625,
        len: 308,
    },
    PacketMeta {
        offset: 80933,
        len: 305,
    },
    PacketMeta {
        offset: 81238,
        len: 304,
    },
    PacketMeta {
        offset: 81542,
        len: 312,
    },
    PacketMeta {
        offset: 81854,
        len: 306,
    },
    PacketMeta {
        offset: 82160,
        len: 306,
    },
    PacketMeta {
        offset: 82466,
        len: 313,
    },
    PacketMeta {
        offset: 82779,
        len: 312,
    },
    PacketMeta {
        offset: 83091,
        len: 306,
    },
    PacketMeta {
        offset: 83397,
        len: 310,
    },
    PacketMeta {
        offset: 83707,
        len: 304,
    },
    PacketMeta {
        offset: 84011,
        len: 312,
    },
    PacketMeta {
        offset: 84323,
        len: 306,
    },
    PacketMeta {
        offset: 84629,
        len: 307,
    },
    PacketMeta {
        offset: 84936,
        len: 308,
    },
    PacketMeta {
        offset: 85244,
        len: 309,
    },
    PacketMeta {
        offset: 85553,
        len: 304,
    },
    PacketMeta {
        offset: 85857,
        len: 304,
    },
    PacketMeta {
        offset: 86161,
        len: 308,
    },
    PacketMeta {
        offset: 86469,
        len: 300,
    },
    PacketMeta {
        offset: 86769,
        len: 298,
    },
    PacketMeta {
        offset: 87067,
        len: 298,
    },
    PacketMeta {
        offset: 87365,
        len: 317,
    },
    PacketMeta {
        offset: 87682,
        len: 307,
    },
    PacketMeta {
        offset: 87989,
        len: 307,
    },
    PacketMeta {
        offset: 88296,
        len: 311,
    },
    PacketMeta {
        offset: 88607,
        len: 310,
    },
    PacketMeta {
        offset: 88917,
        len: 308,
    },
    PacketMeta {
        offset: 89225,
        len: 307,
    },
    PacketMeta {
        offset: 89532,
        len: 306,
    },
    PacketMeta {
        offset: 89838,
        len: 307,
    },
    PacketMeta {
        offset: 90145,
        len: 306,
    },
    PacketMeta {
        offset: 90451,
        len: 304,
    },
    PacketMeta {
        offset: 90755,
        len: 318,
    },
    PacketMeta {
        offset: 91073,
        len: 315,
    },
    PacketMeta {
        offset: 91388,
        len: 315,
    },
    PacketMeta {
        offset: 91703,
        len: 312,
    },
    PacketMeta {
        offset: 92015,
        len: 311,
    },
    PacketMeta {
        offset: 92326,
        len: 307,
    },
    PacketMeta {
        offset: 92633,
        len: 303,
    },
    PacketMeta {
        offset: 92936,
        len: 307,
    },
    PacketMeta {
        offset: 93243,
        len: 313,
    },
    PacketMeta {
        offset: 93556,
        len: 310,
    },
    PacketMeta {
        offset: 93866,
        len: 305,
    },
    PacketMeta {
        offset: 94171,
        len: 312,
    },
    PacketMeta {
        offset: 94483,
        len: 309,
    },
    PacketMeta {
        offset: 94792,
        len: 305,
    },
    PacketMeta {
        offset: 95097,
        len: 308,
    },
    PacketMeta {
        offset: 95405,
        len: 303,
    },
    PacketMeta {
        offset: 95708,
        len: 304,
    },
    PacketMeta {
        offset: 96012,
        len: 302,
    },
    PacketMeta {
        offset: 96314,
        len: 304,
    },
    PacketMeta {
        offset: 96618,
        len: 313,
    },
    PacketMeta {
        offset: 96931,
        len: 307,
    },
    PacketMeta {
        offset: 97238,
        len: 304,
    },
    PacketMeta {
        offset: 97542,
        len: 307,
    },
    PacketMeta {
        offset: 97849,
        len: 302,
    },
    PacketMeta {
        offset: 98151,
        len: 314,
    },
    PacketMeta {
        offset: 98465,
        len: 309,
    },
    PacketMeta {
        offset: 98774,
        len: 307,
    },
    PacketMeta {
        offset: 99081,
        len: 313,
    },
    PacketMeta {
        offset: 99394,
        len: 310,
    },
    PacketMeta {
        offset: 99704,
        len: 317,
    },
    PacketMeta {
        offset: 100021,
        len: 309,
    },
    PacketMeta {
        offset: 100330,
        len: 306,
    },
    PacketMeta {
        offset: 100636,
        len: 306,
    },
    PacketMeta {
        offset: 100942,
        len: 308,
    },
    PacketMeta {
        offset: 101250,
        len: 302,
    },
    PacketMeta {
        offset: 101552,
        len: 310,
    },
    PacketMeta {
        offset: 101862,
        len: 306,
    },
    PacketMeta {
        offset: 102168,
        len: 305,
    },
    PacketMeta {
        offset: 102473,
        len: 307,
    },
    PacketMeta {
        offset: 102780,
        len: 298,
    },
    PacketMeta {
        offset: 103078,
        len: 304,
    },
    PacketMeta {
        offset: 103382,
        len: 301,
    },
    PacketMeta {
        offset: 103683,
        len: 297,
    },
    PacketMeta {
        offset: 103980,
        len: 316,
    },
    PacketMeta {
        offset: 104296,
        len: 309,
    },
    PacketMeta {
        offset: 104605,
        len: 309,
    },
    PacketMeta {
        offset: 104914,
        len: 315,
    },
    PacketMeta {
        offset: 105229,
        len: 309,
    },
    PacketMeta {
        offset: 105538,
        len: 309,
    },
    PacketMeta {
        offset: 105847,
        len: 309,
    },
    PacketMeta {
        offset: 106156,
        len: 304,
    },
    PacketMeta {
        offset: 106460,
        len: 309,
    },
    PacketMeta {
        offset: 106769,
        len: 305,
    },
    PacketMeta {
        offset: 107074,
        len: 304,
    },
    PacketMeta {
        offset: 107378,
        len: 320,
    },
    PacketMeta {
        offset: 107698,
        len: 316,
    },
    PacketMeta {
        offset: 108014,
        len: 318,
    },
    PacketMeta {
        offset: 108332,
        len: 310,
    },
    PacketMeta {
        offset: 108642,
        len: 311,
    },
    PacketMeta {
        offset: 108953,
        len: 307,
    },
    PacketMeta {
        offset: 109260,
        len: 305,
    },
    PacketMeta {
        offset: 109565,
        len: 300,
    },
    PacketMeta {
        offset: 109865,
        len: 316,
    },
    PacketMeta {
        offset: 110181,
        len: 312,
    },
    PacketMeta {
        offset: 110493,
        len: 309,
    },
    PacketMeta {
        offset: 110802,
        len: 309,
    },
    PacketMeta {
        offset: 111111,
        len: 310,
    },
    PacketMeta {
        offset: 111421,
        len: 306,
    },
    PacketMeta {
        offset: 111727,
        len: 310,
    },
    PacketMeta {
        offset: 112037,
        len: 303,
    },
    PacketMeta {
        offset: 112340,
        len: 306,
    },
    PacketMeta {
        offset: 112646,
        len: 301,
    },
    PacketMeta {
        offset: 112947,
        len: 302,
    },
    PacketMeta {
        offset: 113249,
        len: 318,
    },
    PacketMeta {
        offset: 113567,
        len: 303,
    },
    PacketMeta {
        offset: 113870,
        len: 305,
    },
    PacketMeta {
        offset: 114175,
        len: 299,
    },
    PacketMeta {
        offset: 114474,
        len: 299,
    },
    PacketMeta {
        offset: 114773,
        len: 310,
    },
    PacketMeta {
        offset: 115083,
        len: 309,
    },
    PacketMeta {
        offset: 115392,
        len: 305,
    },
    PacketMeta {
        offset: 115697,
        len: 314,
    },
    PacketMeta {
        offset: 116011,
        len: 312,
    },
    PacketMeta {
        offset: 116323,
        len: 306,
    },
    PacketMeta {
        offset: 116629,
        len: 310,
    },
    PacketMeta {
        offset: 116939,
        len: 309,
    },
    PacketMeta {
        offset: 117248,
        len: 305,
    },
    PacketMeta {
        offset: 117553,
        len: 308,
    },
    PacketMeta {
        offset: 117861,
        len: 303,
    },
    PacketMeta {
        offset: 118164,
        len: 313,
    },
    PacketMeta {
        offset: 118477,
        len: 310,
    },
    PacketMeta {
        offset: 118787,
        len: 307,
    },
    PacketMeta {
        offset: 119094,
        len: 310,
    },
    PacketMeta {
        offset: 119404,
        len: 300,
    },
    PacketMeta {
        offset: 119704,
        len: 301,
    },
    PacketMeta {
        offset: 120005,
        len: 305,
    },
    PacketMeta {
        offset: 120310,
        len: 303,
    },
    PacketMeta {
        offset: 120613,
        len: 311,
    },
    PacketMeta {
        offset: 120924,
        len: 307,
    },
    PacketMeta {
        offset: 121231,
        len: 307,
    },
    PacketMeta {
        offset: 121538,
        len: 316,
    },
    PacketMeta {
        offset: 121854,
        len: 308,
    },
    PacketMeta {
        offset: 122162,
        len: 309,
    },
    PacketMeta {
        offset: 122471,
        len: 306,
    },
    PacketMeta {
        offset: 122777,
        len: 303,
    },
    PacketMeta {
        offset: 123080,
        len: 304,
    },
    PacketMeta {
        offset: 123384,
        len: 307,
    },
    PacketMeta {
        offset: 123691,
        len: 306,
    },
    PacketMeta {
        offset: 123997,
        len: 319,
    },
    PacketMeta {
        offset: 124316,
        len: 312,
    },
    PacketMeta {
        offset: 124628,
        len: 313,
    },
    PacketMeta {
        offset: 124941,
        len: 312,
    },
    PacketMeta {
        offset: 125253,
        len: 308,
    },
    PacketMeta {
        offset: 125561,
        len: 309,
    },
    PacketMeta {
        offset: 125870,
        len: 309,
    },
    PacketMeta {
        offset: 126179,
        len: 302,
    },
    PacketMeta {
        offset: 126481,
        len: 314,
    },
    PacketMeta {
        offset: 126795,
        len: 317,
    },
    PacketMeta {
        offset: 127112,
        len: 304,
    },
    PacketMeta {
        offset: 127416,
        len: 311,
    },
    PacketMeta {
        offset: 127727,
        len: 306,
    },
    PacketMeta {
        offset: 128033,
        len: 306,
    },
    PacketMeta {
        offset: 128339,
        len: 306,
    },
    PacketMeta {
        offset: 128645,
        len: 303,
    },
    PacketMeta {
        offset: 128948,
        len: 305,
    },
    PacketMeta {
        offset: 129253,
        len: 303,
    },
    PacketMeta {
        offset: 129556,
        len: 303,
    },
    PacketMeta {
        offset: 129859,
        len: 306,
    },
    PacketMeta {
        offset: 130165,
        len: 307,
    },
    PacketMeta {
        offset: 130472,
        len: 303,
    },
    PacketMeta {
        offset: 130775,
        len: 302,
    },
    PacketMeta {
        offset: 131077,
        len: 299,
    },
    PacketMeta {
        offset: 131376,
        len: 315,
    },
    PacketMeta {
        offset: 131691,
        len: 309,
    },
    PacketMeta {
        offset: 132000,
        len: 308,
    },
    PacketMeta {
        offset: 132308,
        len: 315,
    },
    PacketMeta {
        offset: 132623,
        len: 313,
    },
    PacketMeta {
        offset: 132936,
        len: 307,
    },
    PacketMeta {
        offset: 133243,
        len: 309,
    },
    PacketMeta {
        offset: 133552,
        len: 310,
    },
    PacketMeta {
        offset: 133862,
        len: 309,
    },
    PacketMeta {
        offset: 134171,
        len: 305,
    },
    PacketMeta {
        offset: 134476,
        len: 302,
    },
    PacketMeta {
        offset: 134778,
        len: 312,
    },
    PacketMeta {
        offset: 135090,
        len: 313,
    },
    PacketMeta {
        offset: 135403,
        len: 306,
    },
    PacketMeta {
        offset: 135709,
        len: 309,
    },
    PacketMeta {
        offset: 136018,
        len: 300,
    },
    PacketMeta {
        offset: 136318,
        len: 305,
    },
    PacketMeta {
        offset: 136623,
        len: 300,
    },
    PacketMeta {
        offset: 136923,
        len: 302,
    },
    PacketMeta {
        offset: 137225,
        len: 316,
    },
    PacketMeta {
        offset: 137541,
        len: 311,
    },
    PacketMeta {
        offset: 137852,
        len: 306,
    },
    PacketMeta {
        offset: 138158,
        len: 312,
    },
    PacketMeta {
        offset: 138470,
        len: 310,
    },
    PacketMeta {
        offset: 138780,
        len: 307,
    },
    PacketMeta {
        offset: 139087,
        len: 308,
    },
    PacketMeta {
        offset: 139395,
        len: 309,
    },
    PacketMeta {
        offset: 139704,
        len: 305,
    },
    PacketMeta {
        offset: 140009,
        len: 306,
    },
    PacketMeta {
        offset: 140315,
        len: 310,
    },
    PacketMeta {
        offset: 140625,
        len: 315,
    },
    PacketMeta {
        offset: 140940,
        len: 315,
    },
    PacketMeta {
        offset: 141255,
        len: 310,
    },
    PacketMeta {
        offset: 141565,
        len: 309,
    },
    PacketMeta {
        offset: 141874,
        len: 310,
    },
    PacketMeta {
        offset: 142184,
        len: 307,
    },
    PacketMeta {
        offset: 142491,
        len: 305,
    },
    PacketMeta {
        offset: 142796,
        len: 302,
    },
    PacketMeta {
        offset: 143098,
        len: 315,
    },
    PacketMeta {
        offset: 143413,
        len: 309,
    },
    PacketMeta {
        offset: 143722,
        len: 310,
    },
    PacketMeta {
        offset: 144032,
        len: 313,
    },
    PacketMeta {
        offset: 144345,
        len: 309,
    },
    PacketMeta {
        offset: 144654,
        len: 306,
    },
    PacketMeta {
        offset: 144960,
        len: 311,
    },
    PacketMeta {
        offset: 145271,
        len: 306,
    },
    PacketMeta {
        offset: 145577,
        len: 307,
    },
    PacketMeta {
        offset: 145884,
        len: 305,
    },
    PacketMeta {
        offset: 146189,
        len: 306,
    },
    PacketMeta {
        offset: 146495,
        len: 308,
    },
    PacketMeta {
        offset: 146803,
        len: 301,
    },
    PacketMeta {
        offset: 147104,
        len: 305,
    },
    PacketMeta {
        offset: 147409,
        len: 298,
    },
    PacketMeta {
        offset: 147707,
        len: 297,
    },
    PacketMeta {
        offset: 148004,
        len: 311,
    },
    PacketMeta {
        offset: 148315,
        len: 308,
    },
    PacketMeta {
        offset: 148623,
        len: 308,
    },
    PacketMeta {
        offset: 148931,
        len: 313,
    },
    PacketMeta {
        offset: 149244,
        len: 312,
    },
    PacketMeta {
        offset: 149556,
        len: 308,
    },
    PacketMeta {
        offset: 149864,
        len: 309,
    },
    PacketMeta {
        offset: 150173,
        len: 303,
    },
    PacketMeta {
        offset: 150476,
        len: 304,
    },
    PacketMeta {
        offset: 150780,
        len: 310,
    },
    PacketMeta {
        offset: 151090,
        len: 305,
    },
    PacketMeta {
        offset: 151395,
        len: 310,
    },
    PacketMeta {
        offset: 151705,
        len: 312,
    },
    PacketMeta {
        offset: 152017,
        len: 311,
    },
    PacketMeta {
        offset: 152328,
        len: 311,
    },
    PacketMeta {
        offset: 152639,
        len: 305,
    },
    PacketMeta {
        offset: 152944,
        len: 307,
    },
    PacketMeta {
        offset: 153251,
        len: 302,
    },
    PacketMeta {
        offset: 153553,
        len: 300,
    },
    PacketMeta {
        offset: 153853,
        len: 480,
    },
];

/// opus_music — config 31 CELT-only FB 20 ms x501 packets (code 0, one frame each).
pub const OPUS_MUSIC: OpusClip = OpusClip {
    name: "opus_music",
    why: "music arm: libopus -application audio at API 48 kHz FB 20 ms — the mode libopus actually emits at this profile (census records it)",
    census: "config 31 CELT-only FB 20 ms x501 packets (code 0, one frame each)",
    toc_config: 31,
    sample_rate_hz: 48000,
    frame_samples: 960,
    channels: 1,
    pre_skip: 312,
    align_shift: 312,
    fold_mismatch: 126,
    packets: 501,
    walk_samples: 480960,
    ref_bytes_len: 1920000,
    fnv_packets: 0x24e4329875944a91,
    fnv_ref_pcm: 0xb11a8c59f1f26183,
    fnv_walk_fold: 0xa781907825ab00aa,
    ref_file: "opus_music_ref.bin",
    index: OPUS_MUSIC_INDEX,
    region: include_bytes!("../assets/opus_music_packets.bin"),
};

/// Both arms, in gate order (speech first — the decision hinges on it).
pub const CLIPS: &[&OpusClip] = &[&OPUS_SILK, &OPUS_MUSIC];
