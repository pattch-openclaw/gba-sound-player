//! The decoder seam — the single place this crate touches `opus-rs`.
//!
//! Pattern of flac_spike's `driver`: the ROM entry and the host tests consume
//! the *same* functions, so the code the later decode proof and cycle harness
//! measure is the code the host witness exercised. Unlike FLAC, Opus decode is
//! a **state machine** — packets are not independently decodable — so
//! [`decode_packet`] takes a `&mut OpusDecoder` the caller owns and keeps
//! across packets: the future `decode_clip` walk is sequential per clip with a
//! fresh decoder, and the packet index is the product manifest, not a
//! seek-independence witness (OPUS.md, measurement plan item 3).
//!
//! PR 1 pins the contract only (constructor set, rejection cases, the
//! decoder-state size, the derived budget); decoding the embedded arms against
//! the ffmpeg/libopus reference is the next PR's witness (step 2).

/// The vendored, patched decoder re-exported so host tests and the ROM use
/// exactly one type — the same shared-consumer rule as flac_spike's fold.
pub use opus_rs::OpusDecoder;

/// Sample rates the Opus API accepts — RFC 6716 §2 and source-verified in
/// `OpusDecoder::new` (OPUS.md: 48 kHz is the format ceiling; "fullband" is
/// bandwidth, not rate). The constructor contract test iterates these.
pub const SUPPORTED_RATES_HZ: [i32; 5] = [8000, 12000, 16000, 24000, 48000];

/// The pinned frame duration for the product profile: 20 ms (OPUS.md —
/// packets carry one frame each, pinned at pack time).
pub const FRAME_DURATION_MS: u32 = 20;

/// Samples per decoded frame at the API rate `rate_hz` for the pinned 20 ms
/// duration (960 at 48 kHz). `const fn` so callers evaluate at compile time.
pub const fn frame_samples(rate_hz: u32) -> usize {
    (rate_hz as usize * FRAME_DURATION_MS as usize) / 1000
}

/// The GBA machine clock: 16.78 MHz — same reference flac_spike's harness
/// counts against.
pub const GBA_CLOCK_HZ: u64 = 16_780_000;

/// Real-time allowance in CPU cycles for one pinned 20 ms packet:
/// 20/1000 s × 16,780,000 Hz = **335,600 cycles** — derived, not recalled
/// (OPUS.md measurement plan item 6; against flac-lite's measured ~1,113
/// cycles/sample FIXED, this is ≈350 cycles/sample at 960 samples).
pub const BUDGET_CYCLES_PER_PACKET: u32 = (GBA_CLOCK_HZ * FRAME_DURATION_MS as u64 / 1000) as u32;

/// Inline size of the decoder state with `heap` off (the feature is
/// std-coupled upstream — OPUS.md gotcha 1). This is the number the EWRAM
/// plan is argued against; the host test pins the measured value, and the ROM
/// logs it. The Box-via-`alloc` patch (state into EWRAM heap) is a later
/// PR's problem — fine for host witnesses and the compile/link gate.
pub const DECODER_STATE_BYTES: usize = size_of::<OpusDecoder>();

/// Construct a decoder at the API rate and channel count. The contract is
/// `opus-rs`'s own (OPUS.md): the five rates × mono/stereo succeed, nothing
/// else.
pub fn new_decoder(rate_hz: i32, channels: usize) -> Result<OpusDecoder, &'static str> {
    OpusDecoder::new(rate_hz, channels)
}

/// Decode exactly one packet through a caller-owned decoder, writing
/// interleaved f32 at the API rate. Returns samples decoded per channel.
///
/// `frame_size` is the caller's capacity in samples/channel: this seam
/// derives it from the pinned profile (`frame_samples(decoder_api_rate)`,
/// the 20 ms manifest pin) instead of a caller guess, so a packet whose TOC
/// carries a different geometry trips opus-rs's own guards ("frame_size too
/// small" / "Output buffer too small") — the manifest-vs-TOC mismatch is the
/// next PR's fail-closed witness, not this seam's assumption. (Measured in
/// vendored source: `decode` rejects `frame_size == 0` outright — the TOC
/// decides the *actual* decode, but the caller must still carry ≥ that much.
/// A `frame_size = 0` "let the TOC decide" convention does not exist; this
/// comment records the reading of the source, not a recollection.)
///
/// This is the window the cycle harness will wrap: decode + the decoder's own
/// resampler together (OPUS.md measurement plan item 5 — the internal
/// resampler is part of the product cost).
pub fn decode_packet(
    decoder: &mut OpusDecoder,
    rate_hz: i32,
    packet: &[u8],
    output: &mut [f32],
) -> Result<usize, &'static str> {
    let frame_size = frame_samples(rate_hz as u32);
    decoder.decode(packet, frame_size, output)
}
