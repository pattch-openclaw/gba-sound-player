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
//! PR 1 pinned the contract (constructor set, rejection cases, the
//! decoder-state size, the derived budget). Step 2 adds [`decode_clip`]: the
//! sequential manifest walk the host witness drives against the ffmpeg/libopus
//! reference — and the same walk the ROM decode proof will run on-target.

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

/// Walk one embedded clip: a fresh decoder at the clip's pinned rate,
/// packets strictly in manifest order, each decoded window handed to
/// `on_window`. This is the product-shape driver — the Opus analogue of
/// flac_spike's `driver::decode_clip`, with the deliberate difference
/// OPUS.md item 3 records: Opus decode is a state machine, so the walk is
/// sequential per clip and the (offset, len) index witnesses the product
/// manifest, never seek independence.
///
/// The window callback receives exactly `frame_samples(rate)` samples at
/// the API rate; the fold (`crate::checksum::fold_f32_window`) rides on
/// top of this seam, never inside it, so the cycle harness can wrap
/// [`decode_packet`] alone without fold cost leaking into the window.
///
/// The sequential walk over one clip's manifest — the production driver,
/// delegating the geometry and tiling to [`walk_region`] so the asset
/// generator's pin-measuring example (`examples/dump_walk.rs`) and the
/// witness drive the exact same transcription of the decode loop.
pub fn decode_clip(
    clip: &crate::assets::OpusClip,
    on_window: impl FnMut(&[f32]),
) -> Result<(), (&'static str, usize)> {
    walk_region(
        clip.region,
        clip.index.iter().map(|e| (e.offset, e.len)),
        clip.sample_rate_hz,
        clip.channels,
        on_window,
    )
}

/// Walk a packet region by (offset, len) entries: a fresh decoder, packets
/// strictly in table order, each decoded window handed to `on_window`. The
/// Opus analogue of flac_spike's `driver::decode_clip`, with the deliberate
/// difference OPUS.md item 3 records: Opus decode is a state machine, so the
/// walk is sequential and the (offset, len) table witnesses the product
/// manifest, never seek independence.
///
/// The window callback receives exactly `frame_samples(rate)` samples at the
/// API rate; the fold (`crate::checksum::fold_f32_window`) rides on top of
/// this seam, never inside it, so the cycle harness can wrap [`decode_packet`]
/// alone without fold cost leaking into the window.
///
/// Fails closed on manifest/region drift before touching the decoder: the
/// entries must start at 0, ascend contiguously (no gaps, no overlaps), carry
/// no empty packet, and tile the region exactly — every violation is a named
/// error at the offending index, never a decoder panic or a silent skip.
///
/// What this driver deliberately does NOT try to catch: a tiling-preserving
/// re-split of the region (entries consistent with each other but not with
/// the encoder's packet boundaries). The decoder cannot be the guard either
/// way — measured 2026-10-02 against the vendored decoder: a first packet
/// truncated by 16 bytes (30 B -> 14 B) still returns `Ok(960)`, so a lying
/// length that stays inside the region would decode silently. Manifest
/// integrity is the driver's contract; content corruption is the decode-
/// versus-reference layer's net (tests/opus_witness.rs, layer 2: a re-split
/// changes the window count, which the walk-length pin catches). An earlier
/// draft guarded this with a per-entry `packet.len() != entry.len` check —
/// tautological, since the slice is built from `entry.len`; it accepted
/// every lying length inside the region (the witness's negative control
/// caught it by returning `Ok`).
pub fn walk_region(
    region: &[u8],
    entries: impl Iterator<Item = (u32, u16)> + Clone,
    sample_rate_hz: u32,
    channels: u8,
    mut on_window: impl FnMut(&[f32]),
) -> Result<(), (&'static str, usize)> {
    let mut window = [0.0f32; 1920]; // 40 ms x mono at 48 kHz — 2x the pinned frame
    let window_samples = frame_samples(sample_rate_hz);
    if window_samples > window.len() {
        // Generated clips pin 48 kHz (960); anything wider is drift to name,
        // not a silent slice panic.
        return Err(("pinned geometry exceeds window buffer", 0));
    }
    // Manifest tiling, checked before constructing the decoder: one cursor
    // advances through the entries, so offset drift, gaps, overlaps, empty
    // packets, and past-region entries each fail at their own index, and a
    // final cursor != region length names a table that lies about coverage.
    let region_len = region.len();
    let mut cursor = 0usize;
    let mut count = 0usize;
    for (i, (offset, len)) in entries.clone().enumerate() {
        count = i + 1;
        if len == 0 {
            return Err(("manifest empty packet", i));
        }
        if offset as usize != cursor {
            return Err(("manifest gap/overlap", i));
        }
        let end = cursor + len as usize;
        if end > region_len {
            return Err(("manifest runs past region", i));
        }
        cursor = end;
    }
    if cursor != region_len {
        // Blame the last entry: it is the one whose coverage leaves the
        // cursor off the region end (with an empty index, index 0 is the
        // only index there is).
        return Err(("manifest does not tile region", count - 1));
    }
    let mut decoder = new_decoder(sample_rate_hz as i32, channels as usize).map_err(|e| (e, 0))?;
    for (i, (offset, len)) in entries.enumerate() {
        let start = offset as usize;
        // Slice safe: the tiling pass proved every window lies inside the region.
        let packet = &region[start..start + len as usize];
        let out = &mut window[..window_samples];
        let decoded = decode_packet(&mut decoder, sample_rate_hz as i32, packet, out)
            .map_err(|_| ("decode rejected packet", i))?;
        if decoded != window_samples {
            return Err(("decoded geometry drift", i));
        }
        on_window(out);
    }
    Ok(())
}
