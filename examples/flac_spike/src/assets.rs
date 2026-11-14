//! flac_spike embedded clips — GENERATED
//! DO NOT EDIT BY HAND.  Regenerate:  scripts/gen_spike_assets.sh
//! Generator: scripts/frame_vectors.py spike_assets (strict frame finder,
//! fail-closed arm censuses; FLAC.md 'Perf gate spike' validation rules).
//! Encoder: flac 1.5.0
//!
//! sha256 pins (regeneration tripwires; the witness tests carry the
//! FNV-1a pins in code, these are for humans reviewing a regen diff):
//!   spike_l0_frames.bin      region 2e3560f2210004ed6fa4c5636755be17cc2c9e64f2613d451e5f445b58e1ba0a
//!   spike_l0_pcm.bin         pcm    db90ccc22b1146d5e2f5968d80a5fe849552b7d5c9c77583ed2d04f09b952395
//!   census(l0_stereo.flac): slot 0: fixed0 x80 | slot 1: fixed0 x80
//!   spike_l4_frames.bin      region 2feea050ab0df2f09776433f18debb3d52cac86e28843f014b12c33ca65fbb31
//!   spike_l4_pcm.bin         pcm    db90ccc22b1146d5e2f5968d80a5fe849552b7d5c9c77583ed2d04f09b952395
//!   census(l4_stereo.flac): slot 0: lpc3 x1, lpc4 x79 | slot 1: lpc3 x1, lpc4 x79
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
    census: "slot 0: fixed0 x80 | slot 1: fixed0 x80",
    sample_rate_hz: 16384,
    bits_per_sample: 16,
    channels: 2,
    total_samples: 162304,
    max_blocksize: 2048,
    frames: &[
        FrameMeta {
            offset: 0,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 5540,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 11078,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 16619,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 22156,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 27698,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 33235,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 38777,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 44315,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 49855,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 55396,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 60936,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 66477,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 72015,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 77557,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 83095,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 88637,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 94175,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 99715,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 105255,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 110794,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 116334,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 121872,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 127414,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 132952,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 138493,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 144031,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 149569,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 155108,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 160648,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 166187,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 171726,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 177267,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 182804,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 188347,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 193886,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 199425,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 204965,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 210505,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 216045,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 221584,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 227125,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 232664,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 238207,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 243747,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 249287,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 254829,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 260368,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 265908,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 271448,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 276989,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 282527,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 288068,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 293609,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 299149,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 304689,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 310228,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 315768,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 321307,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 326847,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 332386,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 337925,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 343465,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 349005,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 354546,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 360085,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 365627,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 371167,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 376707,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 382247,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 387787,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 393327,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 398866,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 404407,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 409947,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 415489,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 421029,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 426571,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 432111,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 437651,
            blocksize: 512,
        },
    ],
    region: include_bytes!("../assets/spike_l0_frames.bin"),
    pcm_file: "spike_l0_pcm.bin",
    fnv_frames: 0x9CDABFFC3C1F90D4,
    fnv_pcm: 0xEB3EA1C24FE1574F,
};

/// LPC arm (flac -1 -l 4 -b 2048 -m): the gate's full-profile cost (majority LPC — FLAC.md Correction 3)
pub const L4_LPC: SpikeClip = SpikeClip {
    name: "l4_stereo",
    why: "LPC arm (flac -1 -l 4 -b 2048 -m): the gate's full-profile cost (majority LPC — FLAC.md Correction 3)",
    census: "slot 0: lpc3 x1, lpc4 x79 | slot 1: lpc3 x1, lpc4 x79",
    sample_rate_hz: 16384,
    bits_per_sample: 16,
    channels: 2,
    total_samples: 162304,
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
            offset: 10692,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 16046,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 21395,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 26749,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 32098,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 37451,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 42803,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 48155,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 53501,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 58849,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 64200,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 69549,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 74903,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 80251,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 85604,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 90958,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 96308,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 101660,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 107011,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 112362,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 117712,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 123066,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 128415,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 133767,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 139119,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 144471,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 149822,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 155173,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 160524,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 165875,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 171228,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 176575,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 181928,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 187277,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 192630,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 197981,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 203334,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 208683,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 214034,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 219386,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 224736,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 230087,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 235438,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 240790,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 246142,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 251492,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 256844,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 262195,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 267546,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 272898,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 278248,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 283598,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 288950,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 294301,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 299652,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 305002,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 310355,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 315707,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 321057,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 326408,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 331762,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 337111,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 342464,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 347815,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 353166,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 358517,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 363871,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 369221,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 374573,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 379928,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 385278,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 390634,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 395982,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 401333,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 406683,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 412036,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 417386,
            blocksize: 2048,
        },
        FrameMeta {
            offset: 422736,
            blocksize: 512,
        },
    ],
    region: include_bytes!("../assets/spike_l4_frames.bin"),
    pcm_file: "spike_l4_pcm.bin",
    fnv_frames: 0xB90F81DB99611CAD,
    fnv_pcm: 0xEB3EA1C24FE1574F,
};

/// Both arms, in gate order (FIXED first — the arm a decision defaults to).
pub const CLIPS: [&SpikeClip; 2] = [&L0_FIXED, &L4_LPC];
